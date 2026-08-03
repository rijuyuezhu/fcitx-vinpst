#pragma once

#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/menu_snapshot.h"

#include <cstdint>
#include <memory>
#include <string>
#include <string_view>

struct VinputFcitxDaemonClient;

namespace vinput_fcitx_bridge {

class SdBusDaemonClient final : public DaemonClient {
public:
  static std::unique_ptr<SdBusDaemonClient> ConnectSession(std::string *error);

  ~SdBusDaemonClient() override;
  SdBusDaemonClient(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient &operator=(const SdBusDaemonClient &) = delete;
  SdBusDaemonClient(SdBusDaemonClient &&) = delete;
  SdBusDaemonClient &operator=(SdBusDaemonClient &&) = delete;

  bool StartRecording(std::string *error) override;
  bool StartCommandRecording(std::string_view selected_text,
                             std::string *error) override;
  bool StopRecording(std::string_view scene_id, std::string *payload_json,
                     std::string *error) override;
  bool GetStatus(std::string *status, std::string *error);
  bool GetSceneState(SceneStateSnapshot *state, std::string *error);
  bool SetActiveScene(std::string_view scene_id, bool *persisted, std::string *error);
  bool GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state, std::string *error);
  bool SetActiveAsrProvider(std::string_view provider_id, bool *persisted,
                            std::string *error);
  bool SetActiveAsrTarget(std::string_view provider_id, std::string_view model_value,
                          bool *persisted, std::string *error);
  bool GetTextAdapterState(std::string *state_json, std::string *error);
  bool StartAdapter(std::string_view adapter_id, std::string *error);
  bool StopAdapter(std::string_view adapter_id, std::string *error);
  bool GetRuntimeStatus(std::string *status_json, std::string *error);

private:
  explicit SdBusDaemonClient(::VinputFcitxDaemonClient *client);

  bool CallNoReply(std::uint8_t operation, std::string_view first,
                   std::string_view second, std::string *error);
  bool CallStringReply(std::uint8_t operation, std::string_view first,
                       std::string_view second, std::string *reply, std::string *error);
  bool CallBoolReply(std::uint8_t operation, std::string_view first,
                     std::string_view second, bool *reply, std::string *error);

  ::VinputFcitxDaemonClient *client_ = nullptr;
};

} // namespace vinput_fcitx_bridge
