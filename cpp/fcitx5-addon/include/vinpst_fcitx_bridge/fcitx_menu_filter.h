#pragma once

#include "vinpst_fcitx_bridge/rust_handle.h"
#include "vinpst_fcitx_ffi.h"

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <utility>

#include <fcitx-utils/key.h>

namespace vinpst_fcitx_bridge {

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

class MenuSessionState {
public:
  MenuSessionState();
  ~MenuSessionState() = default;

  MenuSessionState(const MenuSessionState &) = delete;
  MenuSessionState &operator=(const MenuSessionState &) = delete;
  MenuSessionState(MenuSessionState &&) = delete;
  MenuSessionState &operator=(MenuSessionState &&) = delete;

  void Open();
  void Close();
  std::optional<bool> is_open() const;
  bool SetPage(int page);
  std::optional<bool> active() const;
  std::string DecorateTitle(std::string_view base_title);
  std::optional<MenuKeyDecision> HandleKey(bool release, const MenuSemanticKey &key,
                                           bool cursor_available, int current_selection,
                                           std::size_t visible_item_count);
  const ::VinpstFcitxMenuSession *raw_handle() const;

private:
  using StateHandle =
      RustOwnedHandle<::VinpstFcitxMenuSession, vinpst_fcitx_menu_session_free>;

  StateHandle state_;
};

MenuSemanticKey ClassifyMenuKey(const fcitx::Key &key, bool passive, bool filter_active,
                                const fcitx::KeyList &page_prev_keys,
                                const fcitx::KeyList &page_next_keys);

} // namespace vinpst_fcitx_bridge
