#include "vinpst_fcitx_bridge/fcitx_menu_filter.h"
#include "vinpst_fcitx_bridge/fcitx_menu_paging.h"
#include "vinpst_fcitx_bridge/rust_string.h"

#include "vinpst_fcitx_ffi.h"

#include <algorithm>
#include <cctype>
#include <cstdint>
#include <string>

namespace vinpst_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Other) ==
              VINPST_FCITX_MENU_KEY_OTHER);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Passive) ==
              VINPST_FCITX_MENU_KEY_PASSIVE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Escape) ==
              VINPST_FCITX_MENU_KEY_ESCAPE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Slash) ==
              VINPST_FCITX_MENU_KEY_SLASH);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Backspace) ==
              VINPST_FCITX_MENU_KEY_BACKSPACE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::DeleteWord) ==
              VINPST_FCITX_MENU_KEY_DELETE_WORD);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::ClearFilter) ==
              VINPST_FCITX_MENU_KEY_CLEAR_FILTER);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Text) ==
              VINPST_FCITX_MENU_KEY_TEXT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Page) ==
              VINPST_FCITX_MENU_KEY_PAGE);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Digit) ==
              VINPST_FCITX_MENU_KEY_DIGIT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::MovePrevious) ==
              VINPST_FCITX_MENU_KEY_MOVE_PREVIOUS);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::MoveNext) ==
              VINPST_FCITX_MENU_KEY_MOVE_NEXT);
static_assert(static_cast<std::uint8_t>(MenuSemanticKeyKind::Enter) ==
              VINPST_FCITX_MENU_KEY_ENTER);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Pass) ==
              VINPST_FCITX_MENU_ACTION_PASS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Consume) ==
              VINPST_FCITX_MENU_ACTION_CONSUME);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::CloseAndPass) ==
              VINPST_FCITX_MENU_ACTION_CLOSE_AND_PASS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::CloseAndConsume) ==
              VINPST_FCITX_MENU_ACTION_CLOSE_AND_CONSUME);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Rebuild) ==
              VINPST_FCITX_MENU_ACTION_REBUILD);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::MovePrevious) ==
              VINPST_FCITX_MENU_ACTION_MOVE_PREVIOUS);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::MoveNext) ==
              VINPST_FCITX_MENU_ACTION_MOVE_NEXT);
static_assert(static_cast<std::uint8_t>(MenuKeyAction::Select) ==
              VINPST_FCITX_MENU_ACTION_SELECT);
static_assert(kMenuPageSize == VINPST_FCITX_MENU_PAGE_SIZE);

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
  case VINPST_FCITX_MENU_ACTION_PASS:
    return MenuKeyAction::Pass;
  case VINPST_FCITX_MENU_ACTION_CONSUME:
    return MenuKeyAction::Consume;
  case VINPST_FCITX_MENU_ACTION_CLOSE_AND_PASS:
    return MenuKeyAction::CloseAndPass;
  case VINPST_FCITX_MENU_ACTION_CLOSE_AND_CONSUME:
    return MenuKeyAction::CloseAndConsume;
  case VINPST_FCITX_MENU_ACTION_REBUILD:
    return MenuKeyAction::Rebuild;
  case VINPST_FCITX_MENU_ACTION_MOVE_PREVIOUS:
    return MenuKeyAction::MovePrevious;
  case VINPST_FCITX_MENU_ACTION_MOVE_NEXT:
    return MenuKeyAction::MoveNext;
  case VINPST_FCITX_MENU_ACTION_SELECT:
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
    : state_(StateHandle::Adopt(vinpst_fcitx_menu_session_new())) {}

void MenuSessionState::Open() {
  static_cast<void>(vinpst_fcitx_menu_session_open(state_.mutable_raw_handle()));
}

void MenuSessionState::Close() {
  static_cast<void>(vinpst_fcitx_menu_session_close(state_.mutable_raw_handle()));
}

std::optional<bool> MenuSessionState::is_open() const {
  std::uint8_t open = 0;
  if (vinpst_fcitx_menu_session_is_open(state_.raw_handle(), &open) == 0) {
    return std::nullopt;
  }
  return open != 0;
}

bool MenuSessionState::SetPage(int page) {
  return vinpst_fcitx_menu_session_set_page(state_.mutable_raw_handle(), page) != 0;
}

std::optional<bool> MenuSessionState::active() const {
  std::uint8_t active = 0;
  if (vinpst_fcitx_menu_session_filter_active(state_.raw_handle(), &active) == 0) {
    return std::nullopt;
  }
  return active != 0;
}

std::string MenuSessionState::DecorateTitle(std::string_view base_title) {
  VinpstFcitxStringView title{};
  if (vinpst_fcitx_menu_session_decorate_title(state_.mutable_raw_handle(),
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
  const VinpstFcitxMenuKeyInputView input{
      .release = static_cast<std::uint8_t>(release),
      .key_kind = static_cast<std::uint8_t>(key.kind),
      .key_value = key.value,
      .text = ToRustStringView(key.text),
      .cursor_available = static_cast<std::uint8_t>(cursor_available),
      .current_selection = current_selection,
      .visible_item_count = visible_item_count,
  };
  VinpstFcitxMenuKeyDecisionView decision{};
  if (vinpst_fcitx_menu_session_handle_key(state_.mutable_raw_handle(), &input,
                                           &decision) == 0) {
    return std::nullopt;
  }
  const auto action = ActionFromWire(decision.action);
  if (!action.has_value()) {
    return std::nullopt;
  }
  return MenuKeyDecision{*action, decision.value};
}

const ::VinpstFcitxMenuSession *MenuSessionState::raw_handle() const {
  return state_.raw_handle();
}

void SetMenuCandidatePage(fcitx::CommonCandidateList &candidates, int requested_page) {
  const auto page =
      vinpst_fcitx_clamp_menu_page(candidates.totalPages(), requested_page);
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

} // namespace vinpst_fcitx_bridge
