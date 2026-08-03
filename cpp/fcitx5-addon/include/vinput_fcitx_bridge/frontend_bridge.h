#pragma once

#include "vinput_fcitx_bridge/recognition_payload.h"

#include <cstdint>
#include <string>
#include <string_view>

struct VinputFcitxDaemonClient;
struct VinputFcitxFrontendController;

namespace vinput_fcitx_bridge {

class SceneStateSnapshot;

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
  RecognitionPayload payload;
  bool command_mode = false;
};

class FrontendBridge {
public:
  FrontendBridge();
  ~FrontendBridge();

  FrontendBridge(const FrontendBridge &) = delete;
  FrontendBridge &operator=(const FrontendBridge &) = delete;
  FrontendBridge(FrontendBridge &&) = delete;
  FrontendBridge &operator=(FrontendBridge &&) = delete;

  BridgeOutcome StartNormal(const ::VinputFcitxDaemonClient *client,
                            const SceneStateSnapshot &scene_state);
  BridgeOutcome StartCommand(const ::VinputFcitxDaemonClient *client,
                             std::string_view selected_text, std::string_view scene_id);
  BridgeOutcome Stop(const ::VinputFcitxDaemonClient *client,
                     const SceneStateSnapshot &scene_state);
  BridgeOutcome AdoptAndStop(const ::VinputFcitxDaemonClient *client, bool command_mode,
                             const SceneStateSnapshot &scene_state);
  void Reset();

  FrontendTriggerIntent PlanTrigger(FrontendTriggerRequest request) const;
  bool recording() const;
  bool command_mode() const;

private:
  ::VinputFcitxFrontendController *controller_ = nullptr;
};

} // namespace vinput_fcitx_bridge
