#pragma once

#include <optional>
#include <string>
#include <string_view>

namespace fcitx {
class SurroundingText;
}

namespace vinpst_fcitx_bridge {

struct SurroundingTextSelectionRange {
  int offset = 0;
  unsigned int size = 0;
};

std::optional<SurroundingTextSelectionRange>
SelectedTextDeletionRange(const fcitx::SurroundingText &surrounding_text);

std::string
SelectedTextWithPrimaryFallback(const fcitx::SurroundingText &surrounding_text,
                                std::string_view primary_selection_text);

} // namespace vinpst_fcitx_bridge
