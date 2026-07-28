#include "vinput_fcitx_bridge/fcitx_menu_filter.h"

#include <algorithm>
#include <cctype>
#include <sstream>
#include <utility>
#include <vector>

namespace vinput_fcitx_bridge {
namespace {

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

std::string NormalizeSearchText(std::string text) {
  std::transform(text.begin(), text.end(), text.begin(),
                 [](unsigned char ch) { return static_cast<char>(std::tolower(ch)); });
  return text;
}

std::vector<std::string> SplitSearchTerms(std::string_view text) {
  std::vector<std::string> terms;
  std::istringstream stream(NormalizeSearchText(std::string(text)));
  std::string term;
  while (stream >> term) {
    terms.push_back(std::move(term));
  }
  return terms;
}

void PopLastUtf8Character(std::string *text) {
  if (text == nullptr || text->empty()) {
    return;
  }
  std::size_t position = text->size();
  do {
    --position;
  } while (position > 0 &&
           (static_cast<unsigned char>((*text)[position]) & 0xc0U) == 0x80U);
  text->erase(position);
}

} // namespace

void MenuFilterState::Reset() {
  active_ = false;
  query_.clear();
}

void MenuFilterState::Activate() {
  active_ = true;
}

void MenuFilterState::ClearAndDeactivate() {
  Reset();
}

void MenuFilterState::Backspace() {
  if (!active_) {
    return;
  }
  if (query_.empty()) {
    active_ = false;
    return;
  }
  PopLastUtf8Character(&query_);
}

void MenuFilterState::DeleteLastWord() {
  if (!active_) {
    return;
  }
  while (!query_.empty() && static_cast<unsigned char>(query_.back()) < 0x80U &&
         std::isspace(static_cast<unsigned char>(query_.back())) != 0) {
    query_.pop_back();
  }
  while (!query_.empty()) {
    const auto last = static_cast<unsigned char>(query_.back());
    if (last < 0x80U && std::isspace(last) != 0) {
      break;
    }
    PopLastUtf8Character(&query_);
  }
  if (query_.empty()) {
    active_ = false;
  }
}

void MenuFilterState::AppendText(std::string_view text) {
  if (active_) {
    query_.append(text);
  }
}

bool MenuFilterState::Matches(std::string_view search_text) const {
  if (query_.empty()) {
    return true;
  }
  const auto normalized_haystack = NormalizeSearchText(std::string(search_text));
  for (const auto &term : SplitSearchTerms(query_)) {
    if (normalized_haystack.find(term) == std::string::npos) {
      return false;
    }
  }
  return true;
}

std::string MenuFilterState::DecorateTitle(std::string_view base_title) const {
  if (!active_ && query_.empty()) {
    return std::string(base_title);
  }
  if (base_title.size() >= 2 && base_title.substr(base_title.size() - 2) == " /") {
    return std::string(base_title) + query_;
  }
  return std::string(base_title) + " / " + query_;
}

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

} // namespace vinput_fcitx_bridge
