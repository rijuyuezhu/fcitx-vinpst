#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include "vinput_fcitx_bridge/fcitx_menu_paging.h"

#include "vinput_fcitx_bridge/fcitx_selection.h"
#include "vinput_fcitx_ffi.h"

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
                   [this](FrontendNotificationKind kind, std::string_view message) {
                     HandleDaemonNotification(kind, message);
                   },
           });
  if (!daemon_signal_monitor_->active()) {
    FCITX_WARN() << "fcitx-vinput failed to subscribe to daemon notifications";
    daemon_signal_monitor_.reset();
  }
}

void FcitxVinputAddon::HandleDaemonAvailability(bool available) {
  daemon_client_.reset();
  auto *ic = bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
  static_cast<void>(
      ExecuteDaemonControl(VINPUT_FCITX_DAEMON_CONTROL_EVENT_AVAILABILITY_CHANGED, ic,
                           {}, available, false));
}

void FcitxVinputAddon::HandleDaemonStatus(std::string_view status) {
  auto *ic = bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
  static_cast<void>(
      ExecuteDaemonControl(VINPUT_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED, ic, status,
                           false, remote_status_command_mode_));
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

void FcitxVinputAddon::ResetActiveRecording(fcitx::InputContext *ic) {
  CancelTriggerStart();
  CancelTriggerStop();
  bridge_.Reset();
  trigger_mode_controller_.RecordingStopped();
  if (ic != nullptr) {
    const BridgeOutcome clear{
        .kind = BridgeOutcome::Kind::Clear,
        .text = {},
        .payload = {},
        .command_mode = false,
    };
    ApplyBridgeOutcomeToInputContext(clear, ic);
  }
  active_trigger_ic_.unwatch();
  ResetLiveSignalState();
}

void FcitxVinputAddon::HandleDaemonNotification(FrontendNotificationKind kind,
                                                std::string_view message) {
  if (message.empty()) {
    return;
  }
  if (kind == FrontendNotificationKind::Error) {
    auto *active_ic =
        bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
    HideSceneMenu();
    HideAsrMenu();
    remote_status_ic_.unwatch();
    remote_status_command_mode_ = false;
    daemon_client_.reset();
    ResetActiveRecording(active_ic);
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
FcitxVinputAddon::ExecuteDaemonControl(std::uint8_t event, fcitx::InputContext *ic,
                                       std::string_view status, bool flag,
                                       bool command_mode) {
  const auto *data =
      status.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(status.data());
  const auto plan = vinput_fcitx_daemon_control_plan(
      event, data, status.size(), static_cast<std::uint8_t>(flag),
      static_cast<std::uint8_t>(bridge_.recording()),
      static_cast<std::uint8_t>(remote_status_ic_.isValid()));
  if (event == VINPUT_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED &&
      plan != VINPUT_FCITX_DAEMON_CONTROL_PLAN_NONE) {
    live_daemon_status_ = std::string(status);
  }
  switch (plan) {
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_NONE:
    return std::nullopt;
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_RESET_UNAVAILABLE: {
    HideSceneMenu();
    HideAsrMenu();
    remote_status_ic_.unwatch();
    remote_status_command_mode_ = false;
    ResetActiveRecording(ic);
    const BridgeOutcome error{
        .kind = BridgeOutcome::Kind::Error,
        .text = "Voice input daemon is unavailable.",
        .payload = {},
        .command_mode = false,
    };
    return ApplyBridgeOutcome(ic, error);
  }
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_CLEAR_REMOTE_STATUS:
    ClearRemoteDaemonStatus();
    ResetLiveSignalState();
    return AppliedOutcome::None;
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_RESET_LOCAL_RECORDING:
    ResetActiveRecording(ic);
    return AppliedOutcome::None;
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_UPDATE_LOCAL_PREEDIT:
    UpdateLivePreedit();
    return AppliedOutcome::None;
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_PRESENT_REMOTE_STATUS:
    return PresentRemoteDaemonStatus(ic, status, command_mode);
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_ADOPT_AND_STOP_NORMAL:
    ClearRemoteDaemonStatus();
    if (auto *client = EnsureDaemonClient(nullptr); client != nullptr) {
      return ApplyBridgeOutcome(
          ic, bridge_.AdoptAndStop(client->raw_handle(), false, scene_state_));
    }
    return ApplyDaemonUnavailable(ic, "Voice input daemon is unavailable.");
  case VINPUT_FCITX_DAEMON_CONTROL_PLAN_CLEAR_DAEMON_ERROR:
    ClearRemoteDaemonStatus();
    ResetLiveSignalState();
    return AppliedOutcome::None;
  }
  return std::nullopt;
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
  if (client == nullptr || !client->GetStatus(&status, &error)) {
    return std::nullopt;
  }

  const bool command_mode = kind == TriggerKind::Command;
  return ExecuteDaemonControl(VINPUT_FCITX_DAEMON_CONTROL_EVENT_RECONCILE_BEFORE_START,
                              ic, status, command_mode, command_mode);
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

AppliedOutcome FcitxVinputAddon::StartNormalRecording(fcitx::InputContext *ic) {
  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  return ApplyBridgeOutcome(ic,
                            bridge_.StartNormal(client->raw_handle(), scene_state_));
}

AppliedOutcome FcitxVinputAddon::StartCommandRecording(fcitx::InputContext *ic,
                                                       std::string_view selected_text,
                                                       std::string_view scene_id) {
  if (selected_text.empty()) {
    return ApplyBridgeOutcome(ic,
                              bridge_.StartCommand(nullptr, selected_text, scene_id));
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  return ApplyBridgeOutcome(
      ic, bridge_.StartCommand(client->raw_handle(), selected_text, scene_id));
}

AppliedOutcome FcitxVinputAddon::StopRecording(fcitx::InputContext *ic) {
  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }
  return ApplyBridgeOutcome(ic, bridge_.Stop(client->raw_handle(), scene_state_));
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

  switch (bridge_.PlanTrigger(action)) {
  case FrontendTriggerIntent::None:
    return AppliedOutcome::None;
  case FrontendTriggerIntent::StartNormal: {
    std::string error;
    RefreshSceneState(&error);
    if (auto recovered = ReconcileDaemonStatusBeforeStart(ic, TriggerKind::Normal)) {
      return remember_started_input_context(*recovered);
    }
    return remember_started_input_context(StartNormalRecording(ic));
  }
  case FrontendTriggerIntent::StopNormal:
    return StopRecording(ic);
  case FrontendTriggerIntent::StartCommand:
    if (auto recovered = ReconcileDaemonStatusBeforeStart(ic, TriggerKind::Command)) {
      return remember_started_input_context(*recovered);
    }
    return remember_started_input_context(StartCommandRecording(ic, selected_text));
  case FrontendTriggerIntent::StopCommand:
    return StopRecording(ic);
  case FrontendTriggerIntent::ShowSceneMenu:
    ShowSceneMenu(ic);
    return AppliedOutcome::None;
  case FrontendTriggerIntent::ShowAsrMenu:
    ShowAsrMenu(ic);
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
    StopRecording(ic);
  }
  if (!bridge_.recording()) {
    CancelTriggerStart();
    CancelTriggerStop();
    trigger_mode_controller_.RecordingStopped();
    active_trigger_ic_.unwatch();
  }
}

} // namespace vinput_fcitx_bridge
