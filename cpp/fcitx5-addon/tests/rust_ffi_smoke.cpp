#include "vinput_fcitx_ffi.h"

#include <cassert>
#include <cstdint>
#include <memory>
#include <string>
#include <string_view>

namespace {

struct OutcomeDeleter {
  void operator()(VinputFcitxFrontendOutcome *outcome) const {
    vinput_fcitx_frontend_outcome_free(outcome);
  }
};

using OutcomePtr = std::unique_ptr<VinputFcitxFrontendOutcome, OutcomeDeleter>;

std::string CopyText(VinputFcitxStringView view) {
  if (view.len == 0) {
    return {};
  }
  assert(view.data != nullptr);
  return {reinterpret_cast<const char *>(view.data), view.len};
}

} // namespace

int main() {
  constexpr std::string_view json =
      R"({"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"command","source":"asr"}]})";
  const OutcomePtr outcome(vinput_fcitx_frontend_outcome_from_payload(
      reinterpret_cast<const std::uint8_t *>(json.data()), json.size(), 1U));
  assert(outcome != nullptr);

  VinputFcitxFrontendOutcomeView view{};
  assert(vinput_fcitx_frontend_outcome_view(outcome.get(), &view) == 1U);
  assert(view.kind == VINPUT_FCITX_FRONTEND_OUTCOME_CANDIDATE_MENU);
  assert(view.command_mode == 1U);
  assert(CopyText(view.commit_text) == "selected");
  assert(view.candidate_count == 2U);

  VinputFcitxCandidateView candidate{};
  assert(vinput_fcitx_frontend_outcome_candidate(outcome.get(), 1, &candidate) == 1U);
  assert(CopyText(candidate.text) == "command");
  assert(candidate.source == VINPUT_FCITX_CANDIDATE_SOURCE_ASR);
  return 0;
}
