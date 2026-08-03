#include "vinput_fcitx_bridge/recognition_payload.h"

#include "vinput_fcitx_ffi.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>

namespace vinput_fcitx_bridge {
namespace {

struct CommitPlanDeleter {
  void operator()(VinputFcitxCommitPlan *plan) const {
    vinput_fcitx_commit_plan_free(plan);
  }
};

using RustCommitPlan = std::unique_ptr<VinputFcitxCommitPlan, CommitPlanDeleter>;

RustCommitPlan MakeRustCommitPlan(std::string_view json, bool command_mode) {
  return RustCommitPlan(
      vinput_fcitx_commit_plan_new(reinterpret_cast<const std::uint8_t *>(json.data()),
                                   json.size(), command_mode ? 1U : 0U));
}

std::string CopyRustString(const std::uint8_t *data, std::size_t size) {
  if (data == nullptr || size == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(data), size};
}

CandidateSource CandidateSourceFromRust(std::uint8_t source) {
  switch (source) {
  case VINPUT_FCITX_CANDIDATE_SOURCE_LLM:
    return CandidateSource::Llm;
  case VINPUT_FCITX_CANDIDATE_SOURCE_ASR:
    return CandidateSource::Asr;
  case VINPUT_FCITX_CANDIDATE_SOURCE_CANCEL:
    return CandidateSource::Cancel;
  case VINPUT_FCITX_CANDIDATE_SOURCE_RAW:
  default:
    return CandidateSource::Raw;
  }
}

RecognitionPayload CopyRecognitionPayload(const VinputFcitxCommitPlan *plan) {
  RecognitionPayload payload;
  if (plan == nullptr) {
    return payload;
  }

  payload.commit_text = CopyRustString(vinput_fcitx_commit_plan_text_data(plan),
                                       vinput_fcitx_commit_plan_text_len(plan));

  const auto candidate_count = vinput_fcitx_commit_plan_candidate_count(plan);
  payload.candidates.reserve(candidate_count);
  for (std::size_t index = 0; index < candidate_count; ++index) {
    payload.candidates.push_back(Candidate{
        CopyRustString(vinput_fcitx_commit_plan_candidate_text_data(plan, index),
                       vinput_fcitx_commit_plan_candidate_text_len(plan, index)),
        CandidateSourceFromRust(vinput_fcitx_commit_plan_candidate_source(plan, index)),
    });
  }
  return payload;
}

} // namespace

std::string_view ToWireString(CandidateSource source) {
  switch (source) {
  case CandidateSource::Raw:
    return "raw";
  case CandidateSource::Llm:
    return "llm";
  case CandidateSource::Asr:
    return "asr";
  case CandidateSource::Cancel:
    return "cancel";
  }
  return "raw";
}

CandidateSource CandidateSourceFromWire(std::string_view source) {
  if (source == "llm") {
    return CandidateSource::Llm;
  }
  if (source == "asr") {
    return CandidateSource::Asr;
  }
  if (source == "cancel") {
    return CandidateSource::Cancel;
  }
  return CandidateSource::Raw;
}

RecognitionPayload ParseRecognitionPayload(std::string_view json) {
  return MakeCommitPlan(json).payload;
}

bool ShouldShowCandidateMenu(const RecognitionPayload &payload, bool command_mode) {
  if (command_mode && payload.candidates.size() > 1) {
    return true;
  }

  std::size_t llm_count = 0;
  for (const auto &candidate : payload.candidates) {
    if (candidate.source == CandidateSource::Llm && ++llm_count > 1) {
      return true;
    }
  }
  return false;
}

CommitPlan MakeCommitPlan(std::string_view json, bool command_mode) {
  const auto rust_plan = MakeRustCommitPlan(json, command_mode);
  if (rust_plan == nullptr) {
    return {};
  }

  return CommitPlan{
      CopyRecognitionPayload(rust_plan.get()),
      vinput_fcitx_commit_plan_show_candidate_menu(rust_plan.get()) != 0,
  };
}

} // namespace vinput_fcitx_bridge
