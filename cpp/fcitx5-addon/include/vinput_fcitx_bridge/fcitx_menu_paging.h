#pragma once

#include <memory>

#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>

namespace vinput_fcitx_bridge {

void SetMenuCandidatePage(fcitx::CommonCandidateList &candidates, int requested_page);

inline void
PublishMenuCandidateList(fcitx::InputContext *input_context,
                         std::unique_ptr<fcitx::CommonCandidateList> candidates) {
  if (input_context == nullptr) {
    return;
  }
  input_context->inputPanel().setCandidateList(std::move(candidates));
}

} // namespace vinput_fcitx_bridge
