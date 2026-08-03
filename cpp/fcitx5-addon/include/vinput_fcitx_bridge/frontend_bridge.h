#pragma once

#include "vinput_fcitx_bridge/recognition_payload.h"

#include <cstdint>
#include <optional>
#include <string>
#include <string_view>

struct VinputFcitxFrontendState;

namespace vinput_fcitx_bridge {

class DaemonClient {
public:
  virtual ~DaemonClient() = default;

  virtual bool StartRecording(std::string *error) = 0;
  virtual bool StartCommandRecording(std::string_view selected_text,
                                     std::string *error) = 0;
  virtual bool StopRecording(std::string_view scene_id, std::string *payload_json,
                             std::string *error) = 0;
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

  BridgeOutcome StartNormal(DaemonClient *client);
  BridgeOutcome StartNormal(DaemonClient *client, std::string_view scene_id);
  BridgeOutcome StartCommand(DaemonClient *client, std::string_view selected_text);
  BridgeOutcome StartCommand(DaemonClient *client, std::string_view selected_text,
                             std::string_view scene_id);
  BridgeOutcome Stop(DaemonClient *client, std::string_view scene_id);
  void AdoptRecording(bool command_mode, std::string_view scene_id);
  void Reset();

  bool recording() const;
  bool command_mode() const;

private:
  BridgeOutcome StartNormalWithScene(DaemonClient *client,
                                     std::optional<std::string_view> scene_id);
  BridgeOutcome StartCommandWithScene(DaemonClient *client,
                                      std::string_view selected_text,
                                      std::optional<std::string_view> scene_id);
  std::optional<std::string> ActiveSceneId() const;

  ::VinputFcitxFrontendState *state_ = nullptr;
};

} // namespace vinput_fcitx_bridge
