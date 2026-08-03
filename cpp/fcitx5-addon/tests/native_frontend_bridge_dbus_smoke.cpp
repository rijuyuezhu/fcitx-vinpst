#include "vinput_fcitx_bridge/fcitx_outcome.h"
#include "vinput_fcitx_bridge/frontend_bridge.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <string>
#include <string_view>
#include <vector>

namespace {

std::string RequiredEnvironment(const char *name) {
  const char *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    std::cerr << name << " is required\n";
    std::exit(2);
  }
  return value;
}

class RecordingOutcomeSink final : public vinput_fcitx_bridge::OutcomeSink {
public:
  void SetPreedit(std::string_view text) override {
    preedit = std::string(text);
    events.push_back("preedit:" + preedit);
  }

  void ClearPreedit() override {
    preedit.clear();
    events.emplace_back("clear-preedit");
  }

  void ClearCandidateMenu() override {
    events.emplace_back("clear-candidates");
  }

  void DeleteSelectedTextIfAny() override {
    events.emplace_back("delete-selected");
  }

  void CommitString(std::string_view text) override {
    committed_text = std::string(text);
    events.push_back("commit:" + committed_text);
  }

  bool ShowCandidateMenu(const vinput_fcitx_bridge::RecognitionPayload &,
                         bool) override {
    events.emplace_back("show-candidates");
    return false;
  }

  std::string preedit;
  std::string committed_text;
  std::vector<std::string> events;
};

} // namespace

int main() {
  using vinput_fcitx_bridge::AppliedOutcome;
  using vinput_fcitx_bridge::ApplyBridgeOutcomeToSink;
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

  FrontendBridge bridge;
  RecordingOutcomeSink sink;
  const auto start = bridge.StartNormal(client->raw_handle(), "raw");
  if (start.kind != BridgeOutcome::Kind::Preedit || !bridge.recording()) {
    std::cerr << "native frontend start failed: kind=" << static_cast<int>(start.kind)
              << " text=" << start.text << '\n';
    return 1;
  }
  if (ApplyBridgeOutcomeToSink(start, sink) != AppliedOutcome::Preedit ||
      sink.preedit != "... Recording ...") {
    std::cerr << "native frontend start outcome was not applied to preedit\n";
    return 1;
  }

  const auto stop = bridge.Stop(client->raw_handle(), "raw");
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
  if (ApplyBridgeOutcomeToSink(stop, sink) != AppliedOutcome::Commit ||
      sink.committed_text != expected_text) {
    std::cerr << "native frontend commit was not applied to the outcome sink\n";
    return 1;
  }
  const std::vector<std::string> expected_events{
      "preedit:... Recording ...",
      "clear-candidates",
      "clear-preedit",
      "commit:" + expected_text,
  };
  if (sink.events != expected_events) {
    std::cerr << "native frontend sink event ordering mismatch\n";
    return 1;
  }

  std::cout << "native frontend commit: " << stop.text << '\n';
  return 0;
}
