#pragma once

#include <cstddef>
#include <functional>
#include <optional>
#include <string>
#include <utility>

namespace vinpst_fcitx_bridge {

struct PresentedCandidate {
  PresentedCandidate() = default;
  PresentedCandidate(std::string text_value, std::string comment_value,
                     bool commit_value, std::string context_source_value = {},
                     bool suppress_commit_context_value = false)
      : text(std::move(text_value)), comment(std::move(comment_value)),
        commit(commit_value), context_source(std::move(context_source_value)),
        suppress_commit_context(suppress_commit_context_value) {}

  std::string text;
  std::string comment;
  bool commit = true;
  std::string context_source;
  bool suppress_commit_context = false;
};

struct ContextEntryPresentation {
  std::string text;
  std::string source;
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

} // namespace vinpst_fcitx_bridge
