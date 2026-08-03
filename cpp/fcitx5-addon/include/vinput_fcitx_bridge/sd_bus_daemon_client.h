#pragma once

#include "vinput_fcitx_bridge/menu_snapshot.h"

#include <cstdint>
#include <memory>
#include <string>
#include <string_view>

struct VinputFcitxDaemonClient;

namespace vinput_fcitx_bridge {

class SdBusDaemonClient final {
public:
  static std::unique_ptr<SdBusDaemonClient> ConnectSession(std::string *error);

  ~SdBusDaemonClient();
  SdBusDaemonClient(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient &operator=(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient(SdBusDaemonClient &&) = delete;
  SdBusDaemonClient &operator=(SdBusDaemonClient &&) = delete;

  bool GetStatus(std::string *status, std::string *error);
  bool GetSceneState(SceneStateSnapshot *state, std::string *error);
  bool SetActiveScene(std::string_view scene_id, bool *persisted, std::string *error);
  bool GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state, std::string *error);
  bool SetActiveAsrTarget(std::string_view provider_id, std::string_view model_value,
                          bool *persisted, std::string *error);
  const ::VinputFcitxDaemonClient *raw_handle() const;

private:
  explicit SdBusDaemonClient(::VinputFcitxDaemonClient *client);

  bool CallStringReply(std::uint8_t operation, std::string_view first,
                       std::string_view second, std::string *reply, std::string *error);
  bool CallBoolReply(std::uint8_t operation, std::string_view first,
                     std::string_view second, bool *reply, std::string *error);

  ::VinputFcitxDaemonClient *client_ = nullptr;
};

} // namespace vinput_fcitx_bridge
