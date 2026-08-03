#include "vinput_fcitx_bridge/fcitx_menu_filter.h"
#include "vinput_fcitx_bridge/fcitx_menu_paging.h"
#include "vinput_fcitx_bridge/rust_string.h"

#include "vinput_fcitx_ffi.h"

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <string>

namespace vinput_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Other) ==
              VINPUT_FCITX_MENU_KEY_OTHER);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Passive) ==
              VINPUT_FCITX_MENU_KEY_PASSIVE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Escape) ==
              VINPUT_FCITX_MENU_KEY_ESCAPE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Slash) ==
              VINPUT_FCITX_MENU_KEY_SLASH);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Backspace) ==
              VINPUT_FCITX_MENU_KEY_BACKSPACE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::DeleteWord) ==
              VINPUT_FCITX_MENU_KEY_DELETE_WORD);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::ClearFilter) ==
              VINPUT_FCITX_MENU_KEY_CLEAR_FILTER);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Text) ==
              VINPUT_FCITX_MENU_KEY_TEXT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Page) ==
              VINPUT_FCITX_MENU_KEY_PAGE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Digit) ==
              VINPUT_FCITX_MENU_KEY_DIGIT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::MovePrevious) ==
              VINPUT_FCITX_MENU_KEY_MOVE_PREVIOUS);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::MoveNext) ==
              VINPUT_FCITX_MENU_KEY_MOVE_NEXT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Enter) ==
              VINPUT_FCITX_MENU_KEY_ENTER);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Pass) ==
              VINPUT_FCITX_MENU_ACTION_PASS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Consume) ==
              VINPUT_FCITX_MENU_ACTION_CONSUME);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::CloseAndPass) ==
              VINPUT_FCITX_MENU_ACTION_CLOSE_AND_PASS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::CloseAndConsume) ==
              VINPUT_FCITX_MENU_ACTION_CLOSE_AND_CONSUME);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Rebuild) ==
              VINPUT_FCITX_MENU_ACTION_REBUILD);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::MovePrevious) ==
              VINPUT_FCITX_MENU_ACTION_MOVE_PREVIOUS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::MoveNext) ==
              VINPUT_FCITX_MENU_ACTION_MOVE_NEXT);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Select) ==
              VINPUT_FCITX_MENU_ACTION_SELECT);
static_assert(kMenuPageSize == VINPUT_FCITX_MENU_PAGE_SIZE);

bool HasNoModifiers(const fcitx::Key &key) {
  return key.normalize().states() == fcitx::KeyStates();
}

bool IsKeySymbol(const fcitx::Key &key, fcitx::KeySym symbol) {
  const auto normalized = key.normalize();
  return normalized.sym() == symbol && HasNoModifiers(normalized);
}

bool IsOneOfKeySymbols(const fcitx::Key &key,
                       std::initializer_list<fcitx::KeySym> symbols) {
  return std::any_of(symbols.begin(), symbols.end(),
                     [&key](fcitx::KeySym symbol) { return IsKeySymbol(key, symbol); });
}

std::optional<MenuKeyAction> ActionFromWire(std::uint8_t action) {
  switch (action) {
  case VINPUT_FCITX_MENU_ACTION_PASS:
    return MenuKeyAction::Pass;
  case VINPUT_FCITX_MENU_ACTION_CONSUME:
    return MenuKeyAction::Consume;
  case VINPUT_FCITX_MENU_ACTION_CLOSE_AND_PASS:
    return MenuKeyAction::CloseAndPass;
  case VINPUT_FCITX_MENU_ACTION_CLOSE_AND_CONSUME:
    return MenuKeyAction::CloseAndConsume;
  case VINPUT_FCITX_MENU_ACTION_REBUILD:
    return MenuKeyAction::Rebuild;
  case VINPUT_FCITX_MENU_ACTION_MOVE_PREVIOUS:
    return MenuKeyAction::MovePrevious;
  case VINPUT_FCITX_MENU_ACTION_MOVE_NEXT:
    return MenuKeyAction::MoveNext;
  case VINPUT_FCITX_MENU_ACTION_SELECT:
    return MenuKeyAction::Select;
  default:
    return std::nullopt;
  }
}

bool IsMenuEnterKey(const fcitx::Key &key) {
  return IsOneOfKeySymbols(key, {FcitxKey_Return, FcitxKey_KP_Enter});
}

} // namespace

MenuSessionState::MenuSessionState()
    : state_(StateHandle::Adopt(vinput_fcitx_menu_session_new())) {}

void MenuSessionState::Open() {
  static_cast<void>(vinput_fcitx_menu_session_open(state_.mutable_raw_handle()));
}

void MenuSessionState::Close() {
  static_cast<void>(vinput_fcitx_menu_session_close(state_.mutable_raw_handle()));
}

std::optional<bool> MenuSessionState::is_open() const {
  std::uint8_t open = 0;
  if (vinput_fcitx_menu_session_is_open(state_.raw_handle(), &open) == 0) {
    return std::nullopt;
  }
  return open != 0;
}

bool MenuSessionState::SetPage(int page) {
  return vinput_fcitx_menu_session_set_page(state_.mutable_raw_handle(), page) != 0;
}

std::optional<bool> MenuSessionState::active() const {
  std::uint8_t active = 0;
  if (vinput_fcitx_menu_session_filter_active(state_.raw_handle(), &active) == 0) {
    return std::nullopt;
  }
  return active != 0;
}

std::string MenuSessionState::DecorateTitle(std::string_view base_title) {
  VinputFcitxStringView title{};
  if (vinput_fcitx_menu_session_decorate_title(state_.mutable_raw_handle(),
                                               RustBytes(base_title), base_title.size(),
                                               &title) == 0) {
    return std::string(base_title);
  }
  return CopyRustString(title);
}

std::optional<MenuKeyDecision>
MenuSessionState::HandleKey(bool release, const MenuSemanticKey &key,
                            bool cursor_available, int current_selection,
                            std::size_t visible_item_count) {
  VinputFcitxMenuKeyDecisionView decision{};
  if (vinput_fcitx_menu_session_handle_key(
          state_.mutable_raw_handle(), static_cast<std::uint8_t>(release),
          static_cast<std::uint8_t>(key.kind), key.value, RustBytes(key.text),
          key.text.size(), static_cast<std::uint8_t>(cursor_available),
          current_selection, visible_item_count, &decision) == 0) {
    return std::nullopt;
  }
  const auto action = ActionFromWire(decision.action);
  if (!action.has_value()) {
    return std::nullopt;
  }
  return MenuKeyDecision{*action, decision.value};
}

const ::VinputFcitxMenuSession *MenuSessionState::raw_handle() const {
  return state_.raw_handle();
}

void SetMenuCandidatePage(fcitx::CommonCandidateList &candidates, int requested_page) {
  const auto page =
      vinput_fcitx_clamp_menu_page(candidates.totalPages(), requested_page);
  if (page >= 0) {
    candidates.setPage(page);
  }
}

namespace {

bool IsMenuCtrlShortcut(const fcitx::Key &key, fcitx::KeySym symbol) {
  const auto matches = [symbol](const fcitx::Key &candidate) {
    if (candidate.states() != fcitx::KeyState::Ctrl) {
      return false;
    }
    if (candidate.sym() == symbol) {
      return true;
    }
    const auto expected = fcitx::Key::keySymToUnicode(symbol);
    const auto actual = fcitx::Key::keySymToUnicode(candidate.sym());
    if (expected == 0 || actual == 0 || expected > 0xffU || actual > 0xffU) {
      return false;
    }
    return std::tolower(static_cast<unsigned char>(actual)) ==
           std::tolower(static_cast<unsigned char>(expected));
  };
  return matches(key) || matches(key.normalize());
}

bool IsMenuPureModifierKey(const fcitx::Key &key) {
  return key.normalize().isModifier();
}

bool IsMenuSlashKey(const fcitx::Key &key) {
  return IsKeySymbol(key, FcitxKey_slash);
}

bool IsMenuBackspaceKey(const fcitx::Key &key) {
  return IsKeySymbol(key, FcitxKey_BackSpace);
}

bool IsPrintableMenuInput(const fcitx::Key &key, bool filter_active,
                          const fcitx::KeyList &page_prev_keys,
                          const fcitx::KeyList &page_next_keys) {
  if (!filter_active) {
    return false;
  }
  if (IsOneOfKeySymbols(key, {FcitxKey_Return, FcitxKey_KP_Enter, FcitxKey_Escape,
                              FcitxKey_slash, FcitxKey_BackSpace, FcitxKey_Up,
                              FcitxKey_Down}) ||
      key.checkKeyList(page_prev_keys) || key.checkKeyList(page_next_keys) ||
      IsMenuCtrlShortcut(key, FcitxKey_w) || IsMenuCtrlShortcut(key, FcitxKey_u) ||
      IsMenuPureModifierKey(key)) {
    return false;
  }
  const auto normalized = key.normalize();
  if (normalized.states() != fcitx::KeyStates()) {
    return false;
  }
  const auto utf8 = fcitx::Key::keySymToUTF8(normalized.sym());
  if (utf8.empty()) {
    return false;
  }
  return std::none_of(utf8.begin(), utf8.end(),
                      [](unsigned char ch) { return ch < 0x20U || ch == 0x7fU; });
}

std::string MenuKeyToUtf8(const fcitx::Key &key) {
  return fcitx::Key::keySymToUTF8(key.normalize().sym());
}

} // namespace

MenuSemanticKey ClassifyMenuKey(const fcitx::Key &key, bool passive, bool filter_active,
                                const fcitx::KeyList &page_prev_keys,
                                const fcitx::KeyList &page_next_keys) {
  if (passive || IsMenuPureModifierKey(key)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Passive};
  }
  if (IsKeySymbol(key, FcitxKey_Escape)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Escape};
  }
  if (IsMenuSlashKey(key)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Slash};
  }
  if (IsMenuBackspaceKey(key)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Backspace};
  }
  if (IsMenuCtrlShortcut(key, FcitxKey_w)) {
    return MenuSemanticKey{MenuSemanticKeyKind::DeleteWord};
  }
  if (IsMenuCtrlShortcut(key, FcitxKey_u)) {
    return MenuSemanticKey{MenuSemanticKeyKind::ClearFilter};
  }
  if (IsPrintableMenuInput(key, filter_active, page_prev_keys, page_next_keys)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Text, 0, MenuKeyToUtf8(key)};
  }
  if (key.checkKeyList(page_prev_keys)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Page, -1};
  }
  if (key.checkKeyList(page_next_keys)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Page, 1};
  }
  const int digit = key.digitSelection();
  if (digit >= 0) {
    return MenuSemanticKey{MenuSemanticKeyKind::Digit, digit};
  }
  if (IsKeySymbol(key, FcitxKey_Up)) {
    return MenuSemanticKey{MenuSemanticKeyKind::MovePrevious};
  }
  if (IsKeySymbol(key, FcitxKey_Down)) {
    return MenuSemanticKey{MenuSemanticKeyKind::MoveNext};
  }
  if (IsMenuEnterKey(key)) {
    return MenuSemanticKey{MenuSemanticKeyKind::Enter};
  }
  return {};
}

} // namespace vinput_fcitx_bridge
