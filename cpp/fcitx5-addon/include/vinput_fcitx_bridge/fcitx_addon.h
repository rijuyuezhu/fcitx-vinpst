#pragma once

#include "vinput_fcitx_bridge/fcitx_config.h"
#include "vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h"
#include "vinput_fcitx_bridge/fcitx_key_trigger.h"
#include "vinput_fcitx_bridge/fcitx_menu_filter.h"
#include "vinput_fcitx_bridge/fcitx_notifications.h"
#include "vinput_fcitx_bridge/fcitx_outcome.h"
#include "vinput_fcitx_bridge/fcitx_trigger_mode.h"
#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/scene_defaults.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

#include <fcitx-utils/event.h>
#include <fcitx-utils/handlertable.h>
#include <fcitx/addoninstance.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/instance.h>

namespace vinput_fcitx_bridge {

class FcitxVinputAddon final : public fcitx::AddonInstance {
public:
  explicit FcitxVinputAddon(fcitx::Instance *instance);
  FcitxVinputAddon(fcitx::Instance *instance, fcitx::dbus::Bus *signal_bus);
  ~FcitxVinputAddon() override = default;

  FcitxVinputAddon(const FcitxVinputAddon &) = delete;
  FcitxVinputAddon &operator=(const FcitxVinputAddon &) = delete;
  FcitxVinputAddon(FcitxVinputAddon &&) = delete;
  FcitxVinputAddon &operator=(FcitxVinputAddon &&) = delete;

  void reloadConfig() override;
  void save() override;
  const fcitx::Configuration *getConfig() const override;
  void setConfig(const fcitx::RawConfig &config) override;

  fcitx::Instance *instance() const {
    return instance_;
  }
  const FrontendBridge &bridge() const {
    return bridge_;
  }
  const std::string &active_scene_id() const {
    return active_scene_id_;
  }
  AppliedOutcome TriggerNormal(fcitx::InputContext *ic,
                               std::string_view scene_id = kDefaultNormalSceneId);
  AppliedOutcome TriggerCommand(fcitx::InputContext *ic, std::string_view selected_text,
                                std::string_view scene_id = kDefaultCommandSceneId);
  AppliedOutcome ApplyTriggerAction(fcitx::InputContext *ic, FcitxTriggerAction action,
                                    std::string_view selected_text = "");

private:
  SdBusDaemonClient *EnsureDaemonClient(std::string *error);
  AppliedOutcome ApplyDaemonUnavailable(fcitx::InputContext *ic, std::string error);
  AppliedOutcome ApplyBridgeOutcome(fcitx::InputContext *ic,
                                    const BridgeOutcome &outcome);
  std::optional<AppliedOutcome>
  ReconcileDaemonStatusBeforeStart(fcitx::InputContext *ic, TriggerKind kind);
  AppliedOutcome PresentRemoteDaemonStatus(fcitx::InputContext *ic,
                                           std::string_view status, bool command_mode);
  void ClearRemoteDaemonStatus();
  void HandleKeyEvent(fcitx::Event &event);
  void ShowSceneMenu(fcitx::InputContext *ic);
  void RebuildSceneMenu(int page = 0);
  void HideSceneMenu();
  bool RefreshSceneState(std::string *error);
  bool HandleSceneMenuKeyEvent(fcitx::KeyEvent &event);
  void SelectScene(std::size_t index, fcitx::InputContext *ic);
  void ShowAsrMenu(fcitx::InputContext *ic);
  void RebuildAsrMenu(int page = 0);
  void HideAsrMenu();
  bool RefreshAsrMenuState(std::string *error);
  bool HandleAsrMenuKeyEvent(fcitx::KeyEvent &event);
  void SelectAsrTarget(std::size_t index, fcitx::InputContext *ic);
  void ApplyFrontendSettings();
  void SetupDaemonSignalMonitor();
  void SetupDaemonSignalMonitor(fcitx::dbus::Bus *bus);
  void HandleDaemonAvailability(bool available);
  void HandleDaemonStatus(std::string_view status);
  void HandleRecognitionPartial(std::string_view partial_text);
  void HandleDaemonNotification(const DaemonNotificationPayload &payload);
  void UpdateLivePreedit();
  void ResetLiveSignalState();
  void Notify(FrontendNotificationKind kind, std::string_view message);
  void HandleTriggerModeAction(fcitx::InputContext *ic, TriggerModeAction action);
  void ScheduleTriggerStart(fcitx::InputContext *ic);
  void CancelTriggerStart();
  void ScheduleTriggerStop(fcitx::InputContext *fallback_ic);
  void CancelTriggerStop();
  void StopActiveRecording(fcitx::InputContext *fallback_ic);

  fcitx::Instance *instance_ = nullptr;
  FrontendBridge bridge_;
  FrontendSettings frontend_settings_;
  FcitxKeyTriggerPolicy trigger_policy_;
  TriggerModeController trigger_mode_controller_;
  mutable std::unique_ptr<VinputFrontendConfig> frontend_config_;
  SceneStateSnapshot scene_state_;
  MenuFilterState scene_menu_filter_;
  std::vector<std::size_t> scene_menu_indices_;
  int scene_menu_page_ = 0;
  std::string active_scene_id_{kDefaultNormalSceneId};
  fcitx::InputContext *scene_menu_ic_ = nullptr;
  bool scene_menu_visible_ = false;
  AsrDisplayMenuStateSnapshot asr_menu_state_;
  MenuFilterState asr_menu_filter_;
  std::vector<std::size_t> asr_menu_indices_;
  int asr_menu_page_ = 0;
  fcitx::InputContext *asr_menu_ic_ = nullptr;
  bool asr_menu_visible_ = false;
  std::unique_ptr<fcitx::EventSourceTime> pending_trigger_start_event_;
  std::unique_ptr<fcitx::EventSourceTime> pending_trigger_stop_event_;
  fcitx::TrackableObjectReference<fcitx::InputContext> pending_trigger_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> active_trigger_ic_;
  fcitx::TrackableObjectReference<fcitx::InputContext> remote_status_ic_;
  bool remote_status_command_mode_ = false;
  std::unique_ptr<SdBusDaemonClient> daemon_client_;
  std::unique_ptr<FcitxDaemonSignalMonitor> daemon_signal_monitor_;
  std::string live_daemon_status_;
  std::string live_partial_text_;
  std::vector<std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>>>
      event_handlers_;
};

} // namespace vinput_fcitx_bridge
