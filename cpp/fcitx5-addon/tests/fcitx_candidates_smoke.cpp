#include "vinpst_fcitx_bridge/fcitx_candidates.h"

#include <fcitx/candidatelist.h>
#include <fcitx/text.h>

#include <cassert>
#include <memory>
#include <vector>

using vinpst_fcitx_bridge::BuildResultCandidateList;
using vinpst_fcitx_bridge::CandidatePresentation;
using vinpst_fcitx_bridge::PresentedCandidate;
using vinpst_fcitx_bridge::ResultCandidateMenuTitle;

namespace {

void IgnoreSelection(fcitx::InputContext *, const PresentedCandidate &) {}

PresentedCandidate Row(std::string text, std::string comment, bool commit = true) {
  return PresentedCandidate{std::move(text), std::move(comment), commit};
}

CandidatePresentation Rows(std::vector<PresentedCandidate> rows,
                           std::size_t cursor_index = 0) {
  auto shared_rows =
      std::make_shared<const std::vector<PresentedCandidate>>(std::move(rows));
  return CandidatePresentation{
      .candidate_count = shared_rows->size(),
      .cursor_index = cursor_index,
      .candidate_at =
          [shared_rows](std::size_t index) -> std::optional<PresentedCandidate> {
        if (index >= shared_rows->size()) {
          return std::nullopt;
        }
        return (*shared_rows)[index];
      },
  };
}

} // namespace

int main() {
  assert(BuildResultCandidateList(Rows({}), IgnoreSelection) == nullptr);

  assert(ResultCandidateMenuTitle(0) == "Choose Result (0)");
  assert(ResultCandidateMenuTitle(1) == "Choose Result (1)");
  assert(ResultCandidateMenuTitle(3) == "Choose Result (3)");

  auto payload = Rows(
      {
          Row("raw transcript", "Original"),
          Row("polished 1", "1"),
          Row("polished 2", "2"),
      },
      2);

  std::vector<PresentedCandidate> selected_candidates;
  auto candidates = BuildResultCandidateList(
      payload, [&selected_candidates](fcitx::InputContext *input_context,
                                      const PresentedCandidate &candidate) {
        assert(input_context == nullptr);
        selected_candidates.push_back(candidate);
      });
  assert(candidates != nullptr);
  assert(candidates->totalSize() == 3);
  assert(candidates->size() == 3);
  assert(candidates->pageSize() == 5);
  assert(candidates->layoutHint() == fcitx::CandidateLayoutHint::Vertical);
  assert(candidates->globalCursorIndex() == 2);
  assert(candidates->candidateFromAll(0).text().toString() == "raw transcript");
#ifdef VINPST_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(candidates->candidateFromAll(0).comment().toString() == "Original");
  assert(candidates->candidateFromAll(1).comment().toString() == "1");
  assert(candidates->candidateFromAll(2).comment().toString() == "2");
#endif

  candidates->candidateFromAll(1).select(nullptr);
  assert(selected_candidates.size() == 1);
  assert(selected_candidates[0].text == "polished 1");
  assert(selected_candidates[0].comment == "1");
  assert(selected_candidates[0].commit);

  auto asr_candidates = BuildResultCandidateList(
      Rows({Row("asr choice", "Voice Command")}), IgnoreSelection);
  assert(asr_candidates != nullptr);
#ifdef VINPST_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(asr_candidates->candidateFromAll(0).comment().toString() == "Voice Command");
#endif

  auto fallback_candidates = BuildResultCandidateList(
      Rows({Row("raw transcript", "Original"), Row("polished", "1")}, 99),
      IgnoreSelection);
  assert(fallback_candidates != nullptr);
  assert(fallback_candidates->globalCursorIndex() == 0);

  auto cancel_candidates = BuildResultCandidateList(
      Rows({Row("", "Cancel", false)}),
      [&selected_candidates](fcitx::InputContext *input_context,
                             const PresentedCandidate &candidate) {
        assert(input_context == nullptr);
        selected_candidates.push_back(candidate);
      });
  assert(cancel_candidates != nullptr);
  assert(cancel_candidates->totalSize() == 1);
  assert(cancel_candidates->globalCursorIndex() == 0);
#ifdef VINPST_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
  assert(cancel_candidates->candidateFromAll(0).comment().toString() == "Cancel");
#endif
  cancel_candidates->candidateFromAll(0).select(nullptr);
  assert(selected_candidates.size() == 2);
  assert(selected_candidates[1].text.empty());
  assert(!selected_candidates[1].commit);

  auto paged_candidates = BuildResultCandidateList(Rows(
                                                       {
                                                           Row("choice 1", "1"),
                                                           Row("choice 2", "2"),
                                                           Row("choice 3", "3"),
                                                           Row("choice 4", "4"),
                                                           Row("choice 5", "5"),
                                                           Row("choice 6", "6"),
                                                       },
                                                       5),
                                                   IgnoreSelection);
  assert(paged_candidates != nullptr);
  assert(ResultCandidateMenuTitle(paged_candidates->totalSize()) ==
         "Choose Result (6)");
  assert(paged_candidates->totalSize() == 6);
  assert(paged_candidates->size() == 5);
  assert(paged_candidates->pageSize() == 5);
  assert(paged_candidates->layoutHint() == fcitx::CandidateLayoutHint::Vertical);
  assert(paged_candidates->globalCursorIndex() == 5);
  assert(paged_candidates->candidateFromAll(5).text().toString() == "choice 6");


  return 0;
}
