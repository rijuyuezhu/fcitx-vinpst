#include "vinput_fcitx_bridge/fcitx_menu_projection.h"
#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/rust_handle.h"
#include "vinput_fcitx_bridge/rust_string.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"
#include "vinput_fcitx_ffi.h"

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <thread>
#include <vector>

using vinput_fcitx_bridge::AsrMenuController;
using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FrontendBridge;
using vinput_fcitx_bridge::ProjectedMenuControlKind;
using vinput_fcitx_bridge::SceneMenuController;
using vinput_fcitx_bridge::SdBusDaemonClient;

namespace {

using MenuProjectionHandle =
    vinput_fcitx_bridge::RustOwnedHandle<VinputFcitxMenuProjection,
                                         vinput_fcitx_menu_projection_free>;
using MenuSessionHandle =
    vinput_fcitx_bridge::RustOwnedHandle<VinputFcitxMenuSession,
                                         vinput_fcitx_menu_session_free>;

struct ProjectedItem {
  std::string label;
  ProjectedMenuControlKind control_kind = ProjectedMenuControlKind::None;
  std::string first;
  std::string second;
  std::string control_label;
};

struct SceneProjectionState {
  std::string active_label;
  std::vector<ProjectedItem> items;
};

struct AsrProjectionState {
  std::string effective_label;
  std::vector<ProjectedItem> items;
};

ProjectedItem CopyProjectedItem(const VinputFcitxProjectedMenuItemView &view) {
  return ProjectedItem{vinput_fcitx_bridge::CopyRustString(view.label),
                       static_cast<ProjectedMenuControlKind>(view.control_kind),
                       vinput_fcitx_bridge::CopyRustString(view.control_first),
                       vinput_fcitx_bridge::CopyRustString(view.control_second),
                       vinput_fcitx_bridge::CopyRustString(view.control_label)};
}

std::optional<SceneProjectionState>
ProjectScene(const SceneMenuController &controller) {
  auto session = MenuSessionHandle::Adopt(vinput_fcitx_menu_session_new());
  if (!session) {
    return std::nullopt;
  }
  auto projection =
      MenuProjectionHandle::Adopt(vinput_fcitx_scene_menu_controller_projection_new(
          controller.raw_handle(), session.raw_handle()));
  if (!projection) {
    return std::nullopt;
  }
  VinputFcitxMenuProjectionView view{};
  if (vinput_fcitx_menu_projection_view(projection.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  SceneProjectionState state{
      .active_label = vinput_fcitx_bridge::CopyRustString(view.summary), .items = {}};
  state.items.reserve(view.item_count);
  for (std::size_t index = 0; index < view.item_count; ++index) {
    VinputFcitxProjectedMenuItemView item{};
    if (vinput_fcitx_menu_projection_item_view(projection.raw_handle(), index, &item) ==
        0) {
      return std::nullopt;
    }
    state.items.push_back(CopyProjectedItem(item));
  }
  return state;
}

bool HasSceneControl(const SceneProjectionState &projection,
                     std::string_view scene_id) {
  return std::ranges::any_of(projection.items, [scene_id](const auto &item) {
    return item.control_kind == ProjectedMenuControlKind::SetActiveScene &&
           item.first == scene_id;
  });
}

std::optional<AsrProjectionState> ProjectAsr(const AsrMenuController &controller) {
  constexpr std::string_view kLocal = "Local";
  constexpr std::string_view kRemote = "Remote";
  constexpr std::string_view kCommand = "Command";
  constexpr std::string_view kLoadingSuffix = " (loading)";
  constexpr std::string_view kUnavailable = "unavailable";
  constexpr std::string_view kLoadingPrefix = "Loading: ";
  constexpr std::string_view kErrorPrefix = "Error: ";
  auto session = MenuSessionHandle::Adopt(vinput_fcitx_menu_session_new());
  if (!session) {
    return std::nullopt;
  }
  auto projection =
      MenuProjectionHandle::Adopt(vinput_fcitx_asr_menu_controller_projection_new(
          controller.raw_handle(), session.raw_handle(),
          vinput_fcitx_bridge::RustBytes(kLocal), kLocal.size(),
          vinput_fcitx_bridge::RustBytes(kRemote), kRemote.size(),
          vinput_fcitx_bridge::RustBytes(kCommand), kCommand.size(),
          vinput_fcitx_bridge::RustBytes(kLoadingSuffix), kLoadingSuffix.size(),
          vinput_fcitx_bridge::RustBytes(kUnavailable), kUnavailable.size(),
          vinput_fcitx_bridge::RustBytes(kLoadingPrefix), kLoadingPrefix.size(),
          vinput_fcitx_bridge::RustBytes(kErrorPrefix), kErrorPrefix.size()));
  if (!projection) {
    return std::nullopt;
  }
  VinputFcitxMenuProjectionView view{};
  if (vinput_fcitx_menu_projection_view(projection.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  AsrProjectionState state{.effective_label =
                               vinput_fcitx_bridge::CopyRustString(view.summary),
                           .items = {}};
  state.items.reserve(view.item_count);
  for (std::size_t index = 0; index < view.item_count; ++index) {
    VinputFcitxProjectedMenuItemView item{};
    if (vinput_fcitx_menu_projection_item_view(projection.raw_handle(), index, &item) ==
        0) {
      return std::nullopt;
    }
    state.items.push_back(CopyProjectedItem(item));
  }
  return state;
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
  AsrMenuController controller;
  if (!client->RefreshAsrMenuController(&controller, error)) {
    return false;
  }
  const auto projection = ProjectAsr(controller);
  if (projection.has_value() && !projection->effective_label.empty()) {
    return true;
  }
  if (error != nullptr) {
    *error = "ASR display state did not produce an effective backend label";
  }
  return false;
}

bool ExpectSceneLifecycle(SdBusDaemonClient *client, std::string *error) {
  SceneMenuController controller;
  if (!client->RefreshSceneMenuController(&controller, error)) {
    return false;
  }
  auto expected_active_scene =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_ACTIVE_SCENE");
  if (expected_active_scene.empty()) {
    expected_active_scene = "__raw__";
  }
  const auto projection = ProjectScene(controller);
  if (!projection.has_value() || projection->active_label.empty() ||
      projection->items.empty() ||
      HasSceneControl(*projection, expected_active_scene) ||
      !HasSceneControl(*projection, "__command__")) {
    if (error != nullptr) {
      *error = "scene state did not expose expected active scene and menu rows: ";
      *error += expected_active_scene;
    }
    return false;
  }

  bool persisted = true;
  if (!client->SetActiveScene(&controller, "__command__", &persisted, error)) {
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
  if (!client->RefreshSceneMenuController(&controller, error)) {
    return false;
  }
  const auto command_projection = ProjectScene(controller);
  if (!command_projection.has_value() || command_projection->active_label.empty() ||
      HasSceneControl(*command_projection, "__command__") ||
      !HasSceneControl(*command_projection, expected_active_scene)) {
    if (error != nullptr && error->empty()) {
      *error = "scene state did not reflect selected command scene";
    }
    return false;
  }
  return client->SetActiveScene(&controller, expected_active_scene, &persisted, error);
}

bool ExpectAsrTargetMenuLifecycle(SdBusDaemonClient *client, std::string *error) {
  AsrMenuController controller;
  if (!client->RefreshAsrMenuController(&controller, error)) {
    return false;
  }
  auto projection = ProjectAsr(controller);
  if (!projection.has_value() || projection->effective_label.empty()) {
    if (error != nullptr) {
      *error = "ASR display state did not produce a projection";
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

  const auto target = std::find_if(
      projection->items.begin(), projection->items.end(),
      [&](const ProjectedItem &item) {
        return item.control_kind == ProjectedMenuControlKind::SetActiveAsrTarget &&
               item.first == switch_provider && item.second == switch_model;
      });
  if (target == projection->items.end()) {
    if (error != nullptr) {
      *error = "ASR projection missing requested target: " + switch_provider + "/" +
               switch_model;
    }
    return false;
  }
  const auto expected_provider =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_PROVIDER");
  const auto expected_model =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_MODEL");
  const auto expected_title =
      OptionalExpectedText("VINPUT_DBUS_SMOKE_EXPECT_ASR_DISPLAY_TITLE");
  if ((!expected_provider.empty() && expected_provider != switch_provider) ||
      (!expected_model.empty() && expected_model != switch_model) ||
      (!expected_title.empty() && expected_title != target->control_label)) {
    if (error != nullptr) {
      *error = "ASR projected target metadata did not match the expectation";
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
    if (!client->RefreshAsrMenuController(&controller, error)) {
      return false;
    }
    projection = ProjectAsr(controller);
    if (!projection.has_value()) {
      return false;
    }
    if (projection->effective_label.find("Error: ") != std::string::npos) {
      if (error != nullptr) {
        *error = "ASR target reload failed: " + projection->effective_label;
      }
      return false;
    }
    const bool target_still_visible = std::any_of(
        projection->items.begin(), projection->items.end(),
        [&](const ProjectedItem &item) {
          return item.control_kind == ProjectedMenuControlKind::SetActiveAsrTarget &&
                 item.first == switch_provider && item.second == switch_model;
        });
    if (!target_still_visible &&
        projection->effective_label.find("Loading: ") == std::string::npos) {
      return true;
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
  AsrMenuController controller;
  if (!client->RefreshAsrMenuController(&controller, error)) {
    return false;
  }
  const auto projection = ProjectAsr(controller);
  if (projection.has_value() && !projection->effective_label.empty()) {
    return true;
  }
  if (error != nullptr) {
    *error = "ASR display state did not produce a projection";
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
  SceneMenuController frontend_scene_controller;
  if (!client->RefreshSceneMenuController(&frontend_scene_controller, &error)) {
    std::cerr << "frontend scene state check failed: " << error << '\n';
    return 1;
  }
  FrontendBridge normal_bridge;
  auto normal_start =
      normal_bridge.StartNormal(client->raw_handle(), frontend_scene_controller);
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
  auto normal_stop =
      normal_bridge.Stop(client->raw_handle(), frontend_scene_controller);
  if (normal_stop.kind != BridgeOutcome::Kind::Commit ||
      normal_stop.text != expected_normal_text) {
    std::cerr << "normal stop did not produce expected commit text: "
              << normal_stop.text << '\n';
    return 1;
  }

  if (normal_bridge.recording() || normal_bridge.command_mode() ||
      normal_stop.replace_selection) {
    std::cerr << "normal stop did not reset bridge state\n";
    return 1;
  }
  if (!ExpectDaemonStatus(client.get(), "idle", &error)) {
    std::cerr << "daemon status after normal stop failed: " << error << '\n';
    return 1;
  }

  FrontendBridge command_bridge;
  auto command_start =
      command_bridge.StartCommand(client->raw_handle(), "selected text", {});
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
  auto command_stop =
      command_bridge.Stop(client->raw_handle(), frontend_scene_controller);
  if (command_stop.kind != BridgeOutcome::Kind::Commit ||
      command_stop.text != expected_command_text) {
    std::cerr << "command stop did not produce expected commit text: "
              << command_stop.text << '\n';
    return 1;
  }

  if (command_bridge.recording() || command_bridge.command_mode() ||
      !command_stop.replace_selection) {
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
