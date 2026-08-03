#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <algorithm>
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
  using vinput_fcitx_bridge::BridgeOutcome;
  using vinput_fcitx_bridge::CandidateSource;
  using vinput_fcitx_bridge::FrontendBridge;
  using vinput_fcitx_bridge::SdBusDaemonClient;

  const auto expected_text =
      RequiredEnvironment("VINPUT_NATIVE_FRONTEND_EXPECTED_TEXT");

  std::string error;
  auto client = SdBusDaemonClient::ConnectSession(&error);
  if (client == nullptr) {
    std::cerr << "failed to connect frontend client: " << error << '\n';
    return 1;
  }

  vinput_fcitx_bridge::SceneStateSnapshot scene_state;
  if (!client->GetSceneState(&scene_state, &error)) {
    std::cerr << "failed to read frontend scene state: " << error << '\n';
    return 1;
  }

  FrontendBridge bridge;
  const auto start = bridge.StartNormal(client->raw_handle(), scene_state);
  if (start.kind != BridgeOutcome::Kind::Preedit || start.text != "... Recording ..." ||
      !bridge.recording()) {
    std::cerr << "native frontend start failed: kind=" << static_cast<int>(start.kind)
              << " text=" << start.text << '\n';
    return 1;
  }
  const auto stop = bridge.Stop(client->raw_handle(), scene_state);
  if (stop.kind != BridgeOutcome::Kind::Commit || bridge.recording()) {
    std::cerr << "native frontend stop did not commit: kind="
              << static_cast<int>(stop.kind) << " text=" << stop.text << '\n';
    return 1;
  }
  if (stop.text != expected_text || stop.payload.commit_text != expected_text) {
    std::cerr << "native frontend commit mismatch: " << stop.text << '\n';
    return 1;
  }

  const auto raw_candidate = std::ranges::find_if(
      stop.payload.candidates, [&expected_text](const auto &candidate) {
        return candidate.source == CandidateSource::Raw &&
               candidate.text == expected_text;
      });
  if (raw_candidate == stop.payload.candidates.end()) {
    std::cerr << "native frontend payload did not retain the raw candidate\n";
    return 1;
  }
  std::cout << "native frontend commit: " << stop.text << '\n';
  return 0;
}
