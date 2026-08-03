#include "vinput_fcitx_bridge/fcitx_menu_filter.h"

#include <cassert>

int main() {
  using vinput_fcitx_bridge::ClassifyMenuKey;
  using vinput_fcitx_bridge::IsMenuBackspaceKey;
  using vinput_fcitx_bridge::IsMenuCtrlShortcut;
  using vinput_fcitx_bridge::IsMenuPureModifierKey;
  using vinput_fcitx_bridge::IsMenuSlashKey;
  using vinput_fcitx_bridge::IsPrintableMenuInput;
  using vinput_fcitx_bridge::MenuFilterState;
  using vinput_fcitx_bridge::MenuKeyAction;
  using vinput_fcitx_bridge::MenuSemanticKey;
  using vinput_fcitx_bridge::MenuSemanticKeyKind;

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

  MenuFilterState filter;
  const auto initial = filter.view();
  assert(initial.has_value());
  assert(!initial->active);
  assert(initial->query.empty());
  assert(filter.DecorateTitle("Models /filter") == "Models /filter");

  const auto slash = filter.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Slash}, false, -1, 0, 0);
  assert(slash.has_value() && slash->action == MenuKeyAction::Rebuild);
  const auto text_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, true, page_prev, page_next);
  assert(text_key.kind == MenuSemanticKeyKind::Text);
  const auto text = filter.HandleKey(false, text_key, false, -1, 0, 0);
  assert(text.has_value() && text->action == MenuKeyAction::Rebuild);
  const auto filtered = filter.view();
  assert(filtered.has_value());
  assert(filtered->active);
  assert(filtered->query == "1");
  assert(filter.DecorateTitle("Models /") == "Models /1");

  const auto clear = filter.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Escape}, false, -1, 0, 0);
  assert(clear.has_value() && clear->action == MenuKeyAction::Rebuild);
  const auto cleared = filter.view();
  assert(cleared.has_value());
  assert(!cleared->active);
  assert(cleared->query.empty());
  const auto close = filter.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Escape}, false, -1, 0, 0);
  assert(close.has_value() && close->action == MenuKeyAction::CloseAndConsume);

  const auto digit_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, false, page_prev, page_next);
  assert(digit_key.kind == MenuSemanticKeyKind::Digit);
  const auto digit = filter.HandleKey(false, digit_key, false, -1, 1, 13);
  assert(digit.has_value());
  assert(digit->action == MenuKeyAction::Select);
  assert(digit->value == 10 + digit_key.value);

  const auto page =
      filter.HandleKey(false,
                       ClassifyMenuKey(fcitx::Key(FcitxKey_Page_Down), false, false,
                                       page_prev, page_next),
                       false, -1, 1, 13);
  assert(page.has_value());
  assert(page->action == MenuKeyAction::Rebuild);
  assert(page->value == 2);

  const auto release_pass = filter.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_F1), false, false, page_prev, page_next),
      false, -1, 0, 0);
  assert(release_pass.has_value() && release_pass->action == MenuKeyAction::Pass);
  const auto release_consume = filter.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_Shift_R), false, false, page_prev, page_next),
      false, -1, 0, 0);
  assert(release_consume.has_value() &&
         release_consume->action == MenuKeyAction::Consume);

  filter.Reset();
  const auto reset = filter.view();
  assert(reset.has_value() && !reset->active && reset->query.empty());
  return 0;
}
