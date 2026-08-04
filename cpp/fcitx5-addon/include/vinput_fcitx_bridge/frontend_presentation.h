#pragma once

#include <cstddef>
#include <functional>
#include <optional>
#include <string>

namespace vinput_fcitx_bridge {

struct PresentedCandidate {
  std::string text;
  std::string comment;
  bool commit = true;
};

struct CandidatePresentation {
  std::size_t candidate_count = 0;
  std::size_t cursor_index = 0;
  std::function<std::optional<PresentedCandidate>(std::size_t)> candidate_at;

  std::optional<PresentedCandidate> candidate(std::size_t index) const {
    if (index >= candidate_count || !candidate_at) {
      return std::nullopt;
    }
    return candidate_at(index);
  }
};

} // namespace vinput_fcitx_bridge
