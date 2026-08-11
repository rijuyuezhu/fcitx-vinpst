#include "vinpst_fcitx_bridge/fcitx_key_trigger.h"
#include <fcitx-utils/key.h>
#include <fcitx/event.h>

#include <cassert>
#include <cstdlib>

using vinpst_fcitx_bridge::FcitxKeyTriggerPolicy;
using vinpst_fcitx_bridge::FcitxTriggerAction;
int main() {
  const FcitxKeyTriggerPolicy policy;

  fcitx::KeyEvent control_release(nullptr, fcitx::Key(FcitxKey_Control_R), true);
  assert(policy.Classify(control_release) == FcitxTriggerAction::StopNormal);

  fcitx::KeyEvent control_press(nullptr, fcitx::Key(FcitxKey_Control_R), false);
  assert(policy.Classify(control_press) == FcitxTriggerAction::StartNormal);

  fcitx::KeyEvent shift_release(nullptr, fcitx::Key(FcitxKey_Shift_R), true);
  assert(policy.IsSceneMenuTrigger(shift_release));
  assert(policy.Classify(shift_release) == FcitxTriggerAction::ConsumeSceneMenuRelease);
  fcitx::KeyEvent default_scene_press(nullptr, fcitx::Key(FcitxKey_Shift_R), false);
  assert(policy.Classify(default_scene_press) == FcitxTriggerAction::ShowSceneMenu);

  fcitx::KeyEvent command_release(nullptr, fcitx::Key(FcitxKey_F10), true);
  assert(policy.Classify(command_release) == FcitxTriggerAction::StopCommand);
  fcitx::KeyEvent command_press(nullptr, fcitx::Key(FcitxKey_F10), false);
  assert(policy.Classify(command_press) == FcitxTriggerAction::StartCommand);

  fcitx::KeyEvent asr_release(nullptr, fcitx::Key(FcitxKey_F8), true);
  assert(policy.IsAsrMenuTrigger(asr_release));
  assert(policy.Classify(asr_release) == FcitxTriggerAction::ConsumeAsrMenuRelease);
  fcitx::KeyEvent asr_press(nullptr, fcitx::Key(FcitxKey_F8), false);
  assert(policy.Classify(asr_press) == FcitxTriggerAction::ShowAsrMenu);

  const FcitxKeyTriggerPolicy shift_policy{
      {fcitx::Key(FcitxKey_Shift_R)}, {fcitx::Key(FcitxKey_F9)}, {}, {}};
  assert(shift_policy.Classify(shift_release) == FcitxTriggerAction::StopNormal);
  fcitx::KeyEvent shift_press(nullptr, fcitx::Key(FcitxKey_Shift_R), false);
  assert(shift_policy.Classify(shift_press) == FcitxTriggerAction::StartNormal);
  fcitx::KeyEvent custom_command_release(nullptr, fcitx::Key(FcitxKey_F9), true);
  assert(shift_policy.Classify(custom_command_release) ==
         FcitxTriggerAction::StopCommand);
  fcitx::KeyEvent custom_command_press(nullptr, fcitx::Key(FcitxKey_F9), false);
  assert(shift_policy.Classify(custom_command_press) ==
         FcitxTriggerAction::StartCommand);
  const FcitxKeyTriggerPolicy scene_overlap{{fcitx::Key(FcitxKey_F6)},
                                            {fcitx::Key(FcitxKey_F7)},
                                            {fcitx::Key(FcitxKey_F6)},
                                            {}};
  fcitx::KeyEvent overlap_press(nullptr, fcitx::Key(FcitxKey_F6), false);
  assert(scene_overlap.Classify(overlap_press) == FcitxTriggerAction::ShowSceneMenu);

  const FcitxKeyTriggerPolicy asr_overlap{{fcitx::Key(FcitxKey_F6)},
                                          {fcitx::Key(FcitxKey_F6)},
                                          {fcitx::Key(FcitxKey_F6)},
                                          {fcitx::Key(FcitxKey_F6)}};
  assert(asr_overlap.Classify(overlap_press) == FcitxTriggerAction::ShowAsrMenu);

  const FcitxKeyTriggerPolicy multi_policy{
      {fcitx::Key(FcitxKey_F5), fcitx::Key(FcitxKey_F6)},
      {fcitx::Key(FcitxKey_F9)},
      {fcitx::Key(FcitxKey_F7)},
      {fcitx::Key(FcitxKey_F8)},
  };
  fcitx::KeyEvent second_normal_press(nullptr, fcitx::Key(FcitxKey_F6), false);
  assert(multi_policy.Classify(second_normal_press) == FcitxTriggerAction::StartNormal);

  unsetenv("VINPST_FCITX_NORMAL_TRIGGER");
  unsetenv("VINPST_FCITX_COMMAND_TRIGGER");
  unsetenv("VINPST_FCITX_SCENE_MENU_TRIGGER");
  unsetenv("VINPST_FCITX_ASR_MENU_TRIGGER");
  const auto default_env_policy = FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
      {fcitx::Key(FcitxKey_Control_R)}, {fcitx::Key(FcitxKey_F10)},
      {fcitx::Key(FcitxKey_Shift_R)}, {fcitx::Key(FcitxKey_F8)});
  assert(default_env_policy.normal_triggers().front().check(
      fcitx::Key(FcitxKey_Control_R)));
  assert(default_env_policy.command_triggers().front().check(fcitx::Key(FcitxKey_F10)));
  assert(default_env_policy.scene_menu_triggers().front().check(
      fcitx::Key(FcitxKey_Shift_R)));
  assert(default_env_policy.asr_menu_triggers().front().check(fcitx::Key(FcitxKey_F8)));

  setenv("VINPST_FCITX_NORMAL_TRIGGER", "F8", 1);
  setenv("VINPST_FCITX_COMMAND_TRIGGER", "F9", 1);
  setenv("VINPST_FCITX_SCENE_MENU_TRIGGER", "F7", 1);
  setenv("VINPST_FCITX_ASR_MENU_TRIGGER", "F6", 1);
  const auto custom_env_policy = FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
      {fcitx::Key(FcitxKey_Control_R)}, {fcitx::Key(FcitxKey_F10)},
      {fcitx::Key(FcitxKey_Shift_R)}, {fcitx::Key(FcitxKey_F8)});
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

  setenv("VINPST_FCITX_NORMAL_TRIGGER", "not-a-key", 1);
  setenv("VINPST_FCITX_COMMAND_TRIGGER", "", 1);
  setenv("VINPST_FCITX_SCENE_MENU_TRIGGER", "not-a-key", 1);
  setenv("VINPST_FCITX_ASR_MENU_TRIGGER", "not-a-key", 1);
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
  unsetenv("VINPST_FCITX_NORMAL_TRIGGER");
  unsetenv("VINPST_FCITX_COMMAND_TRIGGER");
  unsetenv("VINPST_FCITX_SCENE_MENU_TRIGGER");
  unsetenv("VINPST_FCITX_ASR_MENU_TRIGGER");

  return 0;
}
