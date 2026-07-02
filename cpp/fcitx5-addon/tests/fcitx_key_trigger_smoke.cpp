#include "vinput_fcitx_bridge/fcitx_key_trigger.h"
#include "vinput_fcitx_bridge/scene_defaults.h"

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

#include <cassert>

using vinput_fcitx_bridge::FcitxKeyTriggerPolicy;
using vinput_fcitx_bridge::FcitxTriggerAction;
using vinput_fcitx_bridge::kDefaultCommandSceneId;
using vinput_fcitx_bridge::kDefaultNormalSceneId;

int main() {
  assert(kDefaultNormalSceneId == "__raw__");
  assert(kDefaultCommandSceneId.empty());

  const FcitxKeyTriggerPolicy policy;

  fcitx::KeyEvent control_release(nullptr, fcitx::Key(FcitxKey_Control_R), true);
  assert(policy.IsNormalTrigger(control_release));
  assert(policy.Classify(control_release) == FcitxTriggerAction::StopNormal);

  fcitx::KeyEvent control_press(nullptr, fcitx::Key(FcitxKey_Control_R), false);
  assert(!policy.IsNormalTrigger(control_press));
  assert(policy.Classify(control_press) == FcitxTriggerAction::StartNormal);

  fcitx::KeyEvent shift_release(nullptr, fcitx::Key(FcitxKey_Shift_R), true);
  assert(!policy.IsNormalTrigger(shift_release));
  assert(policy.Classify(shift_release) == FcitxTriggerAction::None);

  fcitx::KeyEvent command_release(nullptr, fcitx::Key(FcitxKey_F10), true);
  assert(policy.IsCommandTrigger(command_release));
  assert(policy.Classify(command_release) == FcitxTriggerAction::StopCommand);
  assert(!policy.IsNormalTrigger(command_release));
  fcitx::KeyEvent command_press(nullptr, fcitx::Key(FcitxKey_F10), false);
  assert(!policy.IsCommandTrigger(command_press));
  assert(policy.Classify(command_press) == FcitxTriggerAction::StartCommand);
  assert(!policy.IsCommandTrigger(control_release));

  const FcitxKeyTriggerPolicy shift_policy{fcitx::Key(FcitxKey_Shift_R),
                                           fcitx::Key(FcitxKey_F9)};
  assert(shift_policy.IsNormalTrigger(shift_release));
  assert(shift_policy.Classify(shift_release) == FcitxTriggerAction::StopNormal);
  assert(!shift_policy.IsNormalTrigger(control_release));
  fcitx::KeyEvent shift_press(nullptr, fcitx::Key(FcitxKey_Shift_R), false);
  assert(shift_policy.Classify(shift_press) == FcitxTriggerAction::StartNormal);
  fcitx::KeyEvent custom_command_release(nullptr, fcitx::Key(FcitxKey_F9), true);
  assert(shift_policy.IsCommandTrigger(custom_command_release));
  assert(shift_policy.Classify(custom_command_release) ==
         FcitxTriggerAction::StopCommand);
  assert(!shift_policy.IsNormalTrigger(custom_command_release));
  fcitx::KeyEvent custom_command_press(nullptr, fcitx::Key(FcitxKey_F9), false);
  assert(!shift_policy.IsCommandTrigger(custom_command_press));
  assert(shift_policy.Classify(custom_command_press) ==
         FcitxTriggerAction::StartCommand);
  assert(!shift_policy.IsCommandTrigger(shift_release));

  return 0;
}
