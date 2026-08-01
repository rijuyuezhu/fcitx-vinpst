#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include "vinput_fcitx_bridge/fcitx_menu_paging.h"

#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <dbus_public.h>

#ifdef VINPUT_FCITX_HAVE_CLIPBOARD
#include "clipboard_public.h"
#include <fcitx-utils/utf8.h>
#endif

#include <fcitx-utils/log.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/surroundingtext.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <chrono>
#include <functional>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

std::string TriggerListDescription(const fcitx::KeyList &keys) {
  std::string description;
  for (const auto &key : keys) {
    if (!description.empty()) {
      description += ", ";
    }
    description += key.toString();
  }
  return description.empty() ? "<disabled>" : description;
}

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
  case FcitxTriggerAction::ShowSceneMenu:
    return "show-scene-menu";
  case FcitxTriggerAction::ConsumeSceneMenuRelease:
    return "consume-scene-menu-release";
  case FcitxTriggerAction::ShowAsrMenu:
    return "show-asr-menu";
  case FcitxTriggerAction::ConsumeAsrMenuRelease:
    return "consume-asr-menu-release";
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
    : FcitxVinputAddon(instance, nullptr) {}

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance,
                                   fcitx::dbus::Bus *signal_bus)
    : instance_(instance), frontend_settings_(LoadFrontendSettings()),
      trigger_policy_(FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
          frontend_settings_.normal_triggers, frontend_settings_.command_triggers,
          frontend_settings_.scene_menu_triggers,
          frontend_settings_.asr_menu_triggers)),
      trigger_mode_controller_(frontend_settings_.trigger_mode) {
  InitFrontendI18n();
  FCITX_INFO() << "fcitx-vinput addon loaded with normal triggers "
               << TriggerListDescription(trigger_policy_.normal_triggers())
               << ", command triggers "
               << TriggerListDescription(trigger_policy_.command_triggers())
               << ", scene menu triggers "
               << TriggerListDescription(trigger_policy_.scene_menu_triggers())
               << ", and ASR menu triggers "
               << TriggerListDescription(trigger_policy_.asr_menu_triggers())
               << ", trigger mode "
               << TriggerModeToString(frontend_settings_.trigger_mode);
  if (instance_ != nullptr) {
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PostInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextCreated, fcitx::EventWatcherPhase::PreInputMethod,
        RequestSurroundingText));
  }
  if (signal_bus != nullptr) {
    SetupDaemonSignalMonitor(signal_bus);
  } else if (instance_ != nullptr) {
    SetupDaemonSignalMonitor();
  }
}

void FcitxVinputAddon::reloadConfig() {
  frontend_settings_ = LoadFrontendSettings();
  ApplyFrontendSettings();
}

void FcitxVinputAddon::save() {
  if (!SaveFrontendSettings(frontend_settings_)) {
    const auto message = FrontendText("Failed to save frontend configuration.");
    FCITX_ERROR() << "fcitx-vinput failed to save frontend configuration";
    Notify(FrontendNotificationKind::Error, message);
  }
}

const fcitx::Configuration *FcitxVinputAddon::getConfig() const {
  frontend_config_ = BuildFrontendConfig(frontend_settings_);
  return frontend_config_.get();
}

void FcitxVinputAddon::setConfig(const fcitx::RawConfig &config) {
  auto frontend_config = BuildFrontendConfig(frontend_settings_);
  frontend_config->load(config, true);
  frontend_settings_ = frontend_config->settings();
  ApplyFrontendSettings();
  save();
}

void FcitxVinputAddon::SetupDaemonSignalMonitor() {
  auto *dbus_addon = instance_->addonManager().addon("dbus");
  if (dbus_addon == nullptr) {
    FCITX_WARN()
        << "fcitx-vinput DBus module is unavailable; daemon signals are disabled";
    return;
  }
  auto *bus = dbus_addon->call<fcitx::IDBusModule::bus>();
  SetupDaemonSignalMonitor(bus);
}

void FcitxVinputAddon::SetupDaemonSignalMonitor(fcitx::dbus::Bus *bus) {
  daemon_signal_monitor_ = std::make_unique<FcitxDaemonSignalMonitor>(
      bus, DaemonSignalCallbacks{
               .service_availability_changed =
                   [this](bool available) { HandleDaemonAvailability(available); },
               .status_changed =
                   [this](std::string_view status) { HandleDaemonStatus(status); },
               .recognition_partial =
                   [this](std::string_view partial_text) {
                     HandleRecognitionPartial(partial_text);
                   },
               .notification =
                   [this](const DaemonNotificationPayload &payload) {
                     HandleDaemonNotification(payload);
                   },
           });
  if (!daemon_signal_monitor_->active()) {
    FCITX_WARN() << "fcitx-vinput failed to subscribe to daemon notifications";
    daemon_signal_monitor_.reset();
  }
}

void FcitxVinputAddon::HandleDaemonAvailability(bool available) {
  daemon_client_.reset();
  if (available || (!bridge_.recording() && !remote_status_ic_.isValid())) {
    return;
  }

  auto *active_ic =
      bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
  HideSceneMenu();
  HideAsrMenu();
  CancelTriggerStart();
  CancelTriggerStop();
  bridge_.Reset();
  trigger_mode_controller_.RecordingStopped();
  remote_status_ic_.unwatch();
  remote_status_command_mode_ = false;
  ResetLiveSignalState();
  const BridgeOutcome error{
      .kind = BridgeOutcome::Kind::Error,
      .text = "Voice input daemon is unavailable.",
      .payload = {},
      .command_mode = false,
  };
  ApplyBridgeOutcome(active_ic, error);
  active_trigger_ic_.unwatch();
}

void FcitxVinputAddon::HandleDaemonStatus(std::string_view status) {
  if (status.empty()) {
    return;
  }
  live_daemon_status_ = std::string(status);
  if (!bridge_.recording()) {
    if (!remote_status_ic_.isValid()) {
      return;
    }
    if (status == dbus::kStatusIdle || status == dbus::kStatusError) {
      ClearRemoteDaemonStatus();
      ResetLiveSignalState();
      return;
    }
    PresentRemoteDaemonStatus(remote_status_ic_.get(), status,
                              remote_status_command_mode_);
    return;
  }
  if (status == dbus::kStatusIdle || status == dbus::kStatusError) {
    auto *active_ic = active_trigger_ic_.get();
    CancelTriggerStart();
    CancelTriggerStop();
    bridge_.Reset();
    trigger_mode_controller_.RecordingStopped();
    if (active_ic != nullptr) {
      const BridgeOutcome clear{
          .kind = BridgeOutcome::Kind::Clear,
          .text = {},
          .payload = {},
          .command_mode = false,
      };
      ApplyBridgeOutcomeToInputContext(clear, active_ic);
    }
    active_trigger_ic_.unwatch();
    ResetLiveSignalState();
    return;
  }
  UpdateLivePreedit();
}

void FcitxVinputAddon::HandleRecognitionPartial(std::string_view partial_text) {
  if (!bridge_.recording() || partial_text.empty() ||
      partial_text == live_partial_text_) {
    return;
  }
  live_partial_text_ = std::string(partial_text);
  UpdateLivePreedit();
}

void FcitxVinputAddon::UpdateLivePreedit() {
  if (!bridge_.recording()) {
    return;
  }
  auto *active_ic = active_trigger_ic_.get();
  if (active_ic == nullptr) {
    return;
  }
  const auto preedit = ComposeDaemonStatusPreedit(
      live_daemon_status_, bridge_.command_mode(), live_partial_text_);
  if (preedit.empty()) {
    return;
  }
  const BridgeOutcome outcome{
      .kind = BridgeOutcome::Kind::Preedit,
      .text = preedit,
      .payload = {},
      .command_mode = bridge_.command_mode(),
  };
  ApplyBridgeOutcomeToInputContext(outcome, active_ic);
}

void FcitxVinputAddon::ResetLiveSignalState() {
  live_daemon_status_.clear();
  live_partial_text_.clear();
}

void FcitxVinputAddon::HandleDaemonNotification(
    const DaemonNotificationPayload &payload) {
  if (payload.empty()) {
    return;
  }
  const auto kind = ClassifyDaemonNotification(payload);
  const auto message = RenderDaemonNotification(payload);
  if (kind == FrontendNotificationKind::Error) {
    auto *active_ic =
        bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
    HideSceneMenu();
    HideAsrMenu();
    CancelTriggerStart();
    CancelTriggerStop();
    bridge_.Reset();
    trigger_mode_controller_.RecordingStopped();
    remote_status_ic_.unwatch();
    remote_status_command_mode_ = false;
    ResetLiveSignalState();
    daemon_client_.reset();
    if (active_ic != nullptr) {
      const BridgeOutcome clear{
          .kind = BridgeOutcome::Kind::Clear,
          .text = {},
          .payload = {},
          .command_mode = false,
      };
      ApplyBridgeOutcomeToInputContext(clear, active_ic);
    }
    active_trigger_ic_.unwatch();
  }
  Notify(kind, message);
}

void FcitxVinputAddon::Notify(FrontendNotificationKind kind, std::string_view message) {
  if (message.empty()) {
    return;
  }
  SendFrontendNotification(instance_, BuildFrontendNotification(kind, message));
}

void FcitxVinputAddon::ApplyFrontendSettings() {
  CancelTriggerStart();
  trigger_policy_ = FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
      frontend_settings_.normal_triggers, frontend_settings_.command_triggers,
      frontend_settings_.scene_menu_triggers, frontend_settings_.asr_menu_triggers);
  trigger_mode_controller_.SetMode(frontend_settings_.trigger_mode);
  if (!bridge_.recording()) {
    CancelTriggerStop();
    trigger_mode_controller_.RecordingStopped();
    active_trigger_ic_.unwatch();
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
  outcome.text = error.empty() ? FrontendText("Voice input daemon is unavailable.")
                               : std::move(error);
  FCITX_WARN() << "fcitx-vinput daemon unavailable: " << outcome.text;
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyBridgeOutcome(fcitx::InputContext *ic,
                                                    const BridgeOutcome &outcome) {
  ClearRemoteDaemonStatus();
  auto display_outcome = outcome;
  if (outcome.kind == BridgeOutcome::Kind::Preedit ||
      outcome.kind == BridgeOutcome::Kind::Error) {
    display_outcome.text = FrontendText(outcome.text);
  }
  if (outcome.kind == BridgeOutcome::Kind::Preedit && bridge_.recording()) {
    live_daemon_status_ = std::string(dbus::kStatusRecording);
    live_partial_text_.clear();
  }
  if (outcome.kind == BridgeOutcome::Kind::Error) {
    ResetLiveSignalState();
    daemon_client_.reset();
    Notify(FrontendNotificationKind::Error, display_outcome.text);
  } else if (!bridge_.recording()) {
    ResetLiveSignalState();
  }
  return ApplyBridgeOutcomeToInputContext(display_outcome, ic);
}

std::optional<AppliedOutcome>
FcitxVinputAddon::ReconcileDaemonStatusBeforeStart(fcitx::InputContext *ic,
                                                   TriggerKind kind) {
  if (bridge_.recording()) {
    return std::nullopt;
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  std::string status;
  if (client == nullptr || !client->GetStatus(&status, &error) || status.empty() ||
      status == dbus::kStatusIdle) {
    return std::nullopt;
  }

  const bool command_mode = kind == TriggerKind::Command;
  if (status == dbus::kStatusRecording && !command_mode) {
    ClearRemoteDaemonStatus();
    bridge_.AdoptRecording(false, active_scene_id_);
    return TriggerNormal(ic, active_scene_id_);
  }
  if (status == dbus::kStatusRecording || status == dbus::kStatusInferring ||
      status == dbus::kStatusPostprocessing) {
    return PresentRemoteDaemonStatus(ic, status, command_mode);
  }
  if (status == dbus::kStatusError) {
    ClearRemoteDaemonStatus();
    ResetLiveSignalState();
    return AppliedOutcome::None;
  }
  return std::nullopt;
}

AppliedOutcome FcitxVinputAddon::PresentRemoteDaemonStatus(fcitx::InputContext *ic,
                                                           std::string_view status,
                                                           bool command_mode) {
  const auto preedit = ComposeDaemonStatusPreedit(status, command_mode, {});
  if (preedit.empty()) {
    return AppliedOutcome::None;
  }
  live_daemon_status_ = std::string(status);
  live_partial_text_.clear();
  remote_status_command_mode_ = command_mode;
  if (ic != nullptr) {
    remote_status_ic_ = ic->watch();
  }
  const BridgeOutcome outcome{
      .kind = BridgeOutcome::Kind::Preedit,
      .text = preedit,
      .payload = {},
      .command_mode = command_mode,
  };
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

void FcitxVinputAddon::ClearRemoteDaemonStatus() {
  auto *ic = remote_status_ic_.get();
  remote_status_ic_.unwatch();
  remote_status_command_mode_ = false;
  if (ic == nullptr) {
    return;
  }
  const BridgeOutcome clear{
      .kind = BridgeOutcome::Kind::Clear,
      .text = {},
      .payload = {},
      .command_mode = false,
  };
  ApplyBridgeOutcomeToInputContext(clear, ic);
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
  const auto remember_started_input_context = [this, ic](AppliedOutcome outcome) {
    if (bridge_.recording() && ic != nullptr) {
      active_trigger_ic_ = ic->watch();
    }
    return outcome;
  };
  switch (action) {
  case FcitxTriggerAction::None:
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartNormal:
    if (!bridge_.recording()) {
      std::string error;
      RefreshSceneState(&error);
      if (auto recovered = ReconcileDaemonStatusBeforeStart(ic, TriggerKind::Normal)) {
        return remember_started_input_context(*recovered);
      }
      return remember_started_input_context(TriggerNormal(ic, active_scene_id_));
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopNormal:
    if (bridge_.recording() && !bridge_.command_mode()) {
      return TriggerNormal(ic, active_scene_id_);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartCommand:
    if (!bridge_.recording()) {
      if (auto recovered = ReconcileDaemonStatusBeforeStart(ic, TriggerKind::Command)) {
        return remember_started_input_context(*recovered);
      }
      return remember_started_input_context(TriggerCommand(ic, selected_text));
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopCommand:
    if (bridge_.recording() && bridge_.command_mode()) {
      return TriggerCommand(ic, "");
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ShowSceneMenu:
    if (!bridge_.recording()) {
      ShowSceneMenu(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ConsumeSceneMenuRelease:
    return AppliedOutcome::None;
  case FcitxTriggerAction::ShowAsrMenu:
    if (!bridge_.recording()) {
      ShowAsrMenu(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ConsumeAsrMenuRelease:
    return AppliedOutcome::None;
  }
  return AppliedOutcome::None;
}

void FcitxVinputAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  if (asr_menu_visible_ && HandleAsrMenuKeyEvent(key_event)) {
    return;
  }
  if (scene_menu_visible_ && HandleSceneMenuKeyEvent(key_event)) {
    return;
  }
  const auto action = trigger_policy_.Classify(key_event);
  if (action == FcitxTriggerAction::None) {
    return;
  }

  FCITX_INFO() << "fcitx-vinput handling trigger " << TriggerActionName(action);
  if (action == FcitxTriggerAction::ShowSceneMenu ||
      action == FcitxTriggerAction::ConsumeSceneMenuRelease ||
      action == FcitxTriggerAction::ShowAsrMenu ||
      action == FcitxTriggerAction::ConsumeAsrMenuRelease) {
    ApplyTriggerAction(key_event.inputContext(), action);
    key_event.filterAndAccept();
    return;
  }

  const bool normal = action == FcitxTriggerAction::StartNormal ||
                      action == FcitxTriggerAction::StopNormal;
  const auto kind = normal ? TriggerKind::Normal : TriggerKind::Command;
  TriggerModeAction mode_action = TriggerModeAction::None;
  if (key_event.isRelease()) {
    mode_action = trigger_mode_controller_.OnRelease(
        kind, key_event.key(), TriggerModeController::Clock::now());
  } else {
    mode_action = trigger_mode_controller_.OnPress(kind, key_event.key(),
                                                   TriggerModeController::Clock::now(),
                                                   bridge_.recording());
    if (mode_action != TriggerModeAction::None &&
        mode_action != TriggerModeAction::Consume) {
      CancelTriggerStop();
    }
  }
  HandleTriggerModeAction(key_event.inputContext(), mode_action);
  key_event.filterAndAccept();
}

void FcitxVinputAddon::HandleTriggerModeAction(fcitx::InputContext *ic,
                                               TriggerModeAction action) {
  switch (action) {
  case TriggerModeAction::None:
  case TriggerModeAction::Consume:
    return;
  case TriggerModeAction::StartNormal:
    ApplyTriggerAction(ic, FcitxTriggerAction::StartNormal);
    break;
  case TriggerModeAction::StartCommand:
    ApplyTriggerAction(ic, FcitxTriggerAction::StartCommand,
                       SelectedTextFromInputContext(instance_, ic));
    break;
  case TriggerModeAction::StopActive:
    StopActiveRecording(ic);
    return;
  case TriggerModeAction::ScheduleNormalStart:
  case TriggerModeAction::ScheduleCommandStart:
    ScheduleTriggerStart(ic);
    return;
  case TriggerModeAction::CancelPendingStart:
    CancelTriggerStart();
    return;
  case TriggerModeAction::ScheduleStop:
    ScheduleTriggerStop(ic);
    return;
  }

  const bool started = bridge_.recording();
  trigger_mode_controller_.ConfirmStart(started);
  if (started && ic != nullptr) {
    active_trigger_ic_ = ic->watch();
  }
}

void FcitxVinputAddon::ScheduleTriggerStart(fcitx::InputContext *ic) {
  CancelTriggerStart();
  if (instance_ == nullptr || ic == nullptr) {
    trigger_mode_controller_.RecordingStopped();
    return;
  }
  pending_trigger_ic_ = ic->watch();
  const auto fire_at =
      fcitx::now(CLOCK_MONOTONIC) +
      static_cast<std::uint64_t>(
          std::chrono::duration_cast<std::chrono::microseconds>(kTriggerHoldThreshold)
              .count());
  pending_trigger_start_event_ = instance_->eventLoop().addTimeEvent(
      CLOCK_MONOTONIC, fire_at, 0, [this](fcitx::EventSourceTime *, std::uint64_t) {
        auto *input_context = pending_trigger_ic_.get();
        pending_trigger_start_event_.reset();
        pending_trigger_ic_.unwatch();
        const auto action = trigger_mode_controller_.FirePendingStart();
        if (input_context == nullptr) {
          trigger_mode_controller_.ConfirmStart(false);
          return false;
        }
        HandleTriggerModeAction(input_context, action);
        return false;
      });
  pending_trigger_start_event_->setOneShot();
}

void FcitxVinputAddon::CancelTriggerStart() {
  if (pending_trigger_start_event_ != nullptr) {
    pending_trigger_start_event_->setEnabled(false);
    pending_trigger_start_event_.reset();
  }
  pending_trigger_ic_.unwatch();
}

void FcitxVinputAddon::ScheduleTriggerStop(fcitx::InputContext *fallback_ic) {
  CancelTriggerStop();
  if (!active_trigger_ic_.isValid() && fallback_ic != nullptr) {
    active_trigger_ic_ = fallback_ic->watch();
  }
  if (instance_ == nullptr) {
    StopActiveRecording(fallback_ic);
    return;
  }
  const auto fire_at =
      fcitx::now(CLOCK_MONOTONIC) +
      static_cast<std::uint64_t>(
          std::chrono::duration_cast<std::chrono::microseconds>(kTriggerReleaseTail)
              .count());
  pending_trigger_stop_event_ = instance_->eventLoop().addTimeEvent(
      CLOCK_MONOTONIC, fire_at, 0, [this](fcitx::EventSourceTime *, std::uint64_t) {
        pending_trigger_stop_event_.reset();
        if (trigger_mode_controller_.FirePendingStop() ==
            TriggerModeAction::StopActive) {
          StopActiveRecording(active_trigger_ic_.get());
        }
        return false;
      });
  pending_trigger_stop_event_->setOneShot();
}

void FcitxVinputAddon::CancelTriggerStop() {
  if (pending_trigger_stop_event_ != nullptr) {
    pending_trigger_stop_event_->setEnabled(false);
    pending_trigger_stop_event_.reset();
  }
}

void FcitxVinputAddon::StopActiveRecording(fcitx::InputContext *fallback_ic) {
  auto *ic = active_trigger_ic_.get();
  if (ic == nullptr) {
    ic = fallback_ic;
  }
  if (bridge_.recording()) {
    if (bridge_.command_mode()) {
      ApplyTriggerAction(ic, FcitxTriggerAction::StopCommand);
    } else {
      ApplyTriggerAction(ic, FcitxTriggerAction::StopNormal);
    }
  }
  if (!bridge_.recording()) {
    CancelTriggerStart();
    CancelTriggerStop();
    trigger_mode_controller_.RecordingStopped();
    active_trigger_ic_.unwatch();
  }
}

} // namespace vinput_fcitx_bridge
