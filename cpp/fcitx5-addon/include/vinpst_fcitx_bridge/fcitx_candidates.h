#pragma once

#include "vinpst_fcitx_bridge/frontend_presentation.h"

#include <fcitx/candidatelist.h>

#include <functional>
#include <memory>
#include <string>

namespace vinpst_fcitx_bridge {

using ResultCandidateSelectCallback =
    std::function<void(fcitx::InputContext *, const PresentedCandidate &)>;

std::string ResultCandidateMenuTitle(std::size_t count);

void ClearResultCandidateMenu(fcitx::InputContext *input_context);

void ApplyResultCandidateSelection(fcitx::InputContext *input_context,
                                   const PresentedCandidate &candidate,
                                   bool replace_selection);

std::unique_ptr<fcitx::CommonCandidateList>
BuildResultCandidateList(const CandidatePresentation &payload,
                         const ResultCandidateSelectCallback &on_select);

} // namespace vinpst_fcitx_bridge
