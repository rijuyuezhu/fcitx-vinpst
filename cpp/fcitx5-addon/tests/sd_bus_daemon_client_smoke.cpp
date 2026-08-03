#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/scene_defaults.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"
#include "vinput_fcitx_ffi.h"

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <string_view>
#include <thread>

using vinput_fcitx_bridge::AsrDisplayMenuStateSnapshot;
using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FrontendBridge;
using vinput_fcitx_bridge::kDefaultCommandSceneId;
using vinput_fcitx_bridge::kDefaultNormalSceneId;
using vinput_fcitx_bridge::SceneStateSnapshot;
using vinput_fcitx_bridge::SdBusDaemonClient;

namespace {

std::string CopyText(VinputFcitxStringView view) {
  return view.data == nullptr || view.len == 0
             ? std::string{}
             : std::string(reinterpret_cast<const char *>(view.data), view.len);
}

struct SceneSnapshotState {
  std::string active_scene_id;
  std::size_t item_count = 0;
};

struct SceneSnapshotItem {
  std::string id;
  std::string label;
};

struct AsrSnapshotState {
  std::string target_provider_id;
  std::string target_model_id;
  std::string effective_provider_id;
  std::string effective_model_id;
  std::string last_error;
  std::string effective_base_label;
  std::string target_base_label;
  bool reload_in_progress = false;
  std::size_t item_count = 0;
};

struct AsrSnapshotItem {
  std::string provider_id;
  std::string kind;
  std::string item_id;
  std::string display_title;
  std::string model_value;
  std::string base_label;
  bool loading = false;
};

std::optional<SceneSnapshotState> ReadState(const SceneStateSnapshot &snapshot) {
  VinputFcitxSceneSnapshotView view{};
  if (vinput_fcitx_scene_snapshot_view(snapshot.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  return SceneSnapshotState{CopyText(view.active_scene_id), view.item_count};
}

std::optional<SceneSnapshotItem> ReadItem(const SceneStateSnapshot &snapshot,
                                          std::size_t index) {
  VinputFcitxSceneSnapshotItemView view{};
  if (vinput_fcitx_scene_snapshot_item_view(snapshot.raw_handle(), index, &view) == 0) {
    return std::nullopt;
  }
  return SceneSnapshotItem{CopyText(view.id), CopyText(view.label)};
}

std::optional<AsrSnapshotState> ReadState(const AsrDisplayMenuStateSnapshot &snapshot) {
  VinputFcitxAsrDisplaySnapshotView view{};
  if (vinput_fcitx_asr_display_snapshot_view(snapshot.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  return AsrSnapshotState{CopyText(view.target_provider_id),
                          CopyText(view.target_model_id),
                          CopyText(view.effective_provider_id),
                          CopyText(view.effective_model_id),
                          CopyText(view.last_error),
                          CopyText(view.effective_base_label),
                          CopyText(view.target_base_label),
                          view.reload_in_progress != 0,
                          view.item_count};
}

std::optional<AsrSnapshotItem> ReadItem(const AsrDisplayMenuStateSnapshot &snapshot,
                                        std::size_t index) {
  VinputFcitxAsrDisplaySnapshotItemView view{};
  if (vinput_fcitx_asr_display_snapshot_item_view(snapshot.raw_handle(), index,
                                                  &view) == 0) {
    return std::nullopt;
  }
  return AsrSnapshotItem{CopyText(view.provider_id), CopyText(view.kind),
                         CopyText(view.item_id),     CopyText(view.display_title),
                         CopyText(view.model_value), CopyText(view.base_label),
                         view.is_loading != 0};
}

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

bool ExpectDaemonStatus(SdBusDaemonClient *client, std::string_view expected,
                        std::string *error) {
  std::string status;
  if (!client->GetStatus(&status, error)) {
    return false;
  }
  if (status == expected) {
    return true;
  }
  if (error != nullptr) {
    *error = "daemon status mismatch: expected ";
    *error += expected;
    *error += ", got ";
    *error += status;
  }
  return false;
}

bool ExpectConfiguredAsrState(SdBusDaemonClient *client, std::string *error) {
  AsrDisplayMenuStateSnapshot snapshot;
  if (!client->GetAsrDisplayMenuState(&snapshot, error)) {
    return false;
  }
  const auto state = ReadState(snapshot);
  if (state.has_value() && !state->target_provider_id.empty() &&
      state->item_count != 0) {
    return true;
  }
  if (error != nullptr) {
    *error = "ASR display state did not include a target provider and menu rows";
  }
  return false;
}

bool ExpectSceneLifecycle(SdBusDaemonClient *client, std::string *error) {
  SceneStateSnapshot state;
  if (!client->GetSceneState(&state, error)) {
    return false;
  }
  auto expected_active_scene =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE");
  if (expected_active_scene.empty()) {
    expected_active_scene = kDefaultNormalSceneId;
  }
  const auto scene_state = ReadState(state);
  if (!scene_state.has_value()) {
    return false;
  }
  bool exposes_expected_scene = false;
  for (std::size_t index = 0; index < scene_state->item_count; ++index) {
    const auto scene = ReadItem(state, index);
    if (scene.has_value() && scene->id == expected_active_scene) {
      exposes_expected_scene = true;
      break;
    }
  }
  if (scene_state->active_scene_id != expected_active_scene ||
      scene_state->item_count < 2 || !exposes_expected_scene) {
    if (error != nullptr) {
      *error = "scene state did not expose expected active scene and menu items: ";
      *error += expected_active_scene;
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
  if (!client->GetSceneState(&state, error)) {
    return false;
  }
  const auto command_scene_state = ReadState(state);
  if (!command_scene_state.has_value() ||
      command_scene_state->active_scene_id != "__command__") {
    if (error != nullptr && error->empty()) {
      *error = "scene state did not reflect selected command scene";
    }
    return false;
  }
  return client->SetActiveScene(expected_active_scene, &persisted, error);
}

bool ExpectAsrTargetMenuLifecycle(SdBusDaemonClient *client, std::string *error) {
  AsrDisplayMenuStateSnapshot snapshot;
  if (!client->GetAsrDisplayMenuState(&snapshot, error)) {
    return false;
  }
  auto state = ReadState(snapshot);
  if (!state.has_value() || state->target_provider_id.empty() ||
      state->effective_provider_id.empty() || state->item_count == 0) {
    if (error != nullptr) {
      *error = "ASR display state did not expose target, effective, and rows";
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
  for (std::size_t index = 0; index < state->item_count; ++index) {
    const auto item = ReadItem(snapshot, index);
    found = found || (item.has_value() && item->provider_id == switch_provider &&
                      item->model_value == switch_model);
  }
  if (!found) {
    if (error != nullptr) {
      *error = "ASR display state missing requested target: " + switch_provider + "/" +
               switch_model;
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
    if (!client->GetAsrDisplayMenuState(&snapshot, error)) {
      return false;
    }
    state = ReadState(snapshot);
    if (!state.has_value()) {
      return false;
    }
    if (!state->reload_in_progress && state->target_provider_id == switch_provider &&
        state->target_model_id == switch_model &&
        state->effective_provider_id == switch_provider) {
      return state->last_error.empty();
    }
    if (!state->reload_in_progress && !state->last_error.empty()) {
      if (error != nullptr) {
        *error = "ASR target reload failed: " + state->last_error;
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
  const auto display_state = ReadState(state);
  if (!display_state.has_value() || display_state->target_provider_id.empty() ||
      display_state->effective_provider_id.empty() || display_state->item_count == 0) {
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

  for (std::size_t index = 0; index < display_state->item_count; ++index) {
    const auto target = ReadItem(state, index);
    if (target.has_value() && target->provider_id == expected_provider &&
        target->model_value == expected_model && target->item_id == expected_id &&
        target->display_title == expected_title) {
      return true;
    }
  }
  if (error != nullptr) {
    *error = "ASR display menu state missing expected localized row";
  }
  return false;
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

  if (!ExpectDaemonStatus(client.get(), "idle", &error)) {
    std::cerr << "daemon status idle check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectConfiguredAsrState(client.get(), &error)) {
    std::cerr << "configured ASR state check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectSceneLifecycle(client.get(), &error)) {
    std::cerr << "scene lifecycle check failed: " << error << '\n';
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
  FrontendBridge normal_bridge;
  auto normal_start =
      normal_bridge.StartNormal(client->raw_handle(), kDefaultNormalSceneId);
  if (normal_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "normal start failed: " << normal_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);
  if (!ExpectDaemonStatus(client.get(), "recording", &error)) {
    std::cerr << "daemon status normal recording check failed: " << error << '\n';
    return 1;
  }

  const auto expected_normal_text =
      ExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_NORMAL", "mock recognition result");
  auto normal_stop = normal_bridge.Stop(client->raw_handle(), kDefaultNormalSceneId);
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
  if (!ExpectDaemonStatus(client.get(), "idle", &error)) {
    std::cerr << "daemon status after normal stop failed: " << error << '\n';
    return 1;
  }

  FrontendBridge command_bridge;
  auto command_start = command_bridge.StartCommand(
      client->raw_handle(), "selected text", kDefaultCommandSceneId);
  if (command_start.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "command start failed: " << command_start.text << '\n';
    return 1;
  }

  WaitForRecording(record_delay);

  if (!ExpectDaemonStatus(client.get(), "recording", &error)) {
    std::cerr << "daemon status command recording check failed: " << error << '\n';
    return 1;
  }

  const auto expected_command_text = ExpectedText(
      "VINPUT_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");
  auto command_stop = command_bridge.Stop(client->raw_handle(), kDefaultCommandSceneId);
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

  if (!ExpectDaemonStatus(client.get(), "idle", &error)) {
    std::cerr << "daemon status after command stop failed: " << error << '\n';
    return 1;
  }

  std::cout << normal_stop.text << '\n' << command_stop.text << '\n';
  return 0;
}
