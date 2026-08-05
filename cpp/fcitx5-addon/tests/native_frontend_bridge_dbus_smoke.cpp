#include "vinpst_fcitx_bridge/fcitx_menu_projection.h"
#include "vinpst_fcitx_bridge/frontend_bridge.h"
#include "vinpst_fcitx_bridge/sd_bus_daemon_client.h"

#include <cstdlib>
#include <iostream>
#include <string>

namespace {

std::string RequiredEnvironment(const char *name) {
  const char *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    std::cerr << name << " is required\n";
    std::exit(2);
  }
  return value;
}

} // namespace

int main() {
  using vinpst_fcitx_bridge::BridgeOutcome;
  using vinpst_fcitx_bridge::FrontendBridge;
  using vinpst_fcitx_bridge::SdBusDaemonClient;

  const auto expected_text =
      RequiredEnvironment("VINPST_NATIVE_FRONTEND_EXPECTED_TEXT");

  std::string error;
  auto client = SdBusDaemonClient::ConnectSession(&error);
  if (client == nullptr) {
    std::cerr << "failed to connect frontend client: " << error << '\n';
    return 1;
  }

  vinpst_fcitx_bridge::SceneMenuController scene_controller;
  if (!client->RefreshSceneMenuController(&scene_controller, &error)) {
    std::cerr << "failed to read frontend scene state: " << error << '\n';
    return 1;
  }

  FrontendBridge bridge;
  const auto start = bridge.StartNormal(client->raw_handle(), scene_controller);
  if (start.kind != BridgeOutcome::Kind::Preedit || start.text != "... Recording ..." ||
      !bridge.recording()) {
    std::cerr << "native frontend start failed: kind=" << static_cast<int>(start.kind)
              << " text=" << start.text << '\n';
    return 1;
  }
  const auto stop = bridge.Stop(client->raw_handle(), scene_controller);
  if (stop.kind != BridgeOutcome::Kind::Commit || bridge.recording()) {
    std::cerr << "native frontend stop did not commit: kind="
              << static_cast<int>(stop.kind) << " text=" << stop.text << '\n';
    return 1;
  }
  if (stop.text != expected_text) {
    std::cerr << "native frontend commit mismatch: " << stop.text << '\n';
    return 1;
  }
  if (stop.candidate_menu.candidate_count != 0) {
    std::cerr << "native frontend commit leaked unused candidate rows\n";
    return 1;
  }
  std::cout << "native frontend commit: " << stop.text << '\n';
  return 0;
}
