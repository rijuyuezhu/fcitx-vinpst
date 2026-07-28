#include "vinput_fcitx_bridge/fcitx_key_trigger.h"
#include "vinput_fcitx_bridge/scene_defaults.h"

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

#include <cassert>
#include <cstdlib>

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
  assert(policy.IsSceneMenuTrigger(shift_release));
  assert(policy.Classify(shift_release) == FcitxTriggerAction::ConsumeSceneMenuRelease);
  fcitx::KeyEvent default_scene_press(nullptr, fcitx::Key(FcitxKey_Shift_R), false);
  assert(policy.Classify(default_scene_press) == FcitxTriggerAction::ShowSceneMenu);

  fcitx::KeyEvent command_release(nullptr, fcitx::Key(FcitxKey_F10), true);
  assert(policy.IsCommandTrigger(command_release));
  assert(policy.Classify(command_release) == FcitxTriggerAction::StopCommand);
  assert(!policy.IsNormalTrigger(command_release));
  fcitx::KeyEvent command_press(nullptr, fcitx::Key(FcitxKey_F10), false);
  assert(!policy.IsCommandTrigger(command_press));
  assert(policy.Classify(command_press) == FcitxTriggerAction::StartCommand);
  assert(!policy.IsCommandTrigger(control_release));

  fcitx::KeyEvent asr_release(nullptr, fcitx::Key(FcitxKey_F8), true);
  assert(policy.IsAsrMenuTrigger(asr_release));
  assert(policy.Classify(asr_release) == FcitxTriggerAction::ConsumeAsrMenuRelease);
  fcitx::KeyEvent asr_press(nullptr, fcitx::Key(FcitxKey_F8), false);
  assert(policy.Classify(asr_press) == FcitxTriggerAction::ShowAsrMenu);

  const FcitxKeyTriggerPolicy shift_policy{{fcitx::Key(FcitxKey_Shift_R)},
                                           {fcitx::Key(FcitxKey_F9)}};
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

  const FcitxKeyTriggerPolicy multi_policy{
      {fcitx::Key(FcitxKey_F5), fcitx::Key(FcitxKey_F6)},
      {fcitx::Key(FcitxKey_F9)},
      {fcitx::Key(FcitxKey_F7)},
      {fcitx::Key(FcitxKey_F8)},
  };
  fcitx::KeyEvent second_normal_press(nullptr, fcitx::Key(FcitxKey_F6), false);
  assert(multi_policy.Classify(second_normal_press) == FcitxTriggerAction::StartNormal);

  unsetenv("VINPUT_FCITX_NORMAL_TRIGGER");
  unsetenv("VINPUT_FCITX_COMMAND_TRIGGER");
  unsetenv("VINPUT_FCITX_SCENE_MENU_TRIGGER");
  unsetenv("VINPUT_FCITX_ASR_MENU_TRIGGER");
  const auto default_env_policy = FcitxKeyTriggerPolicy::FromEnvironment();
  assert(default_env_policy.normal_triggers().front().check(
      fcitx::Key(FcitxKey_Control_R)));
  assert(default_env_policy.command_triggers().front().check(fcitx::Key(FcitxKey_F10)));
  assert(default_env_policy.scene_menu_triggers().front().check(
      fcitx::Key(FcitxKey_Shift_R)));
  assert(default_env_policy.asr_menu_triggers().front().check(fcitx::Key(FcitxKey_F8)));

  setenv("VINPUT_FCITX_NORMAL_TRIGGER", "F8", 1);
  setenv("VINPUT_FCITX_COMMAND_TRIGGER", "F9", 1);
  setenv("VINPUT_FCITX_SCENE_MENU_TRIGGER", "F7", 1);
  setenv("VINPUT_FCITX_ASR_MENU_TRIGGER", "F6", 1);
  const auto custom_env_policy = FcitxKeyTriggerPolicy::FromEnvironment();
  fcitx::KeyEvent env_normal_press(nullptr, fcitx::Key(FcitxKey_F8), false);
  assert(custom_env_policy.Classify(env_normal_press) ==
         FcitxTriggerAction::StartNormal);
  fcitx::KeyEvent env_command_release(nullptr, fcitx::Key(FcitxKey_F9), true);
  assert(custom_env_policy.Classify(env_command_release) ==
         FcitxTriggerAction::StopCommand);
  fcitx::KeyEvent env_scene_press(nullptr, fcitx::Key(FcitxKey_F7), false);
  assert(custom_env_policy.Classify(env_scene_press) ==
         FcitxTriggerAction::ShowSceneMenu);
  fcitx::KeyEvent env_asr_press(nullptr, fcitx::Key(FcitxKey_F6), false);
  assert(custom_env_policy.Classify(env_asr_press) == FcitxTriggerAction::ShowAsrMenu);

  setenv("VINPUT_FCITX_NORMAL_TRIGGER", "not-a-key", 1);
  setenv("VINPUT_FCITX_COMMAND_TRIGGER", "", 1);
  setenv("VINPUT_FCITX_SCENE_MENU_TRIGGER", "not-a-key", 1);
  setenv("VINPUT_FCITX_ASR_MENU_TRIGGER", "not-a-key", 1);
  const auto fallback_env_policy = FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
      {fcitx::Key(FcitxKey_F5), fcitx::Key(FcitxKey_F6)}, {fcitx::Key(FcitxKey_F9)},
      {fcitx::Key(FcitxKey_F7)}, {fcitx::Key(FcitxKey_F8)});
  assert(fallback_env_policy.normal_triggers().size() == 2);
  fcitx::KeyEvent persistent_second_press(nullptr, fcitx::Key(FcitxKey_F6), false);
  assert(fallback_env_policy.Classify(persistent_second_press) ==
         FcitxTriggerAction::StartNormal);
  assert(fallback_env_policy.command_triggers().front().check(fcitx::Key(FcitxKey_F9)));
  assert(
      fallback_env_policy.scene_menu_triggers().front().check(fcitx::Key(FcitxKey_F7)));
  assert(
      fallback_env_policy.asr_menu_triggers().front().check(fcitx::Key(FcitxKey_F8)));
  unsetenv("VINPUT_FCITX_NORMAL_TRIGGER");
  unsetenv("VINPUT_FCITX_COMMAND_TRIGGER");
  unsetenv("VINPUT_FCITX_SCENE_MENU_TRIGGER");
  unsetenv("VINPUT_FCITX_ASR_MENU_TRIGGER");

  return 0;
}
