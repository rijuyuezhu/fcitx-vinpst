#include "vinput_fcitx_bridge/fcitx_menu_filter.h"

#include <cassert>

int main() {
  using vinput_fcitx_bridge::IsMenuBackspaceKey;
  using vinput_fcitx_bridge::IsMenuCtrlShortcut;
  using vinput_fcitx_bridge::IsMenuPureModifierKey;
  using vinput_fcitx_bridge::IsMenuSlashKey;
  using vinput_fcitx_bridge::IsPrintableMenuInput;
  using vinput_fcitx_bridge::MenuFilterState;

  MenuFilterState filter;
  assert(!filter.active());
  assert(filter.query().empty());
  assert(filter.Matches("Moonshine English"));
  assert(filter.DecorateTitle("Models /filter") == "Models /filter");

  filter.Activate();
  assert(filter.active());
  assert(filter.DecorateTitle("Models /") == "Models /");
  filter.AppendText("MOON en");
  assert(filter.Matches("moonshine English provider"));
  assert(!filter.Matches("moonshine Chinese provider"));
  assert(filter.DecorateTitle("Models /") == "Models /MOON en");

  filter.AppendText(" 中a");
  filter.Backspace();
  assert(filter.query() == "MOON en 中");
  filter.Backspace();
  assert(filter.query() == "MOON en ");
  filter.DeleteLastWord();
  assert(filter.query() == "MOON ");
  filter.DeleteLastWord();
  assert(filter.query().empty());
  assert(!filter.active());

  filter.Activate();
  filter.Backspace();
  assert(!filter.active());
  filter.Activate();
  filter.AppendText("abc");
  filter.ClearAndDeactivate();
  assert(!filter.active());
  assert(filter.query().empty());

  const fcitx::KeyList page_prev{fcitx::Key(FcitxKey_Page_Up),
                                 fcitx::Key(FcitxKey_KP_Page_Up)};
  const fcitx::KeyList page_next{fcitx::Key(FcitxKey_Page_Down),
                                 fcitx::Key(FcitxKey_KP_Page_Down)};
  assert(!IsPrintableMenuInput(fcitx::Key(FcitxKey_1), false, page_prev, page_next));
  assert(IsPrintableMenuInput(fcitx::Key(FcitxKey_1), true, page_prev, page_next));
  assert(IsPrintableMenuInput(fcitx::Key(FcitxKey_a), true, page_prev, page_next));
  assert(
      !IsPrintableMenuInput(fcitx::Key(FcitxKey_Page_Up), true, page_prev, page_next));
  assert(!IsPrintableMenuInput(fcitx::Key(FcitxKey_KP_Page_Down), true, page_prev,
                               page_next));
  assert(!IsPrintableMenuInput(fcitx::Key("Control+w"), true, page_prev, page_next));
  assert(IsMenuCtrlShortcut(fcitx::Key("Control+W"), FcitxKey_w));
  assert(IsMenuSlashKey(fcitx::Key(FcitxKey_slash)));
  assert(IsMenuBackspaceKey(fcitx::Key(FcitxKey_BackSpace)));
  assert(IsMenuPureModifierKey(fcitx::Key(FcitxKey_Shift_R)));

  filter.Activate();
  filter.AppendText(vinput_fcitx_bridge::MenuKeyToUtf8(fcitx::Key(FcitxKey_7)));
  assert(filter.query() == "7");
  filter.Reset();
  assert(!filter.active());
  assert(filter.query().empty());

  return 0;
}
