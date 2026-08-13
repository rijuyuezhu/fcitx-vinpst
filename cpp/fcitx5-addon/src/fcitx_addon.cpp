#include "vinpst_fcitx_bridge/fcitx_addon.h"

#include "vinpst_fcitx_bridge/dbus_contract.h"

#include "vinpst_fcitx_bridge/fcitx_i18n.h"

#include "vinpst_fcitx_bridge/fcitx_menu_paging.h"

#include "vinpst_fcitx_bridge/fcitx_selection.h"
#include "vinpst_fcitx_bridge/rust_string.h"
#include "vinpst_fcitx_ffi.h"

#include <dbus_public.h>

#ifdef VINPST_FCITX_HAVE_CLIPBOARD
#include "clipboard_public.h"
#include <fcitx-utils/utf8.h>
#endif

#include <fcitx-utils/dbus/message.h>
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

namespace vinpst_fcitx_bridge {
namespace {

constexpr std::uint64_t kDbusCallTimeoutUsec = 5'000'000;
constexpr auto kDaemonFailureCooldown = std::chrono::milliseconds(1500);
constexpr std::string_view kMethodStartRecording = "StartRecording";
constexpr std::string_view kMethodStartCommandRecording = "StartCommandRecording";
constexpr std::string_view kMethodStopRecording = "StopRecording";

enum class DaemonControlPlan : std::uint8_t {
  None = VINPST_FCITX_DAEMON_CONTROL_PLAN_NONE,
  ResetUnavailable = VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_UNAVAILABLE,
  ClearRemoteStatus = VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_REMOTE_STATUS,
  ResetLocalRecording = VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_LOCAL_RECORDING,
  UpdateLocalPreedit = VINPST_FCITX_DAEMON_CONTROL_PLAN_UPDATE_LOCAL_PREEDIT,
  PresentRemoteStatus = VINPST_FCITX_DAEMON_CONTROL_PLAN_PRESENT_REMOTE_STATUS,
  AdoptAndStopNormal = VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_AND_STOP_NORMAL,
  ClearDaemonError = VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_DAEMON_ERROR,
  AdoptExternalStatus = VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_EXTERNAL_STATUS,
};

std::optional<DaemonControlPlan> DecodeDaemonControlPlan(std::uint8_t value) {
  switch (value) {
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_NONE:
    return DaemonControlPlan::None;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_UNAVAILABLE:
    return DaemonControlPlan::ResetUnavailable;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_REMOTE_STATUS:
    return DaemonControlPlan::ClearRemoteStatus;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_LOCAL_RECORDING:
    return DaemonControlPlan::ResetLocalRecording;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_UPDATE_LOCAL_PREEDIT:
    return DaemonControlPlan::UpdateLocalPreedit;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_PRESENT_REMOTE_STATUS:
    return DaemonControlPlan::PresentRemoteStatus;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_AND_STOP_NORMAL:
    return DaemonControlPlan::AdoptAndStopNormal;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_DAEMON_ERROR:
    return DaemonControlPlan::ClearDaemonError;
  case VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_EXTERNAL_STATUS:
    return DaemonControlPlan::AdoptExternalStatus;
  default:
    return std::nullopt;
  }
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

BridgeOutcome BuildPreeditOutcome(std::string text, bool replace_selection) {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Preedit;
  outcome.text = std::move(text);
  outcome.replace_selection = replace_selection;
  return outcome;
}

#ifdef VINPST_FCITX_HAVE_CLIPBOARD
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

FcitxVinpstAddon::FcitxVinpstAddon(fcitx::Instance *instance)
    : FcitxVinpstAddon(instance, nullptr) {}

FcitxVinpstAddon::FcitxVinpstAddon(fcitx::Instance *instance,
                                   fcitx::dbus::Bus *signal_bus,
                                   fcitx::EventLoop *signal_event_loop)
    : instance_(instance), frontend_settings_(LoadFrontendSettings()),
      trigger_policy_(FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
          frontend_settings_.normal_triggers, frontend_settings_.command_triggers,
          frontend_settings_.scene_menu_triggers,
          frontend_settings_.asr_menu_triggers)),
      trigger_mode_controller_(frontend_settings_.trigger_mode),
      menu_refresh_dispatcher_(std::make_shared<fcitx::EventDispatcher>()) {
  InitFrontendI18n();
  bridge_.SetPresentationText(FrontendText("Original"), FrontendText("Voice Command"),
                              FrontendText("Cancel"));
  FCITX_INFO() << "fcitx-vinpst addon loaded with normal triggers "
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
    menu_refresh_dispatcher_->attach(&instance_->eventLoop());
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PreInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextCreated, fcitx::EventWatcherPhase::PreInputMethod,
        [this](fcitx::Event &event) {
          RequestSurroundingText(event);
          auto &ic_event = static_cast<fcitx::InputContextEvent &>(event);
          if (auto *ic = ic_event.inputContext(); ic != nullptr) {
            last_input_ic_ = ic->watch();
          }
        }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextDestroyed,
        fcitx::EventWatcherPhase::PreInputMethod,
        [this](fcitx::Event &event) { HandleInputContextDestroyed(event); }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextCommitString,
        fcitx::EventWatcherPhase::PostInputMethod,
        [this](fcitx::Event &event) { HandleCommitString(event); }));
  } else if (signal_event_loop != nullptr) {
    menu_refresh_dispatcher_->attach(signal_event_loop);
  }
  if (signal_bus != nullptr) {
    SetupDaemonSignalMonitor(signal_bus);
  } else if (instance_ != nullptr) {
    SetupDaemonSignalMonitor();
  }
}

FcitxVinpstAddon::~FcitxVinpstAddon() {
  menu_refresh_lifetime_.reset();
  ++scene_menu_refresh_seq_;
  ++asr_menu_refresh_seq_;
  if (menu_refresh_dispatcher_ != nullptr &&
      menu_refresh_dispatcher_->eventLoop() != nullptr) {
    menu_refresh_dispatcher_->detach();
  }
}

void FcitxVinpstAddon::reloadConfig() {
  frontend_settings_ = LoadFrontendSettings();
  context_history_.Reload();
  ApplyFrontendSettings();
}

void FcitxVinpstAddon::save() {
  if (!SaveFrontendSettings(frontend_settings_)) {
    const auto message = FrontendText("Failed to save frontend configuration.");
    FCITX_ERROR() << "fcitx-vinpst failed to save frontend configuration";
    Notify(FrontendNotificationKind::Error, message);
  }
}

const fcitx::Configuration *FcitxVinpstAddon::getConfig() const {
  frontend_config_ = BuildFrontendConfig(frontend_settings_);
  return frontend_config_.get();
}

void FcitxVinpstAddon::setConfig(const fcitx::RawConfig &config) {
  auto frontend_config = BuildFrontendConfig(frontend_settings_);
  frontend_config->load(config, true);
  frontend_settings_ = frontend_config->settings();
  ApplyFrontendSettings();
  save();
}

void FcitxVinpstAddon::SetupDaemonSignalMonitor() {
  auto *dbus_addon = instance_->addonManager().addon("dbus");
  if (dbus_addon == nullptr) {
    FCITX_WARN()
        << "fcitx-vinpst DBus module is unavailable; daemon signals are disabled";
    return;
  }
  auto *bus = dbus_addon->call<fcitx::IDBusModule::bus>();
  SetupDaemonSignalMonitor(bus);
}

void FcitxVinpstAddon::SetupDaemonSignalMonitor(fcitx::dbus::Bus *bus) {
  daemon_bus_ = bus;
  notifier_dbus_ = std::make_unique<FcitxNotifierDbusObject>(
      [this](std::string_view code, std::string_view subject, std::string_view detail,
             std::string_view raw_message) {
        const auto [kind, message] =
            PlanStructuredDaemonNotification(code, subject, detail, raw_message);
        HandleDaemonNotification(kind, message);
      });
  if (!bus->addObjectVTable(std::string(dbus::kNotifierObjectPath),
                            std::string(dbus::kNotifierInterface), *notifier_dbus_)) {
    FCITX_WARN() << "fcitx-vinpst failed to register frontend notifier DBus object";
    notifier_dbus_.reset();
  }
  daemon_signal_monitor_ = std::make_unique<FcitxDaemonSignalMonitor>(
      bus,
      DaemonSignalCallbacks{
          .service_availability_changed =
              [this](bool available) { HandleDaemonAvailability(available); },
          .status_changed =
              [this](std::string_view status) { HandleDaemonStatus(status); },
          .recognition_result =
              [this](std::string_view payload) { HandleRecognitionResult(payload); },
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
    FCITX_WARN() << "fcitx-vinpst failed to subscribe to daemon notifications";
    daemon_signal_monitor_.reset();
  }
}

void FcitxVinpstAddon::HandleDaemonAvailability(bool available) {
  daemon_client_.reset();
  awaiting_result_terminal_status_ = false;
  if (available) {
    ClearDaemonSyncFailure();
  }
  auto *ic = bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
  static_cast<void>(
      ExecuteDaemonControl(VINPST_FCITX_DAEMON_CONTROL_EVENT_AVAILABILITY_CHANGED, ic,
                           {}, available, false));
}

void FcitxVinpstAddon::HandleDaemonStatus(std::string_view status) {
  if (awaiting_result_terminal_status_) {
    awaiting_result_terminal_status_ = false;
    if (status == "idle" || status == "error") {
      return;
    }
  }
  auto *ic = bridge_.recording() ? active_trigger_ic_.get() : remote_status_ic_.get();
  if (ic == nullptr) {
    ic = last_input_ic_.get();
  }
  static_cast<void>(
      ExecuteDaemonControl(VINPST_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED, ic, status,
                           false, live_daemon_state_.CommandMode()));
}

void FcitxVinpstAddon::HandleRecognitionResult(std::string_view payload) {
  if (!bridge_.recording()) {
    return;
  }
  awaiting_result_terminal_status_ = true;
  auto *ic = active_trigger_ic_.get();
  static_cast<void>(ApplyBridgeOutcome(ic, bridge_.CompleteRecognitionResult(payload)));
  CancelTriggerStart();
  CancelTriggerStop();
  trigger_mode_controller_.RecordingStopped();
  active_trigger_ic_.unwatch();
}

void FcitxVinpstAddon::HandleRecognitionPartial(std::string_view partial_text) {
  if (!live_daemon_state_.UpdatePartial(partial_text, bridge_.recording())) {
    return;
  }
  UpdateLivePreedit();
}

void FcitxVinpstAddon::UpdateLivePreedit() {
  if (!bridge_.recording()) {
    return;
  }
  auto *active_ic = active_trigger_ic_.get();
  if (active_ic == nullptr) {
    return;
  }
  const auto preedit = live_daemon_state_.Preedit();
  if (preedit.empty()) {
    return;
  }
  const auto outcome = BuildPreeditOutcome(preedit, bridge_.command_mode());
  ApplyBridgeOutcomeToInputContext(outcome, active_ic);
}

void FcitxVinpstAddon::ResetLiveSignalState() {
  live_daemon_state_.Reset();
}

void FcitxVinpstAddon::ResetActiveRecording(fcitx::InputContext *ic) {
  CancelTriggerStart();
  CancelTriggerStop();
  awaiting_result_terminal_status_ = false;
  bridge_.Reset();
  trigger_mode_controller_.RecordingStopped();
  if (ic != nullptr) {
    BridgeOutcome clear;
    clear.kind = BridgeOutcome::Kind::Clear;
    ApplyBridgeOutcomeToInputContext(clear, ic);
  }
  active_trigger_ic_.unwatch();
  ResetLiveSignalState();
}

void FcitxVinpstAddon::HandleDaemonNotification(FrontendNotificationKind kind,
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
    daemon_client_.reset();
    ResetActiveRecording(active_ic);
  }
  Notify(kind, message);
}

void FcitxVinpstAddon::Notify(FrontendNotificationKind kind, std::string_view message) {
  if (message.empty()) {
    return;
  }
  SendFrontendNotification(instance_, BuildFrontendNotification(kind, message));
}

void FcitxVinpstAddon::ApplyFrontendSettings() {
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

SdBusDaemonClient *FcitxVinpstAddon::EnsureDaemonClient(std::string *error) {
  if (!DaemonSyncAllowed()) {
    if (error != nullptr) {
      *error = FrontendText("Voice input daemon is unavailable.");
    }
    return nullptr;
  }
  if (daemon_client_ == nullptr) {
    daemon_client_ = SdBusDaemonClient::ConnectSession(error);
    if (daemon_client_ == nullptr) {
      NoteDaemonSyncFailure();
    }
  }
  return daemon_client_.get();
}

bool FcitxVinpstAddon::DaemonSyncAllowed() const {
  return std::chrono::steady_clock::now() >= daemon_sync_blocked_until_;
}

void FcitxVinpstAddon::NoteDaemonSyncFailure() {
  daemon_sync_blocked_until_ =
      std::chrono::steady_clock::now() + kDaemonFailureCooldown;
}

void FcitxVinpstAddon::ClearDaemonSyncFailure() {
  daemon_sync_blocked_until_ = {};
}

AppliedOutcome FcitxVinpstAddon::DispatchPreparedDaemonCall(fcitx::InputContext *ic,
                                                            std::string_view method,
                                                            bool has_argument,
                                                            bool result_via_signal) {
  if (daemon_bus_ == nullptr || !DaemonSyncAllowed()) {
    FCITX_WARN()
        << "fcitx-vinpst async daemon call unavailable before dispatch: method="
        << method << " bus=" << (daemon_bus_ != nullptr)
        << " cooldown=" << !DaemonSyncAllowed();
    NoteDaemonSyncFailure();
    return ApplyBridgeOutcome(
        ic,
        bridge_.Complete(false, FrontendText("Voice input daemon is unavailable.")));
  }

  std::string argument;
  if (has_argument && !bridge_.PendingArgument(&argument)) {
    FCITX_WARN() << "fcitx-vinpst async daemon call missing prepared argument: method="
                 << method;
    NoteDaemonSyncFailure();
    return ApplyBridgeOutcome(
        ic,
        bridge_.Complete(false, FrontendText("Voice input daemon is unavailable.")));
  }

  const std::string service(dbus::kServiceBusName);
  const std::string path(dbus::kServiceObjectPath);
  const std::string interface(dbus::kServiceInterface);
  const std::string method_name(method);
  auto message = daemon_bus_->createMethodCall(service.c_str(), path.c_str(),
                                               interface.c_str(), method_name.c_str());
  if (has_argument) {
    message << argument;
  }

  auto callback = [this, result_via_signal](fcitx::dbus::Message &reply) {
    auto slot = result_via_signal ? std::move(pending_stop_call_slot_)
                                  : std::move(pending_start_call_slot_);
    if (!reply || reply.isError()) {
      NoteDaemonSyncFailure();
      std::string error = reply ? reply.errorMessage() : std::string{};
      if (error.empty()) {
        error = FrontendText("Voice input daemon is unavailable.");
      }
      auto *active_ic = active_trigger_ic_.get();
      if (result_via_signal) {
        static_cast<void>(
            ApplyBridgeOutcome(active_ic, bridge_.Complete(false, error)));
      } else {
        bridge_.Reset();
        static_cast<void>(ApplyDaemonUnavailable(active_ic, std::move(error)));
      }
      return true;
    }

    ClearDaemonSyncFailure();
    return true;
  };

  auto pending = message.callAsync(kDbusCallTimeoutUsec, std::move(callback));
  if (!pending) {
    FCITX_WARN() << "fcitx-vinpst failed to queue async daemon call: method=" << method;
    NoteDaemonSyncFailure();
    return ApplyBridgeOutcome(
        ic,
        bridge_.Complete(false, FrontendText("Voice input daemon is unavailable.")));
  }
  if (result_via_signal) {
    pending_stop_call_slot_ = std::move(pending);
    return AppliedOutcome::None;
  } else {
    pending_start_call_slot_ = std::move(pending);
    const auto started = bridge_.Complete(true, {});
    if (started.kind != BridgeOutcome::Kind::Preedit) {
      return ApplyBridgeOutcome(ic, started);
    }
    const auto applied = ApplyBridgeOutcome(
        ic, BuildPreeditOutcome("... Starting ...", started.replace_selection));
    return applied == AppliedOutcome::Preedit ? AppliedOutcome::PendingStart : applied;
  }
}

AppliedOutcome FcitxVinpstAddon::ApplyDaemonUnavailable(fcitx::InputContext *ic,
                                                        std::string error) {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Error;
  outcome.text = error.empty() ? FrontendText("Voice input daemon is unavailable.")
                               : std::move(error);
  FCITX_WARN() << "fcitx-vinpst daemon unavailable: " << outcome.text;
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinpstAddon::ApplyBridgeOutcome(fcitx::InputContext *ic,
                                                    const BridgeOutcome &outcome) {
  if (outcome.kind == BridgeOutcome::Kind::None) {
    return AppliedOutcome::None;
  }
  HideResultMenu();
  ApplyContextHistory(outcome);
  ClearRemoteDaemonStatus();
  auto display_outcome = outcome;
  if (outcome.kind == BridgeOutcome::Kind::Preedit ||
      outcome.kind == BridgeOutcome::Kind::Error) {
    display_outcome.text = FrontendText(outcome.text);
  }
  if (outcome.kind == BridgeOutcome::Kind::Preedit && bridge_.recording()) {
    live_daemon_state_.BeginStatus(dbus::kStatusRecording, bridge_.command_mode());
  }
  if (outcome.kind == BridgeOutcome::Kind::Error) {
    ResetLiveSignalState();
    daemon_client_.reset();
    Notify(FrontendNotificationKind::Error, display_outcome.text);
  } else if (!bridge_.recording()) {
    ResetLiveSignalState();
  }
  const auto replace_selection = display_outcome.replace_selection;
  const auto applied = ApplyBridgeOutcomeToInputContext(
      display_outcome, ic,
      [this, replace_selection](fcitx::InputContext *selected_context,
                                const PresentedCandidate &candidate) {
        result_menu_ic_.unwatch();
        if (!candidate.context_source.empty()) {
          context_flush_event_.reset();
          context_history_.AppendEntry(candidate.text, candidate.context_source);
        }
        if (candidate.suppress_commit_context && !candidate.text.empty()) {
          context_flush_event_.reset();
          context_history_.SuppressNext(candidate.text);
        }
        ApplyResultCandidateSelection(selected_context, candidate, replace_selection);
      });
  if (applied == AppliedOutcome::CandidateMenu && ic != nullptr) {
    result_menu_ic_ = ic->watch();
  }
  return applied;
}

void FcitxVinpstAddon::ApplyContextHistory(const BridgeOutcome &outcome) {
  if (!outcome.context_entries.empty() || outcome.suppress_commit_context) {
    context_flush_event_.reset();
  }
  for (const auto &entry : outcome.context_entries) {
    context_history_.AppendEntry(entry.text, entry.source);
  }
  if (outcome.suppress_commit_context && !outcome.text.empty()) {
    context_history_.SuppressNext(outcome.text);
  }
}

void FcitxVinpstAddon::HandleCommitString(fcitx::Event &event) {
  auto &commit = static_cast<fcitx::CommitStringEvent &>(event);
  auto *ic = commit.inputContext();
  if (ic == nullptr) {
    return;
  }
  const auto context = reinterpret_cast<std::size_t>(ic);
  if (context_history_.UserCommit(context, commit.text())) {
    ScheduleContextFlush();
  } else {
    context_flush_event_.reset();
  }
}

void FcitxVinpstAddon::HandleInputContextDestroyed(fcitx::Event &event) {
  auto &destroyed = static_cast<fcitx::InputContextEvent &>(event);
  if (auto *ic = destroyed.inputContext(); ic != nullptr) {
    context_history_.ContextDestroyed(reinterpret_cast<std::size_t>(ic));
  }
}

void FcitxVinpstAddon::ScheduleContextFlush() {
  context_flush_event_.reset();
  if (instance_ == nullptr) {
    return;
  }
  context_flush_event_ = instance_->eventLoop().addTimeEvent(
      CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 5'000'000, 0,
      [this](fcitx::EventSourceTime *, std::uint64_t) {
        context_history_.Flush();
        return false;
      });
  context_flush_event_->setOneShot();
}

std::optional<AppliedOutcome>
FcitxVinpstAddon::ExecuteDaemonControl(std::uint8_t event, fcitx::InputContext *ic,
                                       std::string_view status, bool flag,
                                       bool command_mode) {
  const VinpstFcitxDaemonControlView control{
      .event = event,
      .status = ToRustStringView(status),
      .flag = static_cast<std::uint8_t>(flag),
      .recording = static_cast<std::uint8_t>(bridge_.recording()),
      .remote_status_active = static_cast<std::uint8_t>(remote_status_ic_.isValid()),
  };
  const auto plan = DecodeDaemonControlPlan(vinpst_fcitx_daemon_control_plan(&control));
  if (!plan.has_value()) {
    FCITX_ERROR() << "fcitx-vinpst received an invalid Rust daemon control plan";
    return std::nullopt;
  }
  if (event == VINPST_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED &&
      *plan != DaemonControlPlan::None) {
    live_daemon_state_.UpdateStatus(status);
  }
  switch (*plan) {
  case DaemonControlPlan::None:
    return std::nullopt;
  case DaemonControlPlan::ResetUnavailable: {
    HideSceneMenu();
    HideAsrMenu();
    remote_status_ic_.unwatch();
    ResetActiveRecording(ic);
    BridgeOutcome error;
    error.kind = BridgeOutcome::Kind::Error;
    error.text = "Voice input daemon is unavailable.";
    return ApplyBridgeOutcome(ic, error);
  }
  case DaemonControlPlan::ClearRemoteStatus:
    ClearRemoteDaemonStatus();
    ResetLiveSignalState();
    return AppliedOutcome::None;
  case DaemonControlPlan::ResetLocalRecording:
    ResetActiveRecording(ic);
    return AppliedOutcome::None;
  case DaemonControlPlan::UpdateLocalPreedit:
    UpdateLivePreedit();
    return AppliedOutcome::None;
  case DaemonControlPlan::PresentRemoteStatus:
    return PresentRemoteDaemonStatus(ic, status, command_mode);
  case DaemonControlPlan::AdoptExternalStatus:
    if (ic == nullptr ||
        !bridge_.AdoptExternalRecording(false, scene_menu_controller_)) {
      return AppliedOutcome::None;
    }
    ClearRemoteDaemonStatus();
    active_trigger_ic_ = ic->watch();
    live_daemon_state_.BeginStatus(status, false);
    UpdateLivePreedit();
    return AppliedOutcome::None;
  case DaemonControlPlan::AdoptAndStopNormal:
    ClearRemoteDaemonStatus();
    if (bridge_.PrepareAdoptAndStop(false, scene_menu_controller_)) {
      return DispatchPreparedDaemonCall(ic, kMethodStopRecording, true, true);
    }
    return ApplyDaemonUnavailable(ic, "Voice input daemon is unavailable.");
  case DaemonControlPlan::ClearDaemonError:
    ClearRemoteDaemonStatus();
    ResetLiveSignalState();
    return AppliedOutcome::None;
  }
  return std::nullopt;
}

std::optional<AppliedOutcome>
FcitxVinpstAddon::ReconcileDaemonStatusBeforeStart(fcitx::InputContext *ic,
                                                   TriggerKind kind) {
  if (bridge_.recording()) {
    return std::nullopt;
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  std::string status;
  if (client == nullptr || !client->GetStatus(&status, &error)) {
    if (client != nullptr) {
      NoteDaemonSyncFailure();
      daemon_client_.reset();
    }
    return std::nullopt;
  }
  ClearDaemonSyncFailure();

  const bool command_mode = kind == TriggerKind::Command;
  return ExecuteDaemonControl(VINPST_FCITX_DAEMON_CONTROL_EVENT_RECONCILE_BEFORE_START,
                              ic, status, command_mode, command_mode);
}

AppliedOutcome FcitxVinpstAddon::PresentRemoteDaemonStatus(fcitx::InputContext *ic,
                                                           std::string_view status,
                                                           bool command_mode) {
  live_daemon_state_.BeginStatus(status, command_mode);
  const auto preedit = live_daemon_state_.Preedit();
  if (preedit.empty()) {
    return AppliedOutcome::None;
  }
  if (ic != nullptr) {
    remote_status_ic_ = ic->watch();
  }
  const auto outcome = BuildPreeditOutcome(preedit, command_mode);
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

void FcitxVinpstAddon::ClearRemoteDaemonStatus() {
  auto *ic = remote_status_ic_.get();
  remote_status_ic_.unwatch();
  if (ic == nullptr) {
    return;
  }
  BridgeOutcome clear;
  clear.kind = BridgeOutcome::Kind::Clear;
  ApplyBridgeOutcomeToInputContext(clear, ic);
}

AppliedOutcome FcitxVinpstAddon::StartNormalRecording(fcitx::InputContext *ic) {
  if (!bridge_.PrepareStartNormal(scene_menu_controller_)) {
    return ApplyDaemonUnavailable(ic, "Voice input daemon is unavailable.");
  }
  return DispatchPreparedDaemonCall(ic, kMethodStartRecording, false, false);
}

AppliedOutcome FcitxVinpstAddon::StartCommandRecording(fcitx::InputContext *ic,
                                                       std::string_view selected_text,
                                                       std::string_view scene_id) {
  if (selected_text.empty()) {
    return ApplyBridgeOutcome(ic,
                              bridge_.StartCommand(nullptr, selected_text, scene_id));
  }

  if (!bridge_.PrepareStartCommand(selected_text, scene_id)) {
    return ApplyDaemonUnavailable(ic, "Voice input daemon is unavailable.");
  }
  return DispatchPreparedDaemonCall(ic, kMethodStartCommandRecording, true, false);
}

AppliedOutcome FcitxVinpstAddon::StopRecording(fcitx::InputContext *ic) {
  if (!bridge_.PrepareStop(scene_menu_controller_)) {
    return ApplyDaemonUnavailable(ic, "Voice input daemon is unavailable.");
  }
  return DispatchPreparedDaemonCall(ic, kMethodStopRecording, true, true);
}

AppliedOutcome FcitxVinpstAddon::ApplyTriggerAction(fcitx::InputContext *ic,
                                                    FcitxTriggerAction action,
                                                    std::string_view selected_text) {
  if (ic != nullptr) {
    last_input_ic_ = ic->watch();
  }
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

void FcitxVinpstAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  if (key_event.inputContext() != nullptr) {
    last_input_ic_ = key_event.inputContext()->watch();
  }
  if (HandleResultMenuKeyEvent(key_event)) {
    return;
  }
  if (HandleSceneMenuKeyEvent(key_event)) {
    return;
  }
  if (HandleAsrMenuKeyEvent(key_event)) {
    return;
  }
  const auto action = trigger_policy_.Classify(key_event);
  if (action == FcitxTriggerAction::None) {
    return;
  }

  FCITX_INFO() << "fcitx-vinpst handling trigger " << TriggerActionName(action);
  if (action == FcitxTriggerAction::ShowSceneMenu ||
      action == FcitxTriggerAction::ShowAsrMenu) {
    if (bridge_.PlanTrigger(action) == FrontendTriggerIntent::None) {
      return;
    }
    ApplyTriggerAction(key_event.inputContext(), action);
    key_event.filterAndAccept();
    return;
  }
  if (action == FcitxTriggerAction::ConsumeSceneMenuRelease ||
      action == FcitxTriggerAction::ConsumeAsrMenuRelease) {
    key_event.filterAndAccept();
    return;
  }

  const bool normal = action == FcitxTriggerAction::StartNormal ||
                      action == FcitxTriggerAction::StopNormal;
  const auto kind = normal ? TriggerKind::Normal : TriggerKind::Command;
  const auto trigger_time = trigger_event_time_mapper_.Resolve(
      key_event.time(), TriggerModeController::Clock::now());
  TriggerModeAction mode_action = TriggerModeAction::None;
  if (key_event.isRelease()) {
    mode_action =
        trigger_mode_controller_.OnRelease(kind, key_event.key(), trigger_time);
  } else {
    mode_action = trigger_mode_controller_.OnPress(kind, key_event.key(), trigger_time,
                                                   bridge_.recording());
    if (mode_action != TriggerModeAction::None &&
        mode_action != TriggerModeAction::Consume) {
      CancelTriggerStop();
    }
  }
  HandleTriggerModeAction(key_event.inputContext(), mode_action);
  key_event.filterAndAccept();
}

void FcitxVinpstAddon::HandleTriggerModeAction(fcitx::InputContext *ic,
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

void FcitxVinpstAddon::ScheduleTriggerStart(fcitx::InputContext *ic) {
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

void FcitxVinpstAddon::CancelTriggerStart() {
  if (pending_trigger_start_event_ != nullptr) {
    pending_trigger_start_event_->setEnabled(false);
    pending_trigger_start_event_.reset();
  }
  pending_trigger_ic_.unwatch();
}

void FcitxVinpstAddon::ScheduleTriggerStop(fcitx::InputContext *fallback_ic) {
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

void FcitxVinpstAddon::CancelTriggerStop() {
  if (pending_trigger_stop_event_ != nullptr) {
    pending_trigger_stop_event_->setEnabled(false);
    pending_trigger_stop_event_.reset();
  }
}

void FcitxVinpstAddon::StopActiveRecording(fcitx::InputContext *fallback_ic) {
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

} // namespace vinpst_fcitx_bridge
