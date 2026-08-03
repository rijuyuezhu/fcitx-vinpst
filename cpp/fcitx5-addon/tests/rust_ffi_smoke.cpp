#include "vinput_fcitx_ffi.h"

#include <cassert>
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <string_view>

namespace {

struct CommitPlanDeleter {
  void operator()(VinputFcitxCommitPlan *plan) const {
    vinput_fcitx_commit_plan_free(plan);
  }
};

using CommitPlanPtr = std::unique_ptr<VinputFcitxCommitPlan, CommitPlanDeleter>;

std::string CopyBytes(const std::uint8_t *data, std::size_t size) {
  if (size == 0) {
    return {};
  }
  assert(data != nullptr);
  return {reinterpret_cast<const char *>(data), size};
}

CommitPlanPtr MakePlan(std::string_view json, bool command_mode) {
  return CommitPlanPtr(
      vinput_fcitx_commit_plan_new(reinterpret_cast<const std::uint8_t *>(json.data()),
                                   json.size(), command_mode ? 1U : 0U));
}

} // namespace

int main() {
  constexpr std::string_view json =
      R"({"commit_text":"selected","candidates":[{"text":"selected","source":"raw"},{"text":"command","source":"asr"}]})";
  const auto plan = MakePlan(json, true);

  assert(plan != nullptr);
  assert(vinput_fcitx_commit_plan_show_candidate_menu(plan.get()) == 1U);
  assert(CopyBytes(vinput_fcitx_commit_plan_text_data(plan.get()),
                   vinput_fcitx_commit_plan_text_len(plan.get())) == "selected");
  assert(vinput_fcitx_commit_plan_candidate_count(plan.get()) == 2U);
  assert(CopyBytes(vinput_fcitx_commit_plan_candidate_text_data(plan.get(), 1),
                   vinput_fcitx_commit_plan_candidate_text_len(plan.get(), 1)) ==
         "command");
  assert(vinput_fcitx_commit_plan_candidate_source(plan.get(), 1) ==
         VINPUT_FCITX_CANDIDATE_SOURCE_ASR);

  return 0;
}
