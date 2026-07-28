#pragma once

#include <cstdint>

#include <fcitx-utils/key.h>
#include <fcitx/event.h>

namespace vinput_fcitx_bridge {

enum class FcitxTriggerAction : std::uint8_t {
  None,
  StartNormal,
  StopNormal,
  StartCommand,
  StopCommand,
  ShowSceneMenu,
  ConsumeSceneMenuRelease,
  ShowAsrMenu,
  ConsumeAsrMenuRelease,
};

class FcitxKeyTriggerPolicy {
public:
  explicit FcitxKeyTriggerPolicy(
      fcitx::KeyList normal_triggers = {fcitx::Key(FcitxKey_Control_R)},
      fcitx::KeyList command_triggers = {fcitx::Key(FcitxKey_F10)},
      fcitx::KeyList scene_menu_triggers = {fcitx::Key(FcitxKey_Shift_R)},
      fcitx::KeyList asr_menu_triggers = {fcitx::Key(FcitxKey_F8)});
  static FcitxKeyTriggerPolicy FromEnvironment();
  static FcitxKeyTriggerPolicy WithEnvironmentOverrides(
      fcitx::KeyList normal_triggers, fcitx::KeyList command_triggers,
      fcitx::KeyList scene_menu_triggers, fcitx::KeyList asr_menu_triggers);

  const fcitx::KeyList &normal_triggers() const {
    return normal_triggers_;
  }
  const fcitx::KeyList &command_triggers() const {
    return command_triggers_;
  }
  const fcitx::KeyList &scene_menu_triggers() const {
    return scene_menu_triggers_;
  }
  const fcitx::KeyList &asr_menu_triggers() const {
    return asr_menu_triggers_;
  }

  FcitxTriggerAction Classify(const fcitx::KeyEvent &event) const;
  bool IsNormalTrigger(const fcitx::KeyEvent &event) const;
  bool IsCommandTrigger(const fcitx::KeyEvent &event) const;
  bool IsSceneMenuTrigger(const fcitx::KeyEvent &event) const;
  bool IsAsrMenuTrigger(const fcitx::KeyEvent &event) const;

private:
  fcitx::KeyList normal_triggers_;
  fcitx::KeyList command_triggers_;
  fcitx::KeyList scene_menu_triggers_;
  fcitx::KeyList asr_menu_triggers_;
};

} // namespace vinput_fcitx_bridge
