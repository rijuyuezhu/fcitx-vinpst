#pragma once

#include <string>
#include <string_view>

#include <fcitx-utils/key.h>

namespace vinput_fcitx_bridge {

class MenuFilterState {
public:
  void Reset();
  void Activate();
  void ClearAndDeactivate();
  void Backspace();
  void DeleteLastWord();
  void AppendText(std::string_view text);

  bool active() const {
    return active_;
  }
  const std::string &query() const {
    return query_;
  }
  bool Matches(std::string_view search_text) const;
  std::string DecorateTitle(std::string_view base_title) const;

private:
  bool active_ = false;
  std::string query_;
};

bool IsMenuCtrlShortcut(const fcitx::Key &key, fcitx::KeySym symbol);
bool IsMenuPureModifierKey(const fcitx::Key &key);
bool IsMenuSlashKey(const fcitx::Key &key);
bool IsMenuBackspaceKey(const fcitx::Key &key);
bool IsPrintableMenuInput(const fcitx::Key &key, bool filter_active,
                          const fcitx::KeyList &page_prev_keys,
                          const fcitx::KeyList &page_next_keys);
std::string MenuKeyToUtf8(const fcitx::Key &key);

} // namespace vinput_fcitx_bridge
