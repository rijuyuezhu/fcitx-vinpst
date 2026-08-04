#include "vinput_fcitx_bridge/fcitx_menu_filter.h"

#include <cassert>

int main() {
  using vinput_fcitx_bridge::ClassifyMenuKey;
  using vinput_fcitx_bridge::MenuKeyAction;
  using vinput_fcitx_bridge::MenuSemanticKey;
  using vinput_fcitx_bridge::MenuSemanticKeyKind;
  using vinput_fcitx_bridge::MenuSessionState;

  const fcitx::KeyList page_prev{fcitx::Key(FcitxKey_Page_Up),
                                 fcitx::Key(FcitxKey_KP_Page_Up)};
  const fcitx::KeyList page_next{fcitx::Key(FcitxKey_Page_Down),
                                 fcitx::Key(FcitxKey_KP_Page_Down)};
  assert(ClassifyMenuKey(fcitx::Key("Control+W"), false, true, page_prev, page_next)
             .kind == MenuSemanticKeyKind::DeleteWord);
  assert(ClassifyMenuKey(fcitx::Key(FcitxKey_slash), false, false, page_prev, page_next)
             .kind == MenuSemanticKeyKind::Slash);
  assert(
      ClassifyMenuKey(fcitx::Key(FcitxKey_BackSpace), false, true, page_prev, page_next)
          .kind == MenuSemanticKeyKind::Backspace);

  MenuSessionState session;
  const auto initially_open = session.is_open();
  assert(initially_open.has_value() && !*initially_open);
  session.Open();
  const auto initial = session.active();
  assert(initial.has_value());
  assert(!*initial);
  assert(session.DecorateTitle("Models /filter") == "Models /filter");

  const auto slash = session.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Slash}, false, -1, 0);
  assert(slash.has_value() && slash->action == MenuKeyAction::Rebuild);
  const auto text_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, true, page_prev, page_next);
  assert(text_key.kind == MenuSemanticKeyKind::Text);
  const auto text = session.HandleKey(false, text_key, false, -1, 0);
  assert(text.has_value() && text->action == MenuKeyAction::Rebuild);
  const auto filtered = session.active();
  assert(filtered.has_value());
  assert(*filtered);
  assert(session.DecorateTitle("Models /") == "Models /1");

  const auto clear = session.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Escape}, false, -1, 0);
  assert(clear.has_value() && clear->action == MenuKeyAction::Rebuild);
  const auto cleared = session.active();
  assert(cleared.has_value());
  assert(!*cleared);
  const auto close = session.HandleKey(
      false, MenuSemanticKey{MenuSemanticKeyKind::Escape}, false, -1, 0);
  assert(close.has_value() && close->action == MenuKeyAction::CloseAndConsume);
  const auto closed = session.is_open();
  assert(closed.has_value() && !*closed);

  session.Open();
  assert(session.SetPage(1));
  const auto digit_key =
      ClassifyMenuKey(fcitx::Key(FcitxKey_1), false, false, page_prev, page_next);
  assert(digit_key.kind == MenuSemanticKeyKind::Digit);
  const auto digit = session.HandleKey(false, digit_key, false, -1, 13);
  assert(digit.has_value());
  assert(digit->action == MenuKeyAction::Select);
  assert(digit->value == 10 + digit_key.value);

  session.Open();
  assert(session.SetPage(1));
  const auto page =
      session.HandleKey(false,
                        ClassifyMenuKey(fcitx::Key(FcitxKey_Page_Down), false, false,
                                        page_prev, page_next),
                        false, -1, 13);
  assert(page.has_value());
  assert(page->action == MenuKeyAction::Rebuild);
  assert(page->value == 2);

  const auto release_pass = session.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_F1), false, false, page_prev, page_next),
      false, -1, 0);
  assert(release_pass.has_value() && release_pass->action == MenuKeyAction::Pass);
  const auto release_consume = session.HandleKey(
      true,
      ClassifyMenuKey(fcitx::Key(FcitxKey_Shift_R), false, false, page_prev, page_next),
      false, -1, 0);
  assert(release_consume.has_value() &&
         release_consume->action == MenuKeyAction::Consume);

  session.Close();
  const auto reset = session.active();
  assert(reset.has_value() && !*reset);
  return 0;
}
