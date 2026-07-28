#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/scene_defaults.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <string_view>
#include <thread>

using vinput_fcitx_bridge::AsrBackendStateSnapshot;
using vinput_fcitx_bridge::AsrDisplayMenuStateSnapshot;
using vinput_fcitx_bridge::AsrMenuStateSnapshot;
using vinput_fcitx_bridge::AsrTargetMenuStateSnapshot;
using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FrontendBridge;
using vinput_fcitx_bridge::kDefaultCommandSceneId;
using vinput_fcitx_bridge::kDefaultNormalSceneId;
using vinput_fcitx_bridge::SceneStateSnapshot;
using vinput_fcitx_bridge::SdBusDaemonClient;

namespace {

std::unique_ptr<SdBusDaemonClient> ConnectWithRetry(std::string *error) {
  for (int attempt = 0; attempt < 50; ++attempt) {
    auto client = SdBusDaemonClient::ConnectSession(error);
    if (client != nullptr) {
      return client;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
  }
  return nullptr;
}

std::string ExpectedText(const char *env_name, const char *fallback) {
  const char *value = std::getenv(env_name);
  return value == nullptr ? std::string(fallback) : std::string(value);
}

std::string OptionalExpectedText(const char *env_name) {
  const char *value = std::getenv(env_name);
  return value == nullptr ? std::string() : std::string(value);
}

bool Contains(std::string_view haystack, std::string_view needle) {
  return haystack.find(needle) != std::string_view::npos;
}

bool ExpectRuntimeStatus(SdBusDaemonClient *client, std::string_view expected,
                         std::string *error) {
  std::string status_json;
  if (!client->GetRuntimeStatus(&status_json, error)) {
    return false;
  }
  if (!Contains(status_json, expected)) {
    if (error != nullptr) {
      *error = "runtime status missing expected marker: ";
      *error += expected;
      *error += " in ";
      *error += status_json;
    }
    return false;
  }
  return true;
}

bool ExpectConfiguredDiagnostics(SdBusDaemonClient *client, std::string *error) {
  AsrBackendStateSnapshot asr_state;
  if (!client->GetAsrBackendState(&asr_state, error)) {
    return false;
  }
  if (asr_state.target_provider_id.empty()) {
    if (error != nullptr) {
      *error = "ASR backend state did not include a target provider";
    }
    return false;
  }

  const auto expected_asr_provider =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_ASR_PROVIDER");
  if (!expected_asr_provider.empty()) {
    if (asr_state.target_provider_id != expected_asr_provider ||
        asr_state.effective_provider_id != expected_asr_provider ||
        !asr_state.has_effective_backend) {
      if (error != nullptr) {
        *error = "ASR backend state did not match configured provider: target=";
        *error += asr_state.target_provider_id;
        *error += " effective=";
        *error += asr_state.effective_provider_id;
      }
      return false;
    }
  }

  std::string text_adapter_state_json;
  if (!client->GetTextAdapterState(&text_adapter_state_json, error)) {
    return false;
  }
  if (!Contains(text_adapter_state_json, "\"adapter_count\":")) {
    if (error != nullptr) {
      *error = "text adapter state missing adapter_count in ";
      *error += text_adapter_state_json;
    }
    return false;
  }

  const auto expected_text_adapter =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_TEXT_ADAPTER");
  if (!expected_text_adapter.empty()) {
    const std::string expected_single_adapter_marker =
        "\"single_adapter_id\":\"" + expected_text_adapter + "\"";
    const std::string expected_adapter_id_marker =
        "\"id\":\"" + expected_text_adapter + "\"";
    if (!Contains(text_adapter_state_json, expected_single_adapter_marker) ||
        !Contains(text_adapter_state_json, expected_adapter_id_marker)) {
      if (error != nullptr) {
        *error = "text adapter state missing expected adapter: ";
        *error += expected_text_adapter;
        *error += " in ";
        *error += text_adapter_state_json;
      }
      return false;
    }
  }

  return true;
}

bool ExpectSceneLifecycle(SdBusDaemonClient *client, std::string *error) {
  SceneStateSnapshot state;
  if (!client->GetSceneState(&state, error)) {
    return false;
  }
  if (state.active_scene_id != kDefaultNormalSceneId || state.scenes.size() < 2) {
    if (error != nullptr) {
      *error = "scene state did not expose bundled active scene and menu items";
    }
    return false;
  }

  bool persisted = true;
  if (!client->SetActiveScene("__command__", &persisted, error)) {
    return false;
  }
  const bool expect_persisted =
      !OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_SCENE_PERSISTED").empty();
  if (persisted != expect_persisted) {
    if (error != nullptr) {
      *error = "scene persistence result did not match expectation";
    }
    return false;
  }
  if (!client->GetSceneState(&state, error) || state.active_scene_id != "__command__") {
    if (error != nullptr && error->empty()) {
      *error = "scene state did not reflect selected command scene";
    }
    return false;
  }
  return client->SetActiveScene(kDefaultNormalSceneId, &persisted, error);
}

bool ExpectAsrMenuLifecycle(SdBusDaemonClient *client, std::string *error) {
  AsrMenuStateSnapshot state;
  if (!client->GetAsrMenuState(&state, error)) {
    return false;
  }
  if (state.target_provider_id.empty() || state.effective_provider_id.empty() ||
      state.providers.empty()) {
    if (error != nullptr) {
      *error = "ASR menu state did not expose target, effective, and providers";
    }
    return false;
  }

  const auto switch_provider =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_SWITCH_ASR_PROVIDER");
  if (switch_provider.empty()) {
    return true;
  }
  bool found = false;
  for (const auto &provider : state.providers) {
    found = found || provider.id == switch_provider;
  }
  if (!found) {
    if (error != nullptr) {
      *error = "ASR menu state missing requested switch provider: " + switch_provider;
    }
    return false;
  }

  bool persisted = false;
  if (!client->SetActiveAsrProvider(switch_provider, &persisted, error)) {
    return false;
  }
  const bool expect_persisted =
      !OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_PERSISTED").empty();
  if (persisted != expect_persisted) {
    if (error != nullptr) {
      *error = "ASR provider persistence result did not match expectation";
    }
    return false;
  }

  for (int attempt = 0; attempt < 200; ++attempt) {
    if (!client->GetAsrMenuState(&state, error)) {
      return false;
    }
    if (!state.reload_in_progress && state.effective_provider_id == switch_provider) {
      return state.last_error.empty();
    }
    if (!state.reload_in_progress && !state.last_error.empty()) {
      if (error != nullptr) {
        *error = "ASR provider reload failed: " + state.last_error;
      }
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  if (error != nullptr) {
    *error = "timed out waiting for ASR provider switch to " + switch_provider;
  }
  return false;
}

bool ExpectAsrTargetMenuLifecycle(SdBusDaemonClient *client, std::string *error) {
  AsrTargetMenuStateSnapshot state;
  if (!client->GetAsrTargetMenuState(&state, error)) {
    return false;
  }
  if (state.target_provider_id.empty() || state.effective_provider_id.empty() ||
      state.targets.empty()) {
    if (error != nullptr) {
      *error = "ASR target menu state did not expose target, effective, and rows";
    }
    return false;
  }

  const auto switch_provider =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_SWITCH_ASR_TARGET_PROVIDER");
  const auto switch_model =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_SWITCH_ASR_TARGET_MODEL");
  if (switch_provider.empty() && switch_model.empty()) {
    return true;
  }
  if (switch_provider.empty() || switch_model.empty()) {
    if (error != nullptr) {
      *error = "ASR target switch requires both provider and model values";
    }
    return false;
  }
  bool found = false;
  for (const auto &target : state.targets) {
    found = found || (target.provider_id == switch_provider &&
                      target.model_value == switch_model);
  }
  if (!found) {
    if (error != nullptr) {
      *error = "ASR target menu state missing requested target: " + switch_provider +
               "/" + switch_model;
    }
    return false;
  }

  bool persisted = false;
  if (!client->SetActiveAsrTarget(switch_provider, switch_model, &persisted, error)) {
    return false;
  }
  const bool expect_persisted =
      !OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_TARGET_PERSISTED").empty();
  if (persisted != expect_persisted) {
    if (error != nullptr) {
      *error = "ASR target persistence result did not match expectation";
    }
    return false;
  }

  for (int attempt = 0; attempt < 200; ++attempt) {
    if (!client->GetAsrTargetMenuState(&state, error)) {
      return false;
    }
    if (!state.reload_in_progress && state.target_provider_id == switch_provider &&
        state.target_model_id == switch_model &&
        state.effective_provider_id == switch_provider) {
      return state.last_error.empty();
    }
    if (!state.reload_in_progress && !state.last_error.empty()) {
      if (error != nullptr) {
        *error = "ASR target reload failed: " + state.last_error;
      }
      return false;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  if (error != nullptr) {
    *error = "timed out waiting for ASR target switch to " + switch_provider + "/" +
             switch_model;
  }
  return false;
}

bool ExpectAsrDisplayMenuState(SdBusDaemonClient *client, std::string *error) {
  AsrDisplayMenuStateSnapshot state;
  if (!client->GetAsrDisplayMenuState(&state, error)) {
    return false;
  }
  if (state.target_provider_id.empty() || state.effective_provider_id.empty() ||
      state.targets.empty()) {
    if (error != nullptr) {
      *error = "ASR display menu state did not expose target, effective, and rows";
    }
    return false;
  }

  const auto expected_provider =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_PROVIDER");
  const auto expected_model =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_MODEL");
  const auto expected_id =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_ID");
  const auto expected_title =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_TITLE");
  if (expected_provider.empty() && expected_model.empty() && expected_id.empty() &&
      expected_title.empty()) {
    return true;
  }
  if (expected_provider.empty() || expected_model.empty() || expected_id.empty() ||
      expected_title.empty()) {
    if (error != nullptr) {
      *error = "ASR display expectation requires provider, model, id, and title";
    }
    return false;
  }

  for (const auto &target : state.targets) {
    if (target.provider_id == expected_provider &&
        target.model_value == expected_model && target.item_id == expected_id &&
        target.display_title == expected_title) {
      return true;
    }
  }
  if (error != nullptr) {
    *error = "ASR display menu state missing expected localized row";
  }
  return false;
}

bool ExpectAdapterLifecycle(SdBusDaemonClient *client, std::string_view adapter_id,
                            std::string *error) {
  std::string state_json;
  if (!client->GetTextAdapterState(&state_json, error)) {
    return false;
  }
  const std::string adapter_marker = "\"id\":\"" + std::string(adapter_id) + "\"";
  if (!Contains(state_json, adapter_marker)) {
    if (error != nullptr) {
      *error = "text adapter lifecycle state missing adapter: ";
      *error += adapter_id;
      *error += " in ";
      *error += state_json;
    }
    return false;
  }

  if (!client->StartAdapter(adapter_id, error)) {
    return false;
  }
  if (!client->GetTextAdapterState(&state_json, error)) {
    return false;
  }
  if (!Contains(state_json, adapter_marker) ||
      !Contains(state_json, "\"is_running\":true") ||
      !Contains(state_json, "\"pid\":")) {
    if (error != nullptr) {
      *error = "text adapter lifecycle start did not report running adapter in ";
      *error += state_json;
    }
    std::string stop_error;
    client->StopAdapter(adapter_id, &stop_error);
    return false;
  }

  std::string duplicate_error;
  if (client->StartAdapter(adapter_id, &duplicate_error)) {
    if (error != nullptr) {
      *error = "duplicate text adapter lifecycle start unexpectedly succeeded";
    }
    client->StopAdapter(adapter_id, &duplicate_error);
    return false;
  }
  if (!Contains(duplicate_error, "already running")) {
    if (error != nullptr) {
      *error = "duplicate text adapter lifecycle start produced unexpected error: ";
      *error += duplicate_error;
    }
    client->StopAdapter(adapter_id, &duplicate_error);
    return false;
  }

  if (!client->StopAdapter(adapter_id, error)) {
    return false;
  }
  if (!client->GetTextAdapterState(&state_json, error)) {
    return false;
  }
  if (!Contains(state_json, adapter_marker) ||
      !Contains(state_json, "\"is_running\":false") ||
      !Contains(state_json, "\"pid\":null")) {
    if (error != nullptr) {
      *error = "text adapter lifecycle stop did not report stopped adapter in ";
      *error += state_json;
    }
    return false;
  }

  return true;
}

std::chrono::milliseconds RecordDelay() {
  const char *value = std::getenv("VINPUT_DBUS_SMOKE_RECORD_MS");
  if (value == nullptr) {
    return std::chrono::milliseconds(0);
  }
  char *end = nullptr;
  const long delay_ms = std::strtol(value, &end, 10);
  if (end == value || delay_ms <= 0) {
    return std::chrono::milliseconds(0);
  }
  return std::chrono::milliseconds(delay_ms);
}

void WaitForRecording(std::chrono::milliseconds delay) {
  if (delay.count() > 0) {
    std::this_thread::sleep_for(delay);
  }
}

} // namespace

int main() {
  std::string error;
  auto client = ConnectWithRetry(&error);
  if (client == nullptr) {
    std::cerr << "connect failed: " << error << '\n';
    return 1;
  }

  const auto record_delay = RecordDelay();

  if (!ExpectRuntimeStatus(client.get(), "\"status\":\"idle\"", &error)) {
    std::cerr << "runtime status idle check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectConfiguredDiagnostics(client.get(), &error)) {
    std::cerr << "configured diagnostics check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectSceneLifecycle(client.get(), &error)) {
    std::cerr << "scene lifecycle check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectAsrMenuLifecycle(client.get(), &error)) {
    std::cerr << "ASR menu lifecycle check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectAsrTargetMenuLifecycle(client.get(), &error)) {
    std::cerr << "ASR target menu lifecycle check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectAsrDisplayMenuState(client.get(), &error)) {
    std::cerr << "ASR display menu state check failed: " << error << '\n';
    return 1;
  }
  const auto lifecycle_adapter =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_LIFECYCLE_ADAPTER");
  if (!lifecycle_adapter.empty()) {
    if (!ExpectAdapterLifecycle(client.get(), lifecycle_adapter, &error)) {
      std::cerr << "adapter lifecycle check failed: " << error << '\n';
      return 1;
    }
    if (!OptionalExpectedText("VINPUT_DBUS_SMOKE_LIFECYCLE_ONLY").empty()) {
      return 0;
    }
  }

  FrontendBridge normal_bridge;
  auto normal_start = normal_bridge.StartNormal(client.get());
  if (normal_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "normal start failed: " << normal_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);

  const auto expected_normal_text =
      ExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_NORMAL", "mock recognition result");
  auto normal_stop = normal_bridge.Stop(client.get(), kDefaultNormalSceneId);
  if (normal_stop.kind != BridgeOutcome::Kind::Commit ||
      normal_stop.text != expected_normal_text) {
    std::cerr << "normal stop did not produce expected commit text: "
              << normal_stop.text << '\n';
    return 1;
  }

  if (normal_bridge.recording() || normal_bridge.command_mode() ||
      normal_stop.command_mode) {
    std::cerr << "normal stop did not reset bridge state\n";
    return 1;
  }

  FrontendBridge command_bridge;
  auto command_start = command_bridge.StartCommand(client.get(), "selected text");
  if (command_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "command start failed: " << command_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);

  std::string command_status_json;
  if (!client->GetRuntimeStatus(&command_status_json, &error)) {
    std::cerr << "runtime status command check failed: " << error << '\n';
    return 1;
  }
  if (!Contains(command_status_json, "\"status\":\"recording\"") ||
      !Contains(command_status_json, "\"selected_text_present\":true") ||
      Contains(command_status_json, "selected text")) {
    std::cerr << "runtime status command snapshot was not sanitized: "
              << command_status_json << '\n';
    return 1;
  }

  const auto expected_command_text = ExpectedText(
      "VINPUT_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");
  auto command_stop = command_bridge.Stop(client.get(), kDefaultCommandSceneId);
  if (command_stop.kind != BridgeOutcome::Kind::Commit ||
      command_stop.text != expected_command_text) {
    std::cerr << "command stop did not produce expected commit text: "
              << command_stop.text << '\n';
    return 1;
  }

  if (command_bridge.recording() || command_bridge.command_mode() ||
      !command_stop.command_mode) {
    std::cerr << "command stop did not reset bridge state\n";
    return 1;
  }

  std::cout << normal_stop.text << '\n' << command_stop.text << '\n';
  return 0;
}
