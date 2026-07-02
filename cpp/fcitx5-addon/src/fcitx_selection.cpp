#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/surroundingtext.h>

#include <algorithm>
#include <cstdlib>

namespace vinput_fcitx_bridge {

std::optional<SurroundingTextSelectionRange>
SelectedTextDeletionRange(const fcitx::SurroundingText &surrounding_text) {
  if (!surrounding_text.isValid() ||
      surrounding_text.cursor() == surrounding_text.anchor()) {
    return std::nullopt;
  }

  const auto cursor = static_cast<int>(surrounding_text.cursor());
  const auto anchor = static_cast<int>(surrounding_text.anchor());
  const int from = std::min(cursor, anchor);
  const auto size = static_cast<unsigned int>(std::abs(cursor - anchor));
  return SurroundingTextSelectionRange{from - cursor, size};
}

std::string
SelectedTextFromSurroundingText(const fcitx::SurroundingText &surrounding_text) {
  if (!surrounding_text.isValid()) {
    return {};
  }
  return surrounding_text.selectedText();
}

std::string
SelectedTextWithPrimaryFallback(const fcitx::SurroundingText &surrounding_text,
                                std::string_view primary_selection_text) {
  auto selected_text = SelectedTextFromSurroundingText(surrounding_text);
  if (!selected_text.empty()) {
    return selected_text;
  }
  return std::string(primary_selection_text);
}

} // namespace vinput_fcitx_bridge
