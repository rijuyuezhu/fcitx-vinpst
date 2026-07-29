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

std::string OptionalEnvironment(const char *name) {
  const char *value = std::getenv(name);
  return value == nullptr ? std::string{} : std::string(value);
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
  const auto selected_text = OptionalEnvironment("VINPUT_NATIVE_ADDON_SELECTED_TEXT");
  const bool command_mode = !selected_text.empty();

  FcitxVinputAddon addon(nullptr);
  const auto start = addon.ApplyTriggerAction(
      nullptr,
      command_mode ? FcitxTriggerAction::StartCommand : FcitxTriggerAction::StartNormal,
      selected_text);
  if (start != AppliedOutcome::Preedit || !addon.bridge().recording() ||
      addon.bridge().command_mode() != command_mode ||
      g_last_outcome.kind != BridgeOutcome::Kind::Preedit) {
    std::cerr << "native addon start did not enter recording preedit: applied="
              << static_cast<int>(start) << " recording=" << addon.bridge().recording()
              << " command=" << addon.bridge().command_mode()
              << " outcome=" << static_cast<int>(g_last_outcome.kind)
              << " outcome-command=" << g_last_outcome.command_mode
              << " text=" << g_last_outcome.text << '\n';
    return 1;
  }

  const auto stop =
      addon.ApplyTriggerAction(nullptr, command_mode ? FcitxTriggerAction::StopCommand
                                                     : FcitxTriggerAction::StopNormal);
  const auto expected_outcome =
      command_mode ? AppliedOutcome::CandidateMenu : AppliedOutcome::Commit;
  const auto expected_kind =
      command_mode ? BridgeOutcome::Kind::CandidateMenu : BridgeOutcome::Kind::Commit;
  const auto &expected_commit = command_mode ? selected_text : expected_text;
  if (stop != expected_outcome || addon.bridge().recording() ||
      addon.bridge().command_mode() || g_last_outcome.kind != expected_kind ||
      g_last_outcome.command_mode != command_mode ||
      g_last_outcome.payload.commit_text != expected_commit) {
    std::cerr << "native addon stop did not apply the expected outcome: applied="
              << static_cast<int>(stop) << " recording=" << addon.bridge().recording()
              << " command=" << addon.bridge().command_mode()
              << " outcome=" << static_cast<int>(g_last_outcome.kind)
              << " outcome-command=" << g_last_outcome.command_mode
              << " commit=" << g_last_outcome.payload.commit_text
              << " candidates=" << g_last_outcome.payload.candidates.size() << '\n';
    return 1;
  }

  const auto raw_candidate = std::ranges::find_if(
      g_last_outcome.payload.candidates, [&expected_commit](const auto &candidate) {
        return candidate.source == CandidateSource::Raw &&
               candidate.text == expected_commit;
      });
  if (raw_candidate == g_last_outcome.payload.candidates.end()) {
    std::cerr << "native addon outcome did not retain the raw candidate\n";
    return 1;
  }

  if (command_mode) {
    const auto asr_candidate = std::ranges::find_if(
        g_last_outcome.payload.candidates, [&expected_text](const auto &candidate) {
          return candidate.source == CandidateSource::Asr &&
                 candidate.text == expected_text;
        });
    if (asr_candidate == g_last_outcome.payload.candidates.end()) {
      std::cerr << "native command outcome did not retain the ASR candidate\n";
      return 1;
    }
  }

  std::cout << (command_mode ? "native addon command menu: " : "native addon commit: ")
            << expected_commit << '\n';
  return 0;
}
