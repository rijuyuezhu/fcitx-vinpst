#pragma once

#include "vinput_fcitx_bridge/menu_snapshot.h"

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
  bool SetActiveScene(SceneStateSnapshot *state, std::string_view scene_id,
                      bool *persisted, std::string *error);
  bool GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state, std::string *error);
  bool SetActiveAsrTarget(std::string_view provider_id, std::string_view model_value,
                          bool *persisted, std::string *error);
  const ::VinputFcitxDaemonClient *raw_handle() const;

private:
  explicit SdBusDaemonClient(::VinputFcitxDaemonClient *client);

  ::VinputFcitxDaemonClient *client_ = nullptr;
};

} // namespace vinput_fcitx_bridge
