#include "vinpst_fcitx_bridge/fcitx_outcome.h"

#include "vinpst_fcitx_bridge/fcitx_candidates.h"
#include "vinpst_fcitx_bridge/fcitx_selection.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string>
#include <utility>

namespace vinpst_fcitx_bridge {
namespace {

void ClearCandidateMenu(fcitx::InputContext *input_context) {
  ClearResultCandidateMenu(input_context);
}

void SetPreedit(fcitx::InputContext *input_context, std::string_view text) {
  ClearCandidateMenu(input_context);
  fcitx::Text preedit;
  if (!text.empty()) {
    preedit.append(std::string(text));
  }
  input_context->inputPanel().setPreedit(preedit);
  input_context->updatePreedit();
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

void DeleteSelectedTextIfAny(fcitx::InputContext *input_context) {
  const auto range = SelectedTextDeletionRange(input_context->surroundingText());
  if (range.has_value()) {
    input_context->deleteSurroundingText(range->offset, range->size);
  }
}

void CommitText(fcitx::InputContext *input_context, std::string_view text,
                bool replace_selection) {
  if (replace_selection) {
    DeleteSelectedTextIfAny(input_context);
  }
  ClearCandidateMenu(input_context);
  SetPreedit(input_context, "");
  input_context->commitString(std::string(text));
}

bool ShowCandidateMenu(fcitx::InputContext *input_context,
                       const CandidatePresentation &presentation,
                       bool replace_selection,
                       ResultCandidateSelectCallback on_candidate_select) {
  if (!on_candidate_select) {
    on_candidate_select = [replace_selection](fcitx::InputContext *selected_context,
                                               const PresentedCandidate &candidate) {
      ApplyResultCandidateSelection(selected_context, candidate, replace_selection);
    };
  }
  auto candidate_list = BuildResultCandidateList(presentation, on_candidate_select);
  if (candidate_list == nullptr) {
    return false;
  }
  SetPreedit(input_context, "");
  fcitx::Text aux_up;
  aux_up.append(ResultCandidateMenuTitle(presentation.candidate_count));
  input_context->inputPanel().setAuxUp(aux_up);
  input_context->inputPanel().setCandidateList(std::move(candidate_list));
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  return true;
}

} // namespace

AppliedOutcome ApplyBridgeOutcomeToInputContext(
    const BridgeOutcome &outcome, fcitx::InputContext *input_context,
    ResultCandidateSelectCallback on_candidate_select) {
  if (input_context == nullptr) {
    return AppliedOutcome::None;
  }

  switch (outcome.kind) {
  case BridgeOutcome::Kind::None:
    return AppliedOutcome::None;
  case BridgeOutcome::Kind::Preedit:
  case BridgeOutcome::Kind::Error:
    SetPreedit(input_context, outcome.text);
    return AppliedOutcome::Preedit;
  case BridgeOutcome::Kind::Clear:
    SetPreedit(input_context, "");
    return AppliedOutcome::Clear;
  case BridgeOutcome::Kind::Commit:
    if (outcome.text.empty()) {
      return AppliedOutcome::None;
    }
    CommitText(input_context, outcome.text, outcome.replace_selection);
    return AppliedOutcome::Commit;
  case BridgeOutcome::Kind::CandidateMenu:
    if (ShowCandidateMenu(input_context, outcome.candidate_menu,
                          outcome.replace_selection, std::move(on_candidate_select))) {
      return AppliedOutcome::CandidateMenu;
    }
    if (outcome.text.empty()) {
      return AppliedOutcome::None;
    }
    CommitText(input_context, outcome.text, outcome.replace_selection);
    return AppliedOutcome::Commit;
  }

  return AppliedOutcome::None;
}

} // namespace vinpst_fcitx_bridge
