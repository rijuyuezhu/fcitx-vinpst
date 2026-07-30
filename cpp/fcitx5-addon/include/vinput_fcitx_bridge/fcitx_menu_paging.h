#pragma once

#include <memory>

#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>

namespace vinput_fcitx_bridge {

inline void SetMenuCandidatePage(fcitx::CommonCandidateList &candidates,
                                 int requested_page) {
  const int total_pages = candidates.totalPages();
  if (total_pages <= 0) {
    return;
  }
  int page = requested_page;
  if (page < 0) {
    page = 0;
  } else if (page >= total_pages) {
    page = total_pages - 1;
  }
  candidates.setPage(page);
}

inline void
PublishMenuCandidateList(fcitx::InputContext *input_context,
                         std::unique_ptr<fcitx::CommonCandidateList> candidates) {
  if (input_context == nullptr) {
    return;
  }
  input_context->inputPanel().setCandidateList(std::move(candidates));
}

} // namespace vinput_fcitx_bridge
