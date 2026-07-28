#pragma once

#include "vinput_fcitx_bridge/fcitx_config.h"
#include "vinput_fcitx_bridge/fcitx_key_trigger.h"
#include "vinput_fcitx_bridge/fcitx_outcome.h"
#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/scene_defaults.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <memory>
#include <string>
#include <string_view>
#include <vector>

#include <fcitx-utils/handlertable.h>
#include <fcitx/addoninstance.h>
#include <fcitx/event.h>
#include <fcitx/instance.h>

namespace vinput_fcitx_bridge {

class FcitxVinputAddon final : public fcitx::AddonInstance {
public:
  explicit FcitxVinputAddon(fcitx::Instance *instance);
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
  void HandleKeyEvent(fcitx::Event &event);
  void ShowSceneMenu(fcitx::InputContext *ic);
  void HideSceneMenu();
  bool RefreshSceneState(std::string *error);
  bool HandleSceneMenuKeyEvent(fcitx::KeyEvent &event);
  void SelectScene(std::size_t index, fcitx::InputContext *ic);
  void ShowAsrMenu(fcitx::InputContext *ic);
  void HideAsrMenu();
  bool RefreshAsrMenuState(std::string *error);
  bool HandleAsrMenuKeyEvent(fcitx::KeyEvent &event);
  void SelectAsrTarget(std::size_t index, fcitx::InputContext *ic);
  void ApplyFrontendSettings();

  fcitx::Instance *instance_ = nullptr;
  FrontendBridge bridge_;
  FrontendSettings frontend_settings_;
  FcitxKeyTriggerPolicy trigger_policy_;
  mutable std::unique_ptr<VinputFrontendConfig> frontend_config_;
  SceneStateSnapshot scene_state_;
  std::vector<std::size_t> scene_menu_indices_;
  std::string active_scene_id_{kDefaultNormalSceneId};
  fcitx::InputContext *scene_menu_ic_ = nullptr;
  bool scene_menu_visible_ = false;
  AsrTargetMenuStateSnapshot asr_menu_state_;
  std::vector<std::size_t> asr_menu_indices_;
  fcitx::InputContext *asr_menu_ic_ = nullptr;
  bool asr_menu_visible_ = false;
  std::unique_ptr<SdBusDaemonClient> daemon_client_;
  std::vector<std::unique_ptr<fcitx::HandlerTableEntry<fcitx::EventHandler>>>
      event_handlers_;
};

} // namespace vinput_fcitx_bridge
