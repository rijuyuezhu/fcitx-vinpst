#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <memory>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

struct ResponseDeleter {
  void operator()(VinputFcitxDaemonResponse *response) const {
    vinput_fcitx_daemon_response_free(response);
  }
};

using ResponsePtr = std::unique_ptr<VinputFcitxDaemonResponse, ResponseDeleter>;

const std::uint8_t *Bytes(std::string_view value) {
  return value.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(value.data());
}

std::string CopyText(VinputFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

void SetError(std::string *error, std::string message) {
  if (error != nullptr) {
    *error = std::move(message);
  }
}

void SetResponseError(std::string *error, const VinputFcitxDaemonResponse *response,
                      std::string_view fallback) {
  VinputFcitxDaemonResponseView view{};
  if (response != nullptr && vinput_fcitx_daemon_response_view(response, &view) != 0 &&
      view.is_error != 0) {
    auto message = CopyText(view.text);
    SetError(error, message.empty() ? std::string(fallback) : std::move(message));
    return;
  }
  SetError(error, std::string(fallback));
}

bool CopyTextResponse(ResponsePtr response, std::string *reply, std::string *error) {
  if (reply == nullptr) {
    SetError(error, "Missing string response output.");
    return false;
  }
  VinputFcitxDaemonResponseView view{};
  if (response == nullptr ||
      vinput_fcitx_daemon_response_view(response.get(), &view) == 0) {
    SetError(error, "Voice input daemon request failed before receiving a response.");
    return false;
  }
  if (view.is_error != 0) {
    SetResponseError(error, response.get(), "Voice input daemon request failed.");
    return false;
  }
  *reply = CopyText(view.text);
  return true;
}

} // namespace

std::unique_ptr<SdBusDaemonClient>
SdBusDaemonClient::ConnectSession(std::string *error) {
  VinputFcitxDaemonResponse *raw_error = nullptr;
  auto *client = vinput_fcitx_daemon_client_connect(&raw_error);
  ResponsePtr error_response(raw_error);
  if (client == nullptr) {
    SetResponseError(error, error_response.get(),
                     "Failed to connect to the session D-Bus.");
    return nullptr;
  }
  return std::unique_ptr<SdBusDaemonClient>(new SdBusDaemonClient(client));
}

SdBusDaemonClient::SdBusDaemonClient(VinputFcitxDaemonClient *client)
    : client_(client) {}

SdBusDaemonClient::~SdBusDaemonClient() {
  vinput_fcitx_daemon_client_free(client_);
}

bool SdBusDaemonClient::GetStatus(std::string *status, std::string *error) {
  return CopyTextResponse(ResponsePtr(vinput_fcitx_daemon_client_get_status(client_)),
                          status, error);
}

bool SdBusDaemonClient::GetSceneState(SceneStateSnapshot *state, std::string *error) {
  if (state == nullptr) {
    SetError(error, "Missing scene snapshot output.");
    return false;
  }
  VinputFcitxDaemonResponse *raw_error = nullptr;
  auto *snapshot = vinput_fcitx_daemon_client_get_scene_state(client_, &raw_error);
  ResponsePtr error_response(raw_error);
  if (snapshot == nullptr) {
    SetResponseError(error, error_response.get(),
                     "Voice input daemon returned an invalid scene snapshot.");
    return false;
  }
  *state = SceneStateSnapshot::Adopt(snapshot);
  return true;
}

bool SdBusDaemonClient::SetActiveScene(SceneStateSnapshot *state,
                                       std::string_view scene_id, bool *persisted,
                                       std::string *error) {
  if (state == nullptr || persisted == nullptr) {
    SetError(error, "Missing boolean response output.");
    return false;
  }
  std::uint8_t persisted_value = 0;
  VinputFcitxDaemonResponse *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_set_active_scene(client_, state->mutable_raw_handle(),
                                                  Bytes(scene_id), scene_id.size(),
                                                  &persisted_value, &raw_error) == 0) {
    ResponsePtr error_response(raw_error);
    SetResponseError(error, error_response.get(),
                     "Voice input daemon failed to set the active scene.");
    return false;
  }
  *persisted = persisted_value != 0;
  return true;
}

bool SdBusDaemonClient::GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state,
                                               std::string *error) {
  if (state == nullptr) {
    SetError(error, "Missing ASR snapshot output.");
    return false;
  }
  VinputFcitxDaemonResponse *raw_error = nullptr;
  auto *snapshot =
      vinput_fcitx_daemon_client_get_asr_display_state(client_, &raw_error);
  ResponsePtr error_response(raw_error);
  if (snapshot == nullptr) {
    SetResponseError(error, error_response.get(),
                     "Voice input daemon returned an invalid ASR snapshot.");
    return false;
  }
  *state = AsrDisplayMenuStateSnapshot::Adopt(snapshot);
  return true;
}

bool SdBusDaemonClient::SetActiveAsrTarget(std::string_view provider_id,
                                           std::string_view model_value,
                                           bool *persisted, std::string *error) {
  if (persisted == nullptr) {
    SetError(error, "Missing boolean response output.");
    return false;
  }
  std::uint8_t persisted_value = 0;
  VinputFcitxDaemonResponse *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_set_active_asr_target(
          client_, Bytes(provider_id), provider_id.size(), Bytes(model_value),
          model_value.size(), &persisted_value, &raw_error) == 0) {
    ResponsePtr error_response(raw_error);
    SetResponseError(error, error_response.get(),
                     "Voice input daemon failed to set the active ASR target.");
    return false;
  }
  *persisted = persisted_value != 0;
  return true;
}

const VinputFcitxDaemonClient *SdBusDaemonClient::raw_handle() const {
  return client_;
}

} // namespace vinput_fcitx_bridge
