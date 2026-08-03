#include "vinput_fcitx_bridge/recognition_payload.h"

#include "vinput_fcitx_ffi.h"

namespace vinput_fcitx_bridge {
namespace {

std::string CopyText(VinputFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

CandidateSource CandidateSourceFromValue(std::uint8_t source) {
  switch (source) {
  case VINPUT_FCITX_CANDIDATE_SOURCE_LLM:
    return CandidateSource::Llm;
  case VINPUT_FCITX_CANDIDATE_SOURCE_ASR:
    return CandidateSource::Asr;
  case VINPUT_FCITX_CANDIDATE_SOURCE_CANCEL:
    return CandidateSource::Cancel;
  default:
    return CandidateSource::Raw;
  }
}

} // namespace

std::optional<FrontendOutcomeSnapshot>
CopyFrontendOutcome(const VinputFcitxFrontendOutcome *outcome) {
  VinputFcitxFrontendOutcomeView view{};
  if (outcome == nullptr || vinput_fcitx_frontend_outcome_view(outcome, &view) == 0) {
    return std::nullopt;
  }

  FrontendOutcomeSnapshot result;
  result.kind = view.kind;
  result.text = CopyText(view.text);
  result.payload.commit_text = CopyText(view.commit_text);
  result.command_mode = view.command_mode != 0;
  result.payload.candidates.reserve(view.candidate_count);
  for (std::size_t index = 0; index < view.candidate_count; ++index) {
    VinputFcitxCandidateView candidate{};
    if (vinput_fcitx_frontend_outcome_candidate(outcome, index, &candidate) == 0) {
      return std::nullopt;
    }
    result.payload.candidates.push_back(Candidate{
        CopyText(candidate.text), CandidateSourceFromValue(candidate.source)});
  }
  return result;
}

} // namespace vinput_fcitx_bridge
