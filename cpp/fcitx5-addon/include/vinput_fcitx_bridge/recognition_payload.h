#pragma once

#include <string>
#include <vector>

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

} // namespace vinput_fcitx_bridge
