#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include "vinput_fcitx_ffi.h"

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

std::optional<VinputFcitxDaemonResponseView>
ResponseView(const VinputFcitxDaemonResponse *response) {
  VinputFcitxDaemonResponseView view{};
  if (response == nullptr || vinput_fcitx_daemon_response_view(response, &view) == 0) {
    return std::nullopt;
  }
  return view;
}

ResponsePtr Call(VinputFcitxDaemonClient *client, std::uint8_t operation,
                 std::string_view first, std::string_view second,
                 std::uint8_t expected_kind, std::string *error) {
  ResponsePtr response(vinput_fcitx_daemon_client_call(
      client, operation, Bytes(first), first.size(), Bytes(second), second.size()));
  const auto view = ResponseView(response.get());
  if (!view.has_value()) {
    SetError(error, "Voice input daemon request failed before receiving a response.");
    return {};
  }
  if (view->kind == VINPUT_FCITX_DAEMON_RESPONSE_ERROR) {
    SetError(error, CopyText(view->text));
    return {};
  }
  if (view->kind != expected_kind) {
    SetError(error, "Voice input daemon returned an unexpected response type.");
    return {};
  }
  return response;
}

} // namespace

std::unique_ptr<SdBusDaemonClient>
SdBusDaemonClient::ConnectSession(std::string *error) {
  VinputFcitxDaemonResponse *raw_error = nullptr;
  auto *client = vinput_fcitx_daemon_client_connect(&raw_error);
  ResponsePtr error_response(raw_error);
  if (client == nullptr) {
    const auto view = ResponseView(error_response.get());
    SetError(error, view.has_value() ? CopyText(view->text)
                                     : "Failed to connect to the session D-Bus.");
    return nullptr;
  }
  return std::unique_ptr<SdBusDaemonClient>(new SdBusDaemonClient(client));
}

SdBusDaemonClient::SdBusDaemonClient(VinputFcitxDaemonClient *client)
    : client_(client) {}

SdBusDaemonClient::~SdBusDaemonClient() {
  vinput_fcitx_daemon_client_free(client_);
}

bool SdBusDaemonClient::CallNoReply(std::uint8_t operation, std::string_view first,
                                    std::string_view second, std::string *error) {
  return static_cast<bool>(Call(client_, operation, first, second,
                                VINPUT_FCITX_DAEMON_RESPONSE_NONE, error));
}

bool SdBusDaemonClient::CallStringReply(std::uint8_t operation, std::string_view first,
                                        std::string_view second, std::string *reply,
                                        std::string *error) {
  if (reply == nullptr) {
    SetError(error, "Missing string response output.");
    return false;
  }
  auto response =
      Call(client_, operation, first, second, VINPUT_FCITX_DAEMON_RESPONSE_TEXT, error);
  const auto view = ResponseView(response.get());
  if (!view.has_value()) {
    return false;
  }
  *reply = CopyText(view->text);
  return true;
}

bool SdBusDaemonClient::CallBoolReply(std::uint8_t operation, std::string_view first,
                                      std::string_view second, bool *reply,
                                      std::string *error) {
  if (reply == nullptr) {
    SetError(error, "Missing boolean response output.");
    return false;
  }
  auto response =
      Call(client_, operation, first, second, VINPUT_FCITX_DAEMON_RESPONSE_BOOL, error);
  const auto view = ResponseView(response.get());
  if (!view.has_value()) {
    return false;
  }
  *reply = view->bool_value != 0;
  return true;
}

bool SdBusDaemonClient::StartRecording(std::string *error) {
  return CallNoReply(VINPUT_FCITX_DAEMON_OPERATION_START_RECORDING, {}, {}, error);
}

bool SdBusDaemonClient::StartCommandRecording(std::string_view selected_text,
                                              std::string *error) {
  return CallNoReply(VINPUT_FCITX_DAEMON_OPERATION_START_COMMAND_RECORDING,
                     selected_text, {}, error);
}

bool SdBusDaemonClient::StopRecording(std::string_view scene_id,
                                      std::string *payload_json, std::string *error) {
  return CallStringReply(VINPUT_FCITX_DAEMON_OPERATION_STOP_RECORDING, scene_id, {},
                         payload_json, error);
}

bool SdBusDaemonClient::GetStatus(std::string *status, std::string *error) {
  return CallStringReply(VINPUT_FCITX_DAEMON_OPERATION_GET_STATUS, {}, {}, status,
                         error);
}

bool SdBusDaemonClient::GetSceneState(SceneStateSnapshot *state, std::string *error) {
  if (state == nullptr) {
    SetError(error, "Missing scene snapshot output.");
    return false;
  }
  auto response = Call(client_, VINPUT_FCITX_DAEMON_OPERATION_GET_SCENE_STATE, {}, {},
                       VINPUT_FCITX_DAEMON_RESPONSE_SCENE_SNAPSHOT, error);
  if (!response) {
    return false;
  }
  auto *snapshot = vinput_fcitx_daemon_response_take_scene_snapshot(response.get());
  if (snapshot == nullptr) {
    SetError(error, "Voice input daemon returned an invalid scene snapshot.");
    return false;
  }
  *state = SceneStateSnapshot::Adopt(snapshot);
  return true;
}

bool SdBusDaemonClient::SetActiveScene(std::string_view scene_id, bool *persisted,
                                       std::string *error) {
  return CallBoolReply(VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_SCENE, scene_id, {},
                       persisted, error);
}

bool SdBusDaemonClient::GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state,
                                               std::string *error) {
  if (state == nullptr) {
    SetError(error, "Missing ASR snapshot output.");
    return false;
  }
  auto response =
      Call(client_, VINPUT_FCITX_DAEMON_OPERATION_GET_ASR_DISPLAY_MENU_STATE, {}, {},
           VINPUT_FCITX_DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT, error);
  if (!response) {
    return false;
  }
  auto *snapshot =
      vinput_fcitx_daemon_response_take_asr_display_snapshot(response.get());
  if (snapshot == nullptr) {
    SetError(error, "Voice input daemon returned an invalid ASR snapshot.");
    return false;
  }
  *state = AsrDisplayMenuStateSnapshot::Adopt(snapshot);
  return true;
}

bool SdBusDaemonClient::SetActiveAsrProvider(std::string_view provider_id,
                                             bool *persisted, std::string *error) {
  return CallBoolReply(VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_ASR_PROVIDER,
                       provider_id, {}, persisted, error);
}

bool SdBusDaemonClient::SetActiveAsrTarget(std::string_view provider_id,
                                           std::string_view model_value,
                                           bool *persisted, std::string *error) {
  return CallBoolReply(VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_ASR_TARGET, provider_id,
                       model_value, persisted, error);
}

bool SdBusDaemonClient::GetTextAdapterState(std::string *state_json,
                                            std::string *error) {
  return CallStringReply(VINPUT_FCITX_DAEMON_OPERATION_GET_TEXT_ADAPTER_STATE, {}, {},
                         state_json, error);
}

bool SdBusDaemonClient::StartAdapter(std::string_view adapter_id, std::string *error) {
  return CallNoReply(VINPUT_FCITX_DAEMON_OPERATION_START_ADAPTER, adapter_id, {},
                     error);
}

bool SdBusDaemonClient::StopAdapter(std::string_view adapter_id, std::string *error) {
  return CallNoReply(VINPUT_FCITX_DAEMON_OPERATION_STOP_ADAPTER, adapter_id, {}, error);
}

bool SdBusDaemonClient::GetRuntimeStatus(std::string *status_json, std::string *error) {
  return CallStringReply(VINPUT_FCITX_DAEMON_OPERATION_GET_RUNTIME_STATUS, {}, {},
                         status_json, error);
}

} // namespace vinput_fcitx_bridge
