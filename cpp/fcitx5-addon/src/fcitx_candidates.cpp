#include "vinput_fcitx_bridge/fcitx_candidates.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"
#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr int kResultMenuPageSize = 5;

void DeleteSelectedTextIfAny(fcitx::InputContext *input_context) {
  if (input_context == nullptr) {
    return;
  }
  auto range = SelectedTextDeletionRange(input_context->surroundingText());
  if (!range.has_value()) {
    return;
  }
  input_context->deleteSurroundingText(range->offset, range->size);
}

class ResultCandidateWord final : public fcitx::CandidateWord {
public:
  ResultCandidateWord(PresentedCandidate candidate, std::string_view comment,
                      ResultCandidateSelectCallback on_select)
      : fcitx::CandidateWord(fcitx::Text(candidate.text)),
        candidate_(std::move(candidate)), on_select_(std::move(on_select)) {
#ifdef VINPUT_FCITX5_CORE_HAVE_CANDIDATE_COMMENT
    if (!comment.empty()) {
      setComment(fcitx::Text(std::string(comment)));
    }
#else
    (void)comment;
#endif
  }

  void select(fcitx::InputContext *input_context) const override {
    if (on_select_) {
      on_select_(input_context, candidate_);
    }
  }

private:
  PresentedCandidate candidate_;
  ResultCandidateSelectCallback on_select_;
};

} // namespace

std::string ResultCandidateMenuTitle(std::size_t count) {
  return FrontendCountText("Choose Result (%zu)", count);
}

void ClearResultCandidateMenu(fcitx::InputContext *input_context) {
  if (input_context == nullptr) {
    return;
  }

  fcitx::Text empty;
  input_context->inputPanel().setAuxUp(empty);
  input_context->inputPanel().setCandidateList({});
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

void ApplyResultCandidateSelection(fcitx::InputContext *input_context,
                                   const PresentedCandidate &candidate,
                                   bool replace_selection) {
  if (input_context == nullptr) {
    return;
  }

  ClearResultCandidateMenu(input_context);
  fcitx::Text empty;
  input_context->inputPanel().setPreedit(empty);
  input_context->updatePreedit();

  if (!candidate.commit) {
    return;
  }
  if (replace_selection) {
    DeleteSelectedTextIfAny(input_context);
  }

  input_context->commitString(candidate.text);
}

std::unique_ptr<fcitx::CommonCandidateList>
BuildResultCandidateList(const CandidatePresentation &payload,
                         const ResultCandidateSelectCallback &on_select) {
  if (payload.candidate_count == 0 || !payload.candidate_at) {
    return nullptr;
  }

  auto candidate_list = std::make_unique<fcitx::CommonCandidateList>();
  candidate_list->setPageSize(kResultMenuPageSize);
  candidate_list->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidate_list->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);

  for (std::size_t index = 0; index < payload.candidate_count; ++index) {
    auto candidate = payload.candidate(index);
    if (!candidate.has_value()) {
      return nullptr;
    }
    candidate_list->append<ResultCandidateWord>(*candidate, candidate->comment,
                                                on_select);
  }
  const auto cursor_index =
      payload.cursor_index < payload.candidate_count ? payload.cursor_index : 0;
  candidate_list->setGlobalCursorIndex(static_cast<int>(cursor_index));
  return candidate_list;
}

} // namespace vinput_fcitx_bridge
