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
  using vinput_fcitx_bridge::MenuSemanticKeyKind;

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

  MenuFilterState actions;
  const auto digit_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, false, page_prev, page_next);
  assert(digit_key.kind == MenuSemanticKeyKind::Digit);
  const auto digit_decision = actions.HandleKey(false, digit_key, false, -1, 1, 13);
  assert(digit_decision.has_value());
  assert(digit_decision->action == MenuKeyAction::Select);
  assert(digit_decision->value == 10 + digit_key.value);

  actions.Activate();
  const auto text_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, true, page_prev, page_next);
  assert(text_key.kind == MenuSemanticKeyKind::Text);
  const auto text_decision = actions.HandleKey(false, text_key, false, -1, 0, 0);
  assert(text_decision.has_value());
  assert(text_decision->action == MenuKeyAction::Rebuild);
  assert(actions.query() == "1");

  const auto clear_decision = actions.HandleKey(
      false,
      ClassifyMenuKey(fcitx::Key(FcitxKey_Escape), false, true, page_prev, page_next),
      false, -1, 0, 0);
  assert(clear_decision.has_value());
  assert(clear_decision->action == MenuKeyAction::Rebuild);
  assert(!actions.active());
  assert(actions.query().empty());
  const auto close_decision = actions.HandleKey(
      false,
      ClassifyMenuKey(fcitx::Key(FcitxKey_Escape), false, false, page_prev, page_next),
      false, -1, 0, 0);
  assert(close_decision.has_value());
  assert(close_decision->action == MenuKeyAction::CloseAndConsume);

  const auto page_decision =
      actions.HandleKey(false,
                        ClassifyMenuKey(fcitx::Key(FcitxKey_Page_Down), false, false,
                                        page_prev, page_next),
                        false, -1, 1, 13);
  assert(page_decision.has_value());
  assert(page_decision->action == MenuKeyAction::Rebuild);
  assert(page_decision->value == 2);

  const auto release_pass = actions.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_F1), false, false, page_prev, page_next),
      false, -1, 0, 0);
  assert(release_pass.has_value());
  assert(release_pass->action == MenuKeyAction::Pass);
  const auto release_consume = actions.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_Shift_R), false, false, page_prev, page_next),
      false, -1, 0, 0);
  assert(release_consume.has_value());
  assert(release_consume->action == MenuKeyAction::Consume);

  return 0;
}
