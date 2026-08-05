#pragma once

#include <memory>
#include <string>
#include <string_view>

struct VinpstFcitxDaemonClient;

namespace vinpst_fcitx_bridge {

class AsrMenuController;
class SceneMenuController;

class SdBusDaemonClient final {
public:
  static std::unique_ptr<SdBusDaemonClient> ConnectSession(std::string *error);

  ~SdBusDaemonClient();
  SdBusDaemonClient(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient &operator=(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient(SdBusDaemonClient &&) = delete;
  SdBusDaemonClient &operator=(SdBusDaemonClient &&) = delete;

  bool GetStatus(std::string *status, std::string *error);
  bool RefreshSceneMenuController(SceneMenuController *controller, std::string *error);
  bool SetActiveScene(SceneMenuController *controller, std::string_view scene_id,
                      bool *persisted, std::string *error);
  bool RefreshAsrMenuController(AsrMenuController *controller, std::string *error);
  bool SetActiveAsrTarget(std::string_view provider_id, std::string_view model_value,
                          bool *persisted, std::string *error);
  const ::VinpstFcitxDaemonClient *raw_handle() const;

private:
  struct Impl;

  explicit SdBusDaemonClient(::VinpstFcitxDaemonClient *client);

  std::unique_ptr<Impl> impl_;
};

} // namespace vinpst_fcitx_bridge
