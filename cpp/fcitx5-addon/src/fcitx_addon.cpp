#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/fcitx_selection.h"

#ifdef VINPUT_FCITX_HAVE_CLIPBOARD
#include "clipboard_public.h"
#include <fcitx-utils/utf8.h>
#endif

#include <fcitx-utils/log.h>
#include <fcitx/addonmanager.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/surroundingtext.h>

#include <utility>

namespace vinput_fcitx_bridge {
namespace {

#ifdef VINPUT_FCITX_HAVE_CLIPBOARD
std::string PrimarySelectionFromClipboard(fcitx::Instance *instance,
                                          fcitx::InputContext *ic) {
  if (instance == nullptr || ic == nullptr) {
    return {};
  }
  auto *clipboard = instance->addonManager().addon("clipboard");
  if (clipboard == nullptr) {
    return {};
  }
  auto primary = clipboard->call<fcitx::IClipboard::primary>(ic);
  if (!fcitx::utf8::validate(primary)) {
    return {};
  }
  return primary;
}
#else
std::string PrimarySelectionFromClipboard(fcitx::Instance *, fcitx::InputContext *) {
  return {};
}
#endif

std::string SelectedTextFromInputContext(fcitx::Instance *instance,
                                         fcitx::InputContext *ic) {
  if (ic == nullptr) {
    return {};
  }
  return SelectedTextWithPrimaryFallback(ic->surroundingText(),
                                         PrimarySelectionFromClipboard(instance, ic));
}
std::string_view TriggerActionName(FcitxTriggerAction action) {
  switch (action) {
  case FcitxTriggerAction::None:
    return "none";
  case FcitxTriggerAction::StartNormal:
    return "start-normal";
  case FcitxTriggerAction::StopNormal:
    return "stop-normal";
  case FcitxTriggerAction::StartCommand:
    return "start-command";
  case FcitxTriggerAction::StopCommand:
    return "stop-command";
  }
  return "unknown";
}

void RequestSurroundingText(fcitx::Event &event) {
  auto &ic_event = static_cast<fcitx::InputContextEvent &>(event);
  auto *ic = ic_event.inputContext();
  if (ic == nullptr) {
    return;
  }
  ic->setCapabilityFlags(ic->capabilityFlags() |
                         fcitx::CapabilityFlag::SurroundingText);
}

} // namespace

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance)
    : instance_(instance), trigger_policy_(FcitxKeyTriggerPolicy::FromEnvironment()) {
  FCITX_INFO() << "fcitx-vinput addon loaded with normal trigger "
               << trigger_policy_.normal_trigger() << " and command trigger "
               << trigger_policy_.command_trigger();
  if (instance_ != nullptr) {
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PostInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextCreated, fcitx::EventWatcherPhase::PreInputMethod,
        RequestSurroundingText));
  }
}

SdBusDaemonClient *FcitxVinputAddon::EnsureDaemonClient(std::string *error) {
  if (daemon_client_ == nullptr) {
    daemon_client_ = SdBusDaemonClient::ConnectSession(error);
  }
  return daemon_client_.get();
}

AppliedOutcome FcitxVinputAddon::ApplyDaemonUnavailable(fcitx::InputContext *ic,
                                                        std::string error) {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Error;
  outcome.text =
      error.empty() ? "Voice input daemon is unavailable." : std::move(error);
  FCITX_WARN() << "fcitx-vinput daemon unavailable: " << outcome.text;
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyBridgeOutcome(fcitx::InputContext *ic,
                                                    const BridgeOutcome &outcome) {
  if (outcome.kind == BridgeOutcome::Kind::Error) {
    daemon_client_.reset();
  }
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

AppliedOutcome FcitxVinputAddon::TriggerNormal(fcitx::InputContext *ic,
                                               std::string_view scene_id) {
  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording() ? bridge_.Stop(client, scene_id)
                                     : bridge_.StartNormal(client, scene_id);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::TriggerCommand(fcitx::InputContext *ic,
                                                std::string_view selected_text,
                                                std::string_view scene_id) {
  if (!bridge_.recording() && selected_text.empty()) {
    return ApplyBridgeOutcome(ic,
                              bridge_.StartCommand(nullptr, selected_text, scene_id));
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording()
                     ? bridge_.Stop(client, scene_id)
                     : bridge_.StartCommand(client, selected_text, scene_id);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyTriggerAction(fcitx::InputContext *ic,
                                                    FcitxTriggerAction action,
                                                    std::string_view selected_text) {
  switch (action) {
  case FcitxTriggerAction::None:
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartNormal:
    if (!bridge_.recording()) {
      return TriggerNormal(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopNormal:
    if (bridge_.recording() && !bridge_.command_mode()) {
      return TriggerNormal(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartCommand:
    if (!bridge_.recording()) {
      return TriggerCommand(ic, selected_text);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopCommand:
    if (bridge_.recording() && bridge_.command_mode()) {
      return TriggerCommand(ic, "");
    }
    return AppliedOutcome::None;
  }
  return AppliedOutcome::None;
}

void FcitxVinputAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  const auto action = trigger_policy_.Classify(key_event);
  if (action == FcitxTriggerAction::None) {
    return;
  }

  FCITX_INFO() << "fcitx-vinput handling trigger " << TriggerActionName(action);
  ApplyTriggerAction(key_event.inputContext(), action,
                     SelectedTextFromInputContext(instance_, key_event.inputContext()));
  key_event.filterAndAccept();
}

} // namespace vinput_fcitx_bridge
