#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include <systemd/sd-bus.h>

#include <cerrno>
#include <cstdint>
#include <cstring>
#include <string>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr std::uint64_t kMethodCallTimeoutUsec = 60ULL * 1000ULL * 1000ULL;

void SetSdBusError(std::string *error, std::string_view action, int result,
                   const sd_bus_error &bus_error) {
  if (error == nullptr) {
    return;
  }

  std::string message(action);
  message += ": ";
  if (bus_error.message != nullptr) {
    message += bus_error.message;
  } else if (result < 0) {
    message += std::strerror(-result);
  } else {
    message += "unknown sd-bus error";
  }

  if (bus_error.name != nullptr) {
    message += " [";
    message += bus_error.name;
    message += ']';
  }
  *error = std::move(message);
}

void UnrefMessage(sd_bus_message *message) {
  if (message != nullptr) {
    sd_bus_message_unref(message);
  }
}

bool ReadStringReply(sd_bus_message *message, std::string *reply, std::string *error) {
  const char *wire_reply = nullptr;
  const int result = sd_bus_message_read(message, "s", &wire_reply);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read string reply", result, bus_error);
    return false;
  }

  if (reply != nullptr) {
    *reply = wire_reply != nullptr ? wire_reply : "";
  }
  return true;
}

bool ReadBoolReply(sd_bus_message *message, bool *reply, std::string *error) {
  int wire_reply = 0;
  const int result = sd_bus_message_read(message, "b", &wire_reply);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read boolean reply", result, bus_error);
    return false;
  }
  if (reply != nullptr) {
    *reply = wire_reply != 0;
  }
  return true;
}

bool ReadSceneStateReply(sd_bus_message *message, SceneStateSnapshot *state,
                         std::string *error) {
  const char *active_scene_id = nullptr;
  int result = sd_bus_message_read(message, "s", &active_scene_id);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read active scene", result, bus_error);
    return false;
  }

  result = sd_bus_message_enter_container(message, SD_BUS_TYPE_ARRAY, "(ss)");
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "enter scene array", result, bus_error);
    return false;
  }
  SceneStateSnapshot snapshot(active_scene_id != nullptr ? active_scene_id : "");
  if (!snapshot.valid()) {
    if (error != nullptr) {
      *error = "create Rust scene snapshot: allocation failed";
    }
    return false;
  }
  for (;;) {
    result = sd_bus_message_enter_container(message, SD_BUS_TYPE_STRUCT, "ss");
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "enter scene item", result, bus_error);
      return false;
    }
    if (result == 0) {
      break;
    }
    const char *id = nullptr;
    const char *label = nullptr;
    result = sd_bus_message_read(message, "ss", &id, &label);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "read scene item", result, bus_error);
      return false;
    }
    result = sd_bus_message_exit_container(message);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "exit scene item", result, bus_error);
      return false;
    }
    if (!snapshot.Add(id != nullptr ? id : "", label != nullptr ? label : "")) {
      if (error != nullptr) {
        *error = "append Rust scene snapshot row";
      }
      return false;
    }
  }
  result = sd_bus_message_exit_container(message);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "exit scene array", result, bus_error);
    return false;
  }
  if (state != nullptr) {
    *state = std::move(snapshot);
  }
  return true;
}

bool ReadAsrMenuStateReply(sd_bus_message *message, AsrMenuStateSnapshot *state,
                           std::string *error) {
  const char *target_provider_id = nullptr;
  const char *effective_provider_id = nullptr;
  const char *effective_model_id = nullptr;
  int reload_in_progress = 0;
  const char *last_error = nullptr;
  int result =
      sd_bus_message_read(message, "sssbs", &target_provider_id, &effective_provider_id,
                          &effective_model_id, &reload_in_progress, &last_error);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read ASR menu state", result, bus_error);
    return false;
  }

  result = sd_bus_message_enter_container(message, SD_BUS_TYPE_ARRAY, "(sss)");
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "enter ASR provider array", result, bus_error);
    return false;
  }
  std::vector<AsrMenuProviderItem> providers;
  for (;;) {
    result = sd_bus_message_enter_container(message, SD_BUS_TYPE_STRUCT, "sss");
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "enter ASR provider item", result, bus_error);
      return false;
    }
    if (result == 0) {
      break;
    }
    const char *id = nullptr;
    const char *kind = nullptr;
    const char *model = nullptr;
    result = sd_bus_message_read(message, "sss", &id, &kind, &model);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "read ASR provider item", result, bus_error);
      return false;
    }
    result = sd_bus_message_exit_container(message);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "exit ASR provider item", result, bus_error);
      return false;
    }
    providers.push_back(AsrMenuProviderItem{id != nullptr ? id : "",
                                            kind != nullptr ? kind : "",
                                            model != nullptr ? model : ""});
  }
  result = sd_bus_message_exit_container(message);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "exit ASR provider array", result, bus_error);
    return false;
  }
  if (state != nullptr) {
    state->target_provider_id = target_provider_id != nullptr ? target_provider_id : "";
    state->effective_provider_id =
        effective_provider_id != nullptr ? effective_provider_id : "";
    state->effective_model_id = effective_model_id != nullptr ? effective_model_id : "";
    state->reload_in_progress = reload_in_progress != 0;
    state->last_error = last_error != nullptr ? last_error : "";
    state->providers = std::move(providers);
  }
  return true;
}

bool ReadAsrTargetMenuStateReply(sd_bus_message *message,
                                 AsrTargetMenuStateSnapshot *state,
                                 std::string *error) {
  const char *target_provider_id = nullptr;
  const char *target_model_id = nullptr;
  const char *effective_provider_id = nullptr;
  const char *effective_model_id = nullptr;
  int reload_in_progress = 0;
  const char *last_error = nullptr;
  int result = sd_bus_message_read(
      message, "ssssbs", &target_provider_id, &target_model_id, &effective_provider_id,
      &effective_model_id, &reload_in_progress, &last_error);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read ASR target menu state", result, bus_error);
    return false;
  }

  result = sd_bus_message_enter_container(message, SD_BUS_TYPE_ARRAY, "(ssss)");
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "enter ASR target array", result, bus_error);
    return false;
  }
  std::vector<AsrTargetMenuItem> targets;
  for (;;) {
    result = sd_bus_message_enter_container(message, SD_BUS_TYPE_STRUCT, "ssss");
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "enter ASR target item", result, bus_error);
      return false;
    }
    if (result == 0) {
      break;
    }
    const char *provider_id = nullptr;
    const char *kind = nullptr;
    const char *item_id = nullptr;
    const char *model_value = nullptr;
    result = sd_bus_message_read(message, "ssss", &provider_id, &kind, &item_id,
                                 &model_value);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "read ASR target item", result, bus_error);
      return false;
    }
    result = sd_bus_message_exit_container(message);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "exit ASR target item", result, bus_error);
      return false;
    }
    targets.push_back(AsrTargetMenuItem{
        provider_id != nullptr ? provider_id : "", kind != nullptr ? kind : "",
        item_id != nullptr ? item_id : "", model_value != nullptr ? model_value : ""});
  }
  result = sd_bus_message_exit_container(message);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "exit ASR target array", result, bus_error);
    return false;
  }
  if (state != nullptr) {
    state->target_provider_id = target_provider_id != nullptr ? target_provider_id : "";
    state->target_model_id = target_model_id != nullptr ? target_model_id : "";
    state->effective_provider_id =
        effective_provider_id != nullptr ? effective_provider_id : "";
    state->effective_model_id = effective_model_id != nullptr ? effective_model_id : "";
    state->reload_in_progress = reload_in_progress != 0;
    state->last_error = last_error != nullptr ? last_error : "";
    state->targets = std::move(targets);
  }
  return true;
}

bool ReadAsrDisplayMenuStateReply(sd_bus_message *message,
                                  AsrDisplayMenuStateSnapshot *state,
                                  std::string *error) {
  const char *target_provider_id = nullptr;
  const char *target_model_id = nullptr;
  const char *effective_provider_id = nullptr;
  const char *effective_model_id = nullptr;
  int reload_in_progress = 0;
  const char *last_error = nullptr;
  int result = sd_bus_message_read(
      message, "ssssbs", &target_provider_id, &target_model_id, &effective_provider_id,
      &effective_model_id, &reload_in_progress, &last_error);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read ASR display menu state", result, bus_error);
    return false;
  }

  result = sd_bus_message_enter_container(message, SD_BUS_TYPE_ARRAY, "(sssss)");
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "enter ASR display target array", result, bus_error);
    return false;
  }
  AsrDisplayMenuStateSnapshot snapshot(
      target_provider_id != nullptr ? target_provider_id : "",
      target_model_id != nullptr ? target_model_id : "",
      effective_provider_id != nullptr ? effective_provider_id : "",
      effective_model_id != nullptr ? effective_model_id : "", reload_in_progress != 0,
      last_error != nullptr ? last_error : "");
  if (!snapshot.valid()) {
    if (error != nullptr) {
      *error = "create Rust ASR display snapshot: allocation failed";
    }
    return false;
  }
  for (;;) {
    result = sd_bus_message_enter_container(message, SD_BUS_TYPE_STRUCT, "sssss");
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "enter ASR display target item", result, bus_error);
      return false;
    }
    if (result == 0) {
      break;
    }
    const char *provider_id = nullptr;
    const char *kind = nullptr;
    const char *item_id = nullptr;
    const char *display_title = nullptr;
    const char *model_value = nullptr;
    result = sd_bus_message_read(message, "sssss", &provider_id, &kind, &item_id,
                                 &display_title, &model_value);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "read ASR display target item", result, bus_error);
      return false;
    }
    result = sd_bus_message_exit_container(message);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "exit ASR display target item", result, bus_error);
      return false;
    }
    if (!snapshot.Add(provider_id != nullptr ? provider_id : "",
                      kind != nullptr ? kind : "", item_id != nullptr ? item_id : "",
                      display_title != nullptr ? display_title : "",
                      model_value != nullptr ? model_value : "")) {
      if (error != nullptr) {
        *error = "append Rust ASR display snapshot row";
      }
      return false;
    }
  }
  result = sd_bus_message_exit_container(message);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "exit ASR display target array", result, bus_error);
    return false;
  }
  if (state != nullptr) {
    *state = std::move(snapshot);
  }
  return true;
}

bool ReadAsrBackendStateReply(sd_bus_message *message, AsrBackendStateSnapshot *state,
                              std::string *error) {
  const char *target_provider_id = nullptr;
  const char *target_model_id = nullptr;
  const char *effective_provider_id = nullptr;
  const char *effective_model_id = nullptr;
  const char *last_error = nullptr;
  int reload_in_progress = 0;
  int has_effective_backend = 0;
  int result = sd_bus_message_read(
      message, "sssssbb", &target_provider_id, &target_model_id, &effective_provider_id,
      &effective_model_id, &last_error, &reload_in_progress, &has_effective_backend);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "read ASR backend state reply", result, bus_error);
    return false;
  }

  result = sd_bus_message_enter_container(message, SD_BUS_TYPE_ARRAY, "s");
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "enter ASR remote endpoints", result, bus_error);
    return false;
  }

  std::vector<std::string> remote_endpoints;
  for (;;) {
    const char *endpoint = nullptr;
    result = sd_bus_message_read(message, "s", &endpoint);
    if (result < 0) {
      sd_bus_error bus_error = SD_BUS_ERROR_NULL;
      SetSdBusError(error, "read ASR remote endpoint", result, bus_error);
      return false;
    }
    if (result == 0) {
      break;
    }
    remote_endpoints.emplace_back(endpoint != nullptr ? endpoint : "");
  }

  result = sd_bus_message_exit_container(message);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "exit ASR remote endpoints", result, bus_error);
    return false;
  }

  if (state != nullptr) {
    state->target_provider_id = target_provider_id != nullptr ? target_provider_id : "";
    state->target_model_id = target_model_id != nullptr ? target_model_id : "";
    state->effective_provider_id =
        effective_provider_id != nullptr ? effective_provider_id : "";
    state->effective_model_id = effective_model_id != nullptr ? effective_model_id : "";
    state->last_error = last_error != nullptr ? last_error : "";
    state->reload_in_progress = reload_in_progress != 0;
    state->has_effective_backend = has_effective_backend != 0;
    state->remote_endpoints = std::move(remote_endpoints);
  }
  return true;
}

bool CallMethod(sd_bus *bus, std::string_view method, const char *signature,
                const char *argument, sd_bus_message **reply, std::string *error) {
  const auto bus_name = std::string(dbus::kServiceBusName);
  const auto object_path = std::string(dbus::kServiceObjectPath);
  const auto interface = std::string(dbus::kServiceInterface);
  const auto method_name = std::string(method);

  sd_bus_error bus_error = SD_BUS_ERROR_NULL;
  const int result =
      sd_bus_call_method(bus, bus_name.c_str(), object_path.c_str(), interface.c_str(),
                         method_name.c_str(), &bus_error, reply, signature, argument);
  if (result < 0) {
    SetSdBusError(error, method, result, bus_error);
    sd_bus_error_free(&bus_error);
    return false;
  }
  sd_bus_error_free(&bus_error);
  return true;
}

bool CallMethodWithTwoStrings(sd_bus *bus, std::string_view method,
                              std::string_view first, std::string_view second,
                              sd_bus_message **reply, std::string *error) {
  const auto bus_name = std::string(dbus::kServiceBusName);
  const auto object_path = std::string(dbus::kServiceObjectPath);
  const auto interface = std::string(dbus::kServiceInterface);
  const auto method_name = std::string(method);
  const auto first_argument = std::string(first);
  const auto second_argument = std::string(second);

  sd_bus_error bus_error = SD_BUS_ERROR_NULL;
  const int result =
      sd_bus_call_method(bus, bus_name.c_str(), object_path.c_str(), interface.c_str(),
                         method_name.c_str(), &bus_error, reply, "ss",
                         first_argument.c_str(), second_argument.c_str());
  if (result < 0) {
    SetSdBusError(error, method, result, bus_error);
    sd_bus_error_free(&bus_error);
    return false;
  }
  sd_bus_error_free(&bus_error);
  return true;
}

} // namespace

std::unique_ptr<SdBusDaemonClient>
SdBusDaemonClient::ConnectSession(std::string *error) {
  sd_bus *bus = nullptr;
  const int result = sd_bus_open_user(&bus);
  if (result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "connect user bus", result, bus_error);
    return nullptr;
  }
  const int timeout_result =
      sd_bus_set_method_call_timeout(bus, kMethodCallTimeoutUsec);
  if (timeout_result < 0) {
    sd_bus_error bus_error = SD_BUS_ERROR_NULL;
    SetSdBusError(error, "set user bus method timeout", timeout_result, bus_error);
    sd_bus_unref(bus);
    return nullptr;
  }
  return std::unique_ptr<SdBusDaemonClient>(new SdBusDaemonClient(bus));
}

SdBusDaemonClient::SdBusDaemonClient(sd_bus *bus) : bus_(bus) {}

SdBusDaemonClient::~SdBusDaemonClient() {
  if (bus_ != nullptr) {
    sd_bus_unref(bus_);
  }
}

bool SdBusDaemonClient::StartRecording(std::string *error) {
  return CallNoReply(dbus::kMethodStartRecording, error);
}

bool SdBusDaemonClient::StartCommandRecording(std::string_view selected_text,
                                              std::string *error) {
  return CallNoReplyWithString(dbus::kMethodStartCommandRecording, selected_text,
                               error);
}

bool SdBusDaemonClient::StopRecording(std::string_view scene_id,
                                      std::string *payload_json, std::string *error) {
  return CallStringReplyWithString(dbus::kMethodStopRecording, scene_id, payload_json,
                                   error);
}

bool SdBusDaemonClient::GetStatus(std::string *status, std::string *error) {
  return CallStringReply(dbus::kMethodGetStatus, status, error);
}

bool SdBusDaemonClient::GetAsrBackendState(AsrBackendStateSnapshot *state,
                                           std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, dbus::kMethodGetAsrBackendState, "", nullptr, &message,
                  error)) {
    return false;
  }

  const bool ok = ReadAsrBackendStateReply(message, state, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::GetSceneState(SceneStateSnapshot *state, std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, dbus::kMethodGetSceneState, "", nullptr, &message, error)) {
    return false;
  }
  const bool ok = ReadSceneStateReply(message, state, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::SetActiveScene(std::string_view scene_id, bool *persisted,
                                       std::string *error) {
  return CallBoolReplyWithString(dbus::kMethodSetActiveScene, scene_id, persisted,
                                 error);
}

bool SdBusDaemonClient::GetAsrMenuState(AsrMenuStateSnapshot *state,
                                        std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, dbus::kMethodGetAsrMenuState, "", nullptr, &message, error)) {
    return false;
  }
  const bool ok = ReadAsrMenuStateReply(message, state, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::SetActiveAsrProvider(std::string_view provider_id,
                                             bool *persisted, std::string *error) {
  return CallBoolReplyWithString(dbus::kMethodSetActiveAsrProvider, provider_id,
                                 persisted, error);
}

bool SdBusDaemonClient::GetAsrTargetMenuState(AsrTargetMenuStateSnapshot *state,
                                              std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, dbus::kMethodGetAsrTargetMenuState, "", nullptr, &message,
                  error)) {
    return false;
  }
  const bool ok = ReadAsrTargetMenuStateReply(message, state, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::GetAsrDisplayMenuState(AsrDisplayMenuStateSnapshot *state,
                                               std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, dbus::kMethodGetAsrDisplayMenuState, "", nullptr, &message,
                  error)) {
    return false;
  }
  const bool ok = ReadAsrDisplayMenuStateReply(message, state, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::SetActiveAsrTarget(std::string_view provider_id,
                                           std::string_view model_value,
                                           bool *persisted, std::string *error) {
  return CallBoolReplyWithTwoStrings(dbus::kMethodSetActiveAsrTarget, provider_id,
                                     model_value, persisted, error);
}

bool SdBusDaemonClient::GetTextAdapterState(std::string *state_json,
                                            std::string *error) {
  return CallStringReply(dbus::kMethodGetTextAdapterState, state_json, error);
}

bool SdBusDaemonClient::StartAdapter(std::string_view adapter_id, std::string *error) {
  return CallNoReplyWithString(dbus::kMethodStartAdapter, adapter_id, error);
}

bool SdBusDaemonClient::StopAdapter(std::string_view adapter_id, std::string *error) {
  return CallNoReplyWithString(dbus::kMethodStopAdapter, adapter_id, error);
}

bool SdBusDaemonClient::GetRuntimeStatus(std::string *status_json, std::string *error) {
  return CallStringReply(dbus::kMethodGetRuntimeStatus, status_json, error);
}

bool SdBusDaemonClient::CallNoReply(std::string_view method, std::string *error) {
  sd_bus_message *reply = nullptr;
  const bool ok = CallMethod(bus_, method, "", nullptr, &reply, error);
  UnrefMessage(reply);
  return ok;
}

bool SdBusDaemonClient::CallNoReplyWithString(std::string_view method,
                                              std::string_view value,
                                              std::string *error) {
  const auto argument = std::string(value);
  sd_bus_message *reply = nullptr;
  const bool ok = CallMethod(bus_, method, "s", argument.c_str(), &reply, error);
  UnrefMessage(reply);
  return ok;
}

bool SdBusDaemonClient::CallStringReply(std::string_view method, std::string *reply,
                                        std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, method, "", nullptr, &message, error)) {
    return false;
  }

  const bool ok = ReadStringReply(message, reply, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::CallStringReplyWithString(std::string_view method,
                                                  std::string_view value,
                                                  std::string *reply,
                                                  std::string *error) {
  const auto argument = std::string(value);
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, method, "s", argument.c_str(), &message, error)) {
    return false;
  }

  const bool ok = ReadStringReply(message, reply, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::CallBoolReplyWithString(std::string_view method,
                                                std::string_view value, bool *reply,
                                                std::string *error) {
  const auto argument = std::string(value);
  sd_bus_message *message = nullptr;
  if (!CallMethod(bus_, method, "s", argument.c_str(), &message, error)) {
    return false;
  }
  const bool ok = ReadBoolReply(message, reply, error);
  UnrefMessage(message);
  return ok;
}

bool SdBusDaemonClient::CallBoolReplyWithTwoStrings(std::string_view method,
                                                    std::string_view first,
                                                    std::string_view second,
                                                    bool *reply, std::string *error) {
  sd_bus_message *message = nullptr;
  if (!CallMethodWithTwoStrings(bus_, method, first, second, &message, error)) {
    return false;
  }
  const bool ok = ReadBoolReply(message, reply, error);
  UnrefMessage(message);
  return ok;
}

} // namespace vinput_fcitx_bridge
