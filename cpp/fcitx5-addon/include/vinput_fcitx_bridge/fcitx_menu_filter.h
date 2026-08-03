#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>

#include <fcitx-utils/key.h>

struct VinputFcitxMenuFilterState;

namespace vinput_fcitx_bridge {

inline constexpr int kMenuPageSize = 10;

enum class MenuSemanticKeyKind : std::uint8_t {
  Other,
  Passive,
  Escape,
  Slash,
  Backspace,
  DeleteWord,
  ClearFilter,
  Text,
  Page,
  Digit,
  MovePrevious,
  MoveNext,
  Enter,
};

struct MenuSemanticKey {
  MenuSemanticKey() = default;
  explicit MenuSemanticKey(MenuSemanticKeyKind kind, std::int64_t value = 0,
                           std::string text = {})
      : kind(kind), value(value), text(std::move(text)) {}

  MenuSemanticKeyKind kind = MenuSemanticKeyKind::Other;
  std::int64_t value = 0;
  std::string text;
};

enum class MenuKeyAction : std::uint8_t {
  Pass,
  Consume,
  CloseAndPass,
  CloseAndConsume,
  Rebuild,
  MovePrevious,
  MoveNext,
  Select,
};

struct MenuKeyDecision {
  MenuKeyAction action = MenuKeyAction::Pass;
  std::int64_t value = 0;
};

class MenuFilterState {
public:
  MenuFilterState();
  ~MenuFilterState();

  MenuFilterState(const MenuFilterState &) = delete;
  MenuFilterState &operator=(const MenuFilterState &) = delete;
  MenuFilterState(MenuFilterState &&) = delete;
  MenuFilterState &operator=(MenuFilterState &&) = delete;

  void Reset();
  std::optional<bool> active() const;
  std::string DecorateTitle(std::string_view base_title) const;
  std::optional<MenuKeyDecision> HandleKey(bool release, const MenuSemanticKey &key,
                                           bool cursor_available, int current_selection,
                                           int current_page,
                                           std::size_t visible_item_count);
  const ::VinputFcitxMenuFilterState *raw_handle() const;

private:
  ::VinputFcitxMenuFilterState *state_ = nullptr;
};

MenuSemanticKey ClassifyMenuKey(const fcitx::Key &key, bool passive, bool filter_active,
                                const fcitx::KeyList &page_prev_keys,
                                const fcitx::KeyList &page_next_keys);

} // namespace vinput_fcitx_bridge
