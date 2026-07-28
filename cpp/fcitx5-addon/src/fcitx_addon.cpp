#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

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

constexpr int kMenuPageSize = 10;

class MenuCandidateWord final : public fcitx::CandidateWord {
public:
  MenuCandidateWord(std::string label,
                    std::function<void(fcitx::InputContext *)> on_select)
      : fcitx::CandidateWord(fcitx::Text(std::move(label))),
        on_select_(std::move(on_select)) {}

  void select(fcitx::InputContext *input_context) const override {
    if (on_select_) {
      on_select_(input_context);
    }
  }

private:
  std::function<void(fcitx::InputContext *)> on_select_;
};

bool IsKey(const fcitx::Key &key, FcitxKeySym symbol) {
  const auto normalized = key.normalize();
  return normalized.sym() == symbol && normalized.states() == fcitx::KeyStates();
}

int CurrentMenuSelectionIndex(fcitx::CandidateList *candidate_list) {
  if (candidate_list == nullptr || candidate_list->cursorIndex() < 0) {
    return -1;
  }
  auto *pageable = candidate_list->toPageable();
  const int page = pageable != nullptr ? pageable->currentPage() : 0;
  return page * kMenuPageSize + candidate_list->cursorIndex();
}

std::string DecoratePagedMenuTitle(std::string title,
                                   fcitx::CandidateList *candidate_list) {
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (pageable == nullptr || pageable->totalPages() <= 1 ||
      pageable->currentPage() < 0) {
    return title;
  }
  title += FrontendPageText(pageable->currentPage() + 1, pageable->totalPages());
  return title;
}

void SetFilteredMenuTitle(fcitx::InputContext *ic, std::string_view base_title,
                          const MenuFilterState &filter,
                          fcitx::CandidateList *candidate_list) {
  if (ic == nullptr) {
    return;
  }
  ic->inputPanel().setAuxUp(fcitx::Text(
      DecoratePagedMenuTitle(filter.DecorateTitle(base_title), candidate_list)));
}

bool IsMenuEnterKey(const fcitx::Key &key) {
  return IsKey(key, FcitxKey_Return) || IsKey(key, FcitxKey_KP_Enter);
}

bool IsHandledMenuKey(const fcitx::Key &key, bool trigger_key,
                      const MenuFilterState &filter, const FrontendSettings &settings) {
  return trigger_key || key.checkKeyList(settings.page_prev_keys) ||
         key.checkKeyList(settings.page_next_keys) || key.digitSelection() >= 0 ||
         IsMenuSlashKey(key) || IsMenuBackspaceKey(key) ||
         IsMenuCtrlShortcut(key, FcitxKey_w) || IsMenuCtrlShortcut(key, FcitxKey_u) ||
         IsKey(key, FcitxKey_Up) || IsKey(key, FcitxKey_Down) || IsMenuEnterKey(key) ||
         IsKey(key, FcitxKey_Escape) || IsMenuPureModifierKey(key) ||
         IsPrintableMenuInput(key, filter.active(), settings.page_prev_keys,
                              settings.page_next_keys);
}

bool IsEffectiveAsrTarget(const AsrDisplayMenuItem &target,
                          const AsrDisplayMenuStateSnapshot &state) {
  if (target.provider_id != state.effective_provider_id) {
    return false;
  }
  if (target.model_value == state.effective_model_id) {
    return true;
  }
  return !state.reload_in_progress && state.last_error.empty() &&
         target.provider_id == state.target_provider_id &&
         target.model_value == state.target_model_id;
}

std::string TranslatedProviderKind(std::string_view kind) {
  if (kind == "local") {
    return FrontendText("Local");
  }
  if (kind == "remote") {
    return FrontendText("Remote");
  }
  if (kind == "command") {
    return FrontendText("Command");
  }
  return std::string(kind);
}

std::string AsrTargetLabel(const AsrDisplayMenuItem &target,
                           const AsrDisplayMenuStateSnapshot &state) {
  std::string label =
      target.display_title.empty()
          ? (target.item_id.empty() ? target.provider_id : target.item_id)
          : target.display_title;
  label += " [" + TranslatedProviderKind(target.kind) + "]";
  if (state.reload_in_progress && target.provider_id == state.target_provider_id &&
      target.model_value == state.target_model_id) {
    label += FrontendText(" (loading)");
  }
  return label;
}

std::string AsrDisplayTitleFor(std::string_view provider_id,
                               std::string_view model_value,
                               const AsrDisplayMenuStateSnapshot &state) {
  for (const auto &target : state.targets) {
    if (target.provider_id == provider_id && target.model_value == model_value) {
      return target.display_title.empty() ? target.item_id : target.display_title;
    }
  }
  return std::string(model_value);
}

std::string EffectiveAsrLabel(const AsrDisplayMenuStateSnapshot &state) {
  std::string label = state.effective_model_id.empty()
                          ? state.effective_provider_id
                          : AsrDisplayTitleFor(state.effective_provider_id,
                                               state.effective_model_id, state);
  if (label.empty()) {
    label = FrontendText("unavailable");
  }
  if (state.reload_in_progress && !state.target_provider_id.empty()) {
    label += " | " + FrontendText("Loading: ") + state.target_provider_id;
    if (!state.target_model_id.empty()) {
      label += "/" + AsrDisplayTitleFor(state.target_provider_id, state.target_model_id,
                                        state);
    }
  }
  if (!state.last_error.empty()) {
    label += " | " + FrontendText("Error: ") + state.last_error;
  }
  return label;
}

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
  switch (action) {
  case FcitxTriggerAction::None:
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartNormal:
    if (!bridge_.recording()) {
      std::string error;
      RefreshSceneState(&error);
      if (auto recovered = ReconcileDaemonStatusBeforeStart(ic, TriggerKind::Normal)) {
        return *recovered;
      }
      return TriggerNormal(ic, active_scene_id_);
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
        return *recovered;
      }
      return TriggerCommand(ic, selected_text);
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

void FcitxVinputAddon::ShowSceneMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideAsrMenu();
  std::string error;
  if (!RefreshSceneState(&error)) {
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }

  scene_menu_ic_ = ic;
  scene_menu_visible_ = true;
  scene_menu_filter_.Reset();
  RebuildSceneMenu();
}

void FcitxVinputAddon::RebuildSceneMenu() {
  if (!scene_menu_visible_ || scene_menu_ic_ == nullptr) {
    return;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  scene_menu_indices_.clear();
  std::string active_label = active_scene_id_;
  for (std::size_t index = 0; index < scene_state_.scenes.size(); ++index) {
    const auto &scene = scene_state_.scenes[index];
    if (scene.id == active_scene_id_) {
      active_label = scene.label;
      continue;
    }
    if (!scene_menu_filter_.Matches(scene.label + " " + scene.id)) {
      continue;
    }
    scene_menu_indices_.push_back(index);
    candidates->append<MenuCandidateWord>(
        scene.label, [this, index](fcitx::InputContext *input_context) {
          SelectScene(index, input_context);
        });
  }
  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
  }

  SetFilteredMenuTitle(scene_menu_ic_, FrontendText("Scenes /filter"),
                       scene_menu_filter_, candidates.get());
  scene_menu_ic_->inputPanel().setAuxDown(
      fcitx::Text(FrontendText("Current: ") + active_label));
  scene_menu_ic_->inputPanel().setCandidateList(std::move(candidates));
  scene_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshSceneState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  if (client == nullptr || !client->GetSceneState(&scene_state_, error)) {
    return false;
  }
  if (!scene_state_.active_scene_id.empty()) {
    active_scene_id_ = scene_state_.active_scene_id;
  }
  return true;
}

void FcitxVinputAddon::HideSceneMenu() {
  auto *ic = scene_menu_ic_;
  scene_menu_visible_ = false;
  scene_menu_ic_ = nullptr;
  scene_menu_filter_.Reset();
  scene_menu_indices_.clear();
  if (ic == nullptr) {
    return;
  }
  fcitx::Text empty;
  ic->inputPanel().setAuxUp(empty);
  ic->inputPanel().setAuxDown(empty);
  ic->inputPanel().setCandidateList({});
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::HandleSceneMenuKeyEvent(fcitx::KeyEvent &event) {
  if (!scene_menu_visible_ || scene_menu_ic_ == nullptr) {
    return false;
  }
  const auto &key = event.key();
  const bool trigger_key = trigger_policy_.IsSceneMenuTrigger(event);
  const bool printable_filter_input = IsPrintableMenuInput(
      key, scene_menu_filter_.active(), frontend_settings_.page_prev_keys,
      frontend_settings_.page_next_keys);
  const bool handled =
      IsHandledMenuKey(key, trigger_key, scene_menu_filter_, frontend_settings_);

  if (event.isRelease()) {
    if (!handled) {
      return false;
    }
    event.filterAndAccept();
    return true;
  }
  if (!handled) {
    HideSceneMenu();
    return false;
  }
  if (trigger_key || IsMenuPureModifierKey(key)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    if (scene_menu_filter_.active() || !scene_menu_filter_.query().empty()) {
      scene_menu_filter_.ClearAndDeactivate();
      RebuildSceneMenu();
    } else {
      HideSceneMenu();
    }
    event.filterAndAccept();
    return true;
  }
  if (IsMenuSlashKey(key)) {
    scene_menu_filter_.Activate();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (IsMenuBackspaceKey(key) && scene_menu_filter_.active()) {
    scene_menu_filter_.Backspace();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (scene_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_w)) {
    scene_menu_filter_.DeleteLastWord();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (scene_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_u)) {
    scene_menu_filter_.ClearAndDeactivate();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (printable_filter_input) {
    scene_menu_filter_.AppendText(MenuKeyToUtf8(key));
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = scene_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (key.checkKeyList(frontend_settings_.page_prev_keys)) {
    if (pageable != nullptr && pageable->hasPrev()) {
      pageable->prev();
    }
  } else if (key.checkKeyList(frontend_settings_.page_next_keys)) {
    if (pageable != nullptr && pageable->hasNext()) {
      pageable->next();
    }
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int page = pageable != nullptr ? pageable->currentPage() : 0;
      const int index = page * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
        SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)],
                    scene_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
      cursor->prevCandidate();
    } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
      cursor->nextCandidate();
    } else if (IsMenuEnterKey(key)) {
      int index = CurrentMenuSelectionIndex(candidate_list.get());
      if (index < 0 && !scene_menu_indices_.empty()) {
        index = 0;
      }
      if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
        SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)],
                    scene_menu_ic_);
      } else {
        HideSceneMenu();
      }
      event.filterAndAccept();
      return true;
    } else {
      HideSceneMenu();
      return false;
    }
  }

  SetFilteredMenuTitle(scene_menu_ic_, FrontendText("Scenes /filter"),
                       scene_menu_filter_, candidate_list.get());
  scene_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
}

void FcitxVinputAddon::SelectScene(std::size_t index, fcitx::InputContext *ic) {
  if (index >= scene_state_.scenes.size()) {
    HideSceneMenu();
    return;
  }
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);
  const auto &scene = scene_state_.scenes[index];
  if (client == nullptr || !client->SetActiveScene(scene.id, &persisted, &error)) {
    HideSceneMenu();
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }
  active_scene_id_ = scene.id;
  scene_state_.active_scene_id = scene.id;
  HideSceneMenu();
  const auto message = FrontendValueText("Switched scene to '%s'.", scene.label);
  Notify(FrontendNotificationKind::Info, message);
  FCITX_INFO() << "fcitx-vinput switched active scene to " << scene.id
               << " persisted=" << persisted;
}

void FcitxVinputAddon::ShowAsrMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideSceneMenu();
  std::string error;
  if (!RefreshAsrMenuState(&error)) {
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }

  asr_menu_ic_ = ic;
  asr_menu_visible_ = true;
  asr_menu_filter_.Reset();
  RebuildAsrMenu();
}

void FcitxVinputAddon::RebuildAsrMenu() {
  if (!asr_menu_visible_ || asr_menu_ic_ == nullptr) {
    return;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  asr_menu_indices_.clear();
  for (std::size_t index = 0; index < asr_menu_state_.targets.size(); ++index) {
    const auto &target = asr_menu_state_.targets[index];
    if (IsEffectiveAsrTarget(target, asr_menu_state_)) {
      continue;
    }
    const auto label = AsrTargetLabel(target, asr_menu_state_);
    const auto search_text = label + " " + target.provider_id + " " + target.kind +
                             " " + target.item_id + " " + target.display_title + " " +
                             target.model_value;
    if (!asr_menu_filter_.Matches(search_text)) {
      continue;
    }
    asr_menu_indices_.push_back(index);
    candidates->append<MenuCandidateWord>(
        label, [this, index](fcitx::InputContext *input_context) {
          SelectAsrTarget(index, input_context);
        });
  }
  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
  }

  SetFilteredMenuTitle(asr_menu_ic_, FrontendText("Models /filter"), asr_menu_filter_,
                       candidates.get());
  asr_menu_ic_->inputPanel().setAuxDown(
      fcitx::Text(FrontendText("Current: ") + EffectiveAsrLabel(asr_menu_state_)));
  asr_menu_ic_->inputPanel().setCandidateList(std::move(candidates));
  asr_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshAsrMenuState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  return client != nullptr && client->GetAsrDisplayMenuState(&asr_menu_state_, error);
}

void FcitxVinputAddon::HideAsrMenu() {
  auto *ic = asr_menu_ic_;
  asr_menu_visible_ = false;
  asr_menu_ic_ = nullptr;
  asr_menu_filter_.Reset();
  asr_menu_indices_.clear();
  if (ic == nullptr) {
    return;
  }
  fcitx::Text empty;
  ic->inputPanel().setAuxUp(empty);
  ic->inputPanel().setAuxDown(empty);
  ic->inputPanel().setCandidateList({});
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::HandleAsrMenuKeyEvent(fcitx::KeyEvent &event) {
  if (!asr_menu_visible_ || asr_menu_ic_ == nullptr) {
    return false;
  }
  const auto &key = event.key();
  const bool trigger_key = trigger_policy_.IsAsrMenuTrigger(event);
  const bool printable_filter_input = IsPrintableMenuInput(
      key, asr_menu_filter_.active(), frontend_settings_.page_prev_keys,
      frontend_settings_.page_next_keys);
  const bool handled =
      IsHandledMenuKey(key, trigger_key, asr_menu_filter_, frontend_settings_);

  if (event.isRelease()) {
    if (!handled) {
      return false;
    }
    event.filterAndAccept();
    return true;
  }
  if (!handled) {
    HideAsrMenu();
    return false;
  }
  if (trigger_key || IsMenuPureModifierKey(key)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    if (asr_menu_filter_.active() || !asr_menu_filter_.query().empty()) {
      asr_menu_filter_.ClearAndDeactivate();
      RebuildAsrMenu();
    } else {
      HideAsrMenu();
    }
    event.filterAndAccept();
    return true;
  }
  if (IsMenuSlashKey(key)) {
    asr_menu_filter_.Activate();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (IsMenuBackspaceKey(key) && asr_menu_filter_.active()) {
    asr_menu_filter_.Backspace();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (asr_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_w)) {
    asr_menu_filter_.DeleteLastWord();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (asr_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_u)) {
    asr_menu_filter_.ClearAndDeactivate();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (printable_filter_input) {
    asr_menu_filter_.AppendText(MenuKeyToUtf8(key));
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = asr_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (key.checkKeyList(frontend_settings_.page_prev_keys)) {
    if (pageable != nullptr && pageable->hasPrev()) {
      pageable->prev();
    }
  } else if (key.checkKeyList(frontend_settings_.page_next_keys)) {
    if (pageable != nullptr && pageable->hasNext()) {
      pageable->next();
    }
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int page = pageable != nullptr ? pageable->currentPage() : 0;
      const int index = page * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
        SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)],
                        asr_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
      cursor->prevCandidate();
    } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
      cursor->nextCandidate();
    } else if (IsMenuEnterKey(key)) {
      int index = CurrentMenuSelectionIndex(candidate_list.get());
      if (index < 0 && !asr_menu_indices_.empty()) {
        index = 0;
      }
      if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
        SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)],
                        asr_menu_ic_);
      } else {
        HideAsrMenu();
      }
      event.filterAndAccept();
      return true;
    } else {
      HideAsrMenu();
      return false;
    }
  }

  SetFilteredMenuTitle(asr_menu_ic_, FrontendText("Models /filter"), asr_menu_filter_,
                       candidate_list.get());
  asr_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
}

void FcitxVinputAddon::SelectAsrTarget(std::size_t index, fcitx::InputContext *ic) {
  if (index >= asr_menu_state_.targets.size()) {
    HideAsrMenu();
    return;
  }
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);
  const auto &target = asr_menu_state_.targets[index];
  if (client == nullptr ||
      !client->SetActiveAsrTarget(target.provider_id, target.model_value, &persisted,
                                  &error)) {
    HideAsrMenu();
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }
  HideAsrMenu();
  const auto display_title =
      target.display_title.empty() ? target.item_id : target.display_title;
  const auto message =
      FrontendValueText("ASR switch requested for '%s'.", display_title);
  Notify(FrontendNotificationKind::Info, message);
  FCITX_INFO() << "fcitx-vinput requested ASR target switch to " << target.provider_id
               << '/' << target.item_id << " persisted=" << persisted;
}

} // namespace vinput_fcitx_bridge
