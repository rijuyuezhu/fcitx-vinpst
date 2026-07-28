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
};

class FcitxKeyTriggerPolicy {
public:
  explicit FcitxKeyTriggerPolicy(
      fcitx::Key normal_trigger = fcitx::Key(FcitxKey_Control_R),
      fcitx::Key command_trigger = fcitx::Key(FcitxKey_F10),
      fcitx::Key scene_menu_trigger = fcitx::Key(FcitxKey_Shift_R));
  static FcitxKeyTriggerPolicy FromEnvironment();

  const fcitx::Key &normal_trigger() const {
    return normal_trigger_;
  }
  const fcitx::Key &command_trigger() const {
    return command_trigger_;
  }
  const fcitx::Key &scene_menu_trigger() const {
    return scene_menu_trigger_;
  }

  FcitxTriggerAction Classify(const fcitx::KeyEvent &event) const;
  bool IsNormalTrigger(const fcitx::KeyEvent &event) const;
  bool IsCommandTrigger(const fcitx::KeyEvent &event) const;
  bool IsSceneMenuTrigger(const fcitx::KeyEvent &event) const;

private:
  fcitx::Key normal_trigger_;
  fcitx::Key command_trigger_;
  fcitx::Key scene_menu_trigger_;
};

} // namespace vinput_fcitx_bridge
