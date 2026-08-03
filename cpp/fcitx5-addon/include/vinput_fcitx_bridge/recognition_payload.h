#pragma once

#include <cstdint>
#include <optional>
#include <string>
#include <vector>

struct VinputFcitxFrontendOutcome;

namespace vinput_fcitx_bridge {

enum class CandidateSource {
  Raw,
  Llm,
  Asr,
  Cancel,
};

struct Candidate {
  std::string text;
  CandidateSource source = CandidateSource::Raw;
};

struct RecognitionPayload {
  std::string commit_text;
  std::vector<Candidate> candidates;
};

struct FrontendOutcomeSnapshot {
  std::uint8_t kind = 0;
  std::string text;
  RecognitionPayload payload;
  bool command_mode = false;
};

std::optional<FrontendOutcomeSnapshot>
CopyFrontendOutcome(const ::VinputFcitxFrontendOutcome *outcome);

} // namespace vinput_fcitx_bridge
