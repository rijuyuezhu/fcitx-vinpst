#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

#include <cstdlib>

namespace {

fcitx::Key KeyFromEnvironment(const char *name, const fcitx::Key &fallback) {
  const auto *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    return fallback;
  }
  fcitx::Key key(value);
  return key.isValid() ? key : fallback;
}

} // namespace

namespace vinput_fcitx_bridge {

FcitxKeyTriggerPolicy::FcitxKeyTriggerPolicy(fcitx::Key normal_trigger,
                                             fcitx::Key command_trigger,
                                             fcitx::Key scene_menu_trigger,
                                             fcitx::Key asr_menu_trigger)
    : normal_trigger_(normal_trigger), command_trigger_(command_trigger),
      scene_menu_trigger_(scene_menu_trigger), asr_menu_trigger_(asr_menu_trigger) {}

FcitxKeyTriggerPolicy FcitxKeyTriggerPolicy::FromEnvironment() {
  return FcitxKeyTriggerPolicy(
      KeyFromEnvironment("VINPUT_FCITX_NORMAL_TRIGGER", fcitx::Key(FcitxKey_Control_R)),
      KeyFromEnvironment("VINPUT_FCITX_COMMAND_TRIGGER", fcitx::Key(FcitxKey_F10)),
      KeyFromEnvironment("VINPUT_FCITX_SCENE_MENU_TRIGGER",
                         fcitx::Key(FcitxKey_Shift_R)),
      KeyFromEnvironment("VINPUT_FCITX_ASR_MENU_TRIGGER", fcitx::Key(FcitxKey_F8)));
}

FcitxTriggerAction FcitxKeyTriggerPolicy::Classify(const fcitx::KeyEvent &event) const {
  if (event.key().check(normal_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::StopNormal
                             : FcitxTriggerAction::StartNormal;
  }
  if (event.key().check(command_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::StopCommand
                             : FcitxTriggerAction::StartCommand;
  }
  if (event.key().check(scene_menu_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::ConsumeSceneMenuRelease
                             : FcitxTriggerAction::ShowSceneMenu;
  }
  if (event.key().check(asr_menu_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::ConsumeAsrMenuRelease
                             : FcitxTriggerAction::ShowAsrMenu;
  }
  return FcitxTriggerAction::None;
}

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

bool FcitxKeyTriggerPolicy::IsCommandTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(command_trigger_);
}

bool FcitxKeyTriggerPolicy::IsSceneMenuTrigger(const fcitx::KeyEvent &event) const {
  return event.key().check(scene_menu_trigger_);
}

bool FcitxKeyTriggerPolicy::IsAsrMenuTrigger(const fcitx::KeyEvent &event) const {
  return event.key().check(asr_menu_trigger_);
}

} // namespace vinput_fcitx_bridge
