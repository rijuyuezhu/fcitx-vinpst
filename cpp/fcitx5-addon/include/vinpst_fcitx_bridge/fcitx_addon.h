#pragma once

#include "vinpst_fcitx_bridge/context_history_bridge.h"
#include "vinpst_fcitx_bridge/fcitx_config.h"
#include "vinpst_fcitx_bridge/fcitx_daemon_signal_monitor.h"
#include "vinpst_fcitx_bridge/fcitx_key_trigger.h"
#include "vinpst_fcitx_bridge/fcitx_menu_filter.h"
#include "vinpst_fcitx_bridge/fcitx_menu_projection.h"
#include "vinpst_fcitx_bridge/fcitx_notifications.h"
#include "vinpst_fcitx_bridge/fcitx_notifier_dbus_object.h"
#include "vinpst_fcitx_bridge/fcitx_outcome.h"
#include "vinpst_fcitx_bridge/fcitx_trigger_mode.h"
#include "vinpst_fcitx_bridge/frontend_bridge.h"
#include "vinpst_fcitx_bridge/sd_bus_daemon_client.h"

#include <atomic>
#include <chrono>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include <fcitx-utils/event.h>
#include <fcitx-utils/eventdispatcher.h>
#include <fcitx-utils/handlertable.h>
#include <fcitx/addoninstance.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/instance.h>

namespace vinpst_fcitx_bridge {

struct FcitxProjectedMenuState {
  MenuSessionState session;
  std::shared_ptr<MenuProjection> projection;
  fcitx::InputContext *input_context = nullptr;
};

class FcitxVinpstAddon final : public fcitx::AddonInstance {
public:
  explicit FcitxVinpstAddon(fcitx::Instance *instance);
  FcitxVinpstAddon(fcitx::Instance *instance, fcitx::dbus::Bus *signal_bus);
  ~FcitxVinpstAddon() override;

  FcitxVinpstAddon(const FcitxVinpstAddon &) = delete;
  FcitxVinpstAddon &operator=(const FcitxVinpstAddon &) = delete;
  FcitxVinpstAddon(FcitxVinpstAddon &&) = delete;
  FcitxVinpstAddon &operator=(FcitxVinpstAddon &&) = delete;

  void reloadConfig() override;
  void save() override;
  const fcitx::Configuration *getConfig() const override;
  void setConfig(const fcitx::RawConfig &config) override;

  AppliedOutcome ApplyTriggerAction(fcitx::InputContext *ic, FcitxTriggerAction action,
                                    std::string_view selected_text = "");

private:
  AppliedOutcome StartNormalRecording(fcitx::InputContext *ic);
  AppliedOutcome StartCommandRecording(fcitx::InputContext *ic,
                                       std::string_view selected_text,
                                       std::string_view scene_id = {});
  AppliedOutcome StopRecording(fcitx::InputContext *ic);
  SdBusDaemonClient *EnsureDaemonClient(std::string *error);
  AppliedOutcome ApplyDaemonUnavailable(fcitx::InputContext *ic, std::string error);
  AppliedOutcome ApplyBridgeOutcome(fcitx::InputContext *ic,
                                    const BridgeOutcome &outcome);
  std::optional<AppliedOutcome> ExecuteDaemonControl(std::uint8_t event,
                                                     fcitx::InputContext *ic,
                                                     std::string_view status, bool flag,
                                                     bool command_mode);
  std::optional<AppliedOutcome>
  ReconcileDaemonStatusBeforeStart(fcitx::InputContext *ic, TriggerKind kind);
  AppliedOutcome PresentRemoteDaemonStatus(fcitx::InputContext *ic,
                                           std::string_view status, bool command_mode);
  void ClearRemoteDaemonStatus();
  void HandleKeyEvent(fcitx::Event &event);
  void HandleCommitString(fcitx::Event &event);
  void HandleInputContextDestroyed(fcitx::Event &event);
  void ScheduleContextFlush();
  void ApplyContextHistory(const BridgeOutcome &outcome);
  bool HandleResultMenuKeyEvent(fcitx::KeyEvent &event);
  void HideResultMenu();
  void ShowSceneMenu(fcitx::InputContext *ic);
  void RebuildSceneMenu(int page = 0);
  void HideSceneMenu();
  bool RefreshSceneState(std::string *error);
  void RequestSceneMenuStateRefresh(fcitx::InputContext *ic);
  bool HandleSceneMenuKeyEvent(fcitx::KeyEvent &event);
  void ShowAsrMenu(fcitx::InputContext *ic);
  void RebuildAsrMenu(int page = 0);
  void HideAsrMenu();
  void RequestAsrMenuStateRefresh(fcitx::InputContext *ic);
  bool HandleAsrMenuKeyEvent(fcitx::KeyEvent &event);
  void ExecuteMenuControl(const ProjectedMenuControl &control, fcitx::InputContext *ic);
  void ApplyFrontendSettings();
  void SetupDaemonSignalMonitor();
  void SetupDaemonSignalMonitor(fcitx::dbus::Bus *bus);
  void HandleDaemonAvailability(bool available);
  void HandleDaemonStatus(std::string_view status);
  void HandleRecognitionResult(std::string_view payload);
  void HandleRecognitionPartial(std::string_view partial_text);
  void HandleDaemonNotification(FrontendNotificationKind kind,
                                std::string_view message);
  void UpdateLivePreedit();
  void ResetLiveSignalState();
  void ResetActiveRecording(fcitx::InputContext *ic);
  void Notify(FrontendNotificationKind kind, std::string_view message);
  void HandleTriggerModeAction(fcitx::InputContext *ic, TriggerModeAction action);
  void ScheduleTriggerStart(fcitx::InputContext *ic);
  void CancelTriggerStart();
  void ScheduleTriggerStop(fcitx::InputContext *fallback_ic);
  void CancelTriggerStop();
  void StopActiveRecording(fcitx::InputContext *fallback_ic);
  AppliedOutcome DispatchPreparedDaemonCall(fcitx::InputContext *ic,
                                            std::string_view method,
                                            bool has_argument,
                                            bool result_via_signal);
  bool DaemonSyncAllowed() const;
  void NoteDaemonSyncFailure();
  void ClearDaemonSyncFailure();

  fcitx::Instance *instance_ = nullptr;
  fcitx::dbus::Bus *daemon_bus_ = nullptr;
  FrontendBridge bridge_;
  ContextHistoryBridge context_history_;
  FrontendSettings frontend_settings_;
  FcitxKeyTriggerPolicy trigger_policy_;
  TriggerModeController trigger_mode_controller_;
  mutable std::unique_ptr<VinpstFrontendConfig> frontend_config_;
  SceneMenuController scene_menu_controller_;
  FcitxProjectedMenuState scene_menu_;
  AsrMenuController asr_menu_controller_;
  FcitxProjectedMenuState asr_menu_;
  std::unique_ptr<fcitx::EventSourceTime> pending_trigger_start_event_;
  std::unique_ptr<fcitx::EventSourceTime> pending_trigger_stop_event_;
  std::unique_ptr<fcitx::EventSourceTime> context_flush_event_;
  fcitx::TrackableObjectReference<fcitx::InputContext> pending_trigger_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> active_trigger_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> remote_status_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> last_input_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> result_menu_ic_;
  std::unique_ptr<SdBusDaemonClient> daemon_client_;
  std::unique_ptr<FcitxDaemonSignalMonitor> daemon_signal_monitor_;
  std::unique_ptr<FcitxNotifierDbusObject> notifier_dbus_;
  std::unique_ptr<fcitx::dbus::Slot> pending_start_call_slot_;
  std::unique_ptr<fcitx::dbus::Slot> pending_stop_call_slot_;
  std::chrono::steady_clock::time_point daemon_sync_blocked_until_{};
  std::shared_ptr<fcitx::EventDispatcher> menu_refresh_dispatcher_;
  std::shared_ptr<bool> menu_refresh_lifetime_ = std::make_shared<bool>(true);
  std::atomic<std::uint64_t> scene_menu_refresh_seq_{0};
  std::atomic<std::uint64_t> asr_menu_refresh_seq_{0};
  DaemonLivePresentationState live_daemon_state_;
  std::vector<std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>>>
      event_handlers_;
};

} // namespace vinpst_fcitx_bridge
