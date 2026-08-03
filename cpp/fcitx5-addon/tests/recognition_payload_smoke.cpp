#include "vinput_fcitx_bridge/recognition_payload.h"

#include "vinput_fcitx_ffi.h"

#include <cassert>
#include <cstdint>
#include <memory>
#include <string_view>

namespace {

struct OutcomeDeleter {
  void operator()(VinputFcitxFrontendOutcome *outcome) const {
    vinput_fcitx_frontend_outcome_free(outcome);
  }
};

using OutcomePtr = std::unique_ptr<VinputFcitxFrontendOutcome, OutcomeDeleter>;

OutcomePtr MakeOutcome(std::string_view json, bool command_mode) {
  return OutcomePtr(vinput_fcitx_frontend_outcome_from_payload(
      reinterpret_cast<const std::uint8_t *>(json.data()), json.size(),
      command_mode ? 1U : 0U));
}

} // namespace

int main() {
  using vinput_fcitx_bridge::CandidateSource;
  using vinput_fcitx_bridge::CopyFrontendOutcome;

  const auto normalized =
      MakeOutcome(R"({"candidates":[{"text":"first","source":"asr"}]})", false);
  const auto normalized_view = CopyFrontendOutcome(normalized.get());
  assert(normalized_view.has_value());
  assert(normalized_view->payload.commit_text == "first");
  assert(normalized_view->payload.candidates.size() == 1);
  assert(normalized_view->payload.candidates[0].source == CandidateSource::Asr);

  const auto candidates = MakeOutcome(
      R"({"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"changed","source":"asr"}]})",
      true);
  const auto candidate_view = CopyFrontendOutcome(candidates.get());
  assert(candidate_view.has_value());
  assert(candidate_view->kind == VINPUT_FCITX_FRONTEND_OUTCOME_CANDIDATE_MENU);
  assert(candidate_view->command_mode);
  assert(candidate_view->payload.candidates.size() == 2);

  const auto invalid = MakeOutcome("not json", false);
  const auto invalid_view = CopyFrontendOutcome(invalid.get());
  assert(invalid_view.has_value());
  assert(invalid_view->kind == VINPUT_FCITX_FRONTEND_OUTCOME_CLEAR);
  assert(invalid_view->payload.commit_text.empty());
  return 0;
}
