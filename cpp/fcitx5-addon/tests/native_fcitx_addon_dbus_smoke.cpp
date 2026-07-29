#include "vinput_fcitx_bridge/fcitx_addon.h"

#include <algorithm>
#include <cstdlib>
#include <iostream>
#include <string>

namespace {

vinput_fcitx_bridge::BridgeOutcome g_last_outcome;

std::string RequiredEnvironment(const char *name) {
  const char *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    std::cerr << name << " is required\n";
    std::exit(2);
  }
  return value;
}

} // namespace

namespace vinput_fcitx_bridge {

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *) {
  g_last_outcome = outcome;
  switch (outcome.kind) {
  case BridgeOutcome::Kind::None:
    return AppliedOutcome::None;
  case BridgeOutcome::Kind::Preedit:
  case BridgeOutcome::Kind::Error:
    return AppliedOutcome::Preedit;
  case BridgeOutcome::Kind::Clear:
    return AppliedOutcome::Clear;
  case BridgeOutcome::Kind::Commit:
    return AppliedOutcome::Commit;
  case BridgeOutcome::Kind::CandidateMenu:
    return AppliedOutcome::CandidateMenu;
  }
  return AppliedOutcome::None;
}

} // namespace vinput_fcitx_bridge

int main() {
  using vinput_fcitx_bridge::AppliedOutcome;
  using vinput_fcitx_bridge::BridgeOutcome;
  using vinput_fcitx_bridge::CandidateSource;
  using vinput_fcitx_bridge::FcitxTriggerAction;
  using vinput_fcitx_bridge::FcitxVinputAddon;

  const auto expected_text =
      RequiredEnvironment("VINPUT_NATIVE_FRONTEND_EXPECTED_TEXT");

  FcitxVinputAddon addon(nullptr);
  const auto start = addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StartNormal);
  if (start != AppliedOutcome::Preedit || !addon.bridge().recording() ||
      g_last_outcome.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "native addon start did not enter recording preedit\n";
    return 1;
  }

  const auto stop = addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StopNormal);
  if (stop != AppliedOutcome::Commit || addon.bridge().recording() ||
      g_last_outcome.kind != BridgeOutcome::Kind::Commit ||
      g_last_outcome.text != expected_text ||
      g_last_outcome.payload.commit_text != expected_text) {
    std::cerr << "native addon stop did not apply the expected commit\n";
    return 1;
  }

  const auto raw_candidate = std::ranges::find_if(
      g_last_outcome.payload.candidates, [&expected_text](const auto &candidate) {
        return candidate.source == CandidateSource::Raw &&
               candidate.text == expected_text;
      });
  if (raw_candidate == g_last_outcome.payload.candidates.end()) {
    std::cerr << "native addon outcome did not retain the raw candidate\n";
    return 1;
  }

  std::cout << "native addon commit: " << g_last_outcome.text << '\n';
  return 0;
}
