#pragma once

#include "vinpst_fcitx_bridge/frontend_presentation.h"
#include "vinpst_fcitx_bridge/rust_handle.h"
#include "vinpst_fcitx_ffi.h"

#include <cstdint>
#include <string>
#include <string_view>

namespace vinpst_fcitx_bridge {

class SceneMenuController;

enum class FrontendTriggerRequest : std::uint8_t {
  None,
  StartNormal,
  StopNormal,
  StartCommand,
  StopCommand,
  ShowSceneMenu,
  ConsumeSceneMenuRelease,
  ShowAsrMenu,
  ConsumeAsrMenuRelease,
};

enum class FrontendTriggerIntent : std::uint8_t {
  None,
  StartNormal,
  StopNormal,
  StartCommand,
  StopCommand,
  ShowSceneMenu,
  ShowAsrMenu,
};

struct BridgeOutcome {
  enum class Kind : std::uint8_t { None, Preedit, Clear, Commit, CandidateMenu, Error };

  Kind kind = Kind::None;
  std::string text;
  CandidatePresentation candidate_menu;
  bool replace_selection = false;
};

class FrontendBridge {
public:
  FrontendBridge();
  ~FrontendBridge() = default;

  FrontendBridge(const FrontendBridge &) = delete;
  FrontendBridge &operator=(const FrontendBridge &) = delete;
  FrontendBridge(FrontendBridge &&) = delete;
  FrontendBridge &operator=(FrontendBridge &&) = delete;

  BridgeOutcome StartNormal(const ::VinpstFcitxDaemonClient *client,
                            const SceneMenuController &scene_controller);
  BridgeOutcome StartCommand(const ::VinpstFcitxDaemonClient *client,
                             std::string_view selected_text, std::string_view scene_id);
  BridgeOutcome Stop(const ::VinpstFcitxDaemonClient *client,
                     const SceneMenuController &scene_controller);
  BridgeOutcome AdoptAndStop(const ::VinpstFcitxDaemonClient *client, bool command_mode,
                             const SceneMenuController &scene_controller);
  void SetPresentationText(std::string original, std::string voice_command,
                           std::string cancel);
  void Reset();

  FrontendTriggerIntent PlanTrigger(FrontendTriggerRequest request) const;
  bool recording() const;
  bool command_mode() const;

private:
  using ControllerHandle = RustOwnedHandle<::VinpstFcitxFrontendController,
                                           vinpst_fcitx_frontend_controller_free>;

  ControllerHandle controller_;
  std::string original_text_ = "Original";
  std::string voice_command_text_ = "Voice Command";
  std::string cancel_text_ = "Cancel";
};

} // namespace vinpst_fcitx_bridge
