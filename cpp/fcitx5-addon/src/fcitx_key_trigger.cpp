#include "vinpst_fcitx_bridge/fcitx_key_trigger.h"

#include <cstdlib>

namespace {

fcitx::KeyList KeyListFromEnvironment(const char *name, fcitx::KeyList fallback) {
  const auto *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    return fallback;
  }
  fcitx::Key key(value);
  return key.isValid() ? fcitx::KeyList{key} : std::move(fallback);
}

} // namespace

namespace vinpst_fcitx_bridge {

FcitxKeyTriggerPolicy::FcitxKeyTriggerPolicy(fcitx::KeyList normal_triggers,
                                             fcitx::KeyList command_triggers,
                                             fcitx::KeyList scene_menu_triggers,
                                             fcitx::KeyList asr_menu_triggers)
    : normal_triggers_(std::move(normal_triggers)),
      command_triggers_(std::move(command_triggers)),
      scene_menu_triggers_(std::move(scene_menu_triggers)),
      asr_menu_triggers_(std::move(asr_menu_triggers)) {}

FcitxKeyTriggerPolicy FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
    fcitx::KeyList normal_triggers, fcitx::KeyList command_triggers,
    fcitx::KeyList scene_menu_triggers, fcitx::KeyList asr_menu_triggers) {
  return FcitxKeyTriggerPolicy(
      KeyListFromEnvironment("VINPST_FCITX_NORMAL_TRIGGER", std::move(normal_triggers)),
      KeyListFromEnvironment("VINPST_FCITX_COMMAND_TRIGGER",
                             std::move(command_triggers)),
      KeyListFromEnvironment("VINPST_FCITX_SCENE_MENU_TRIGGER",
                             std::move(scene_menu_triggers)),
      KeyListFromEnvironment("VINPST_FCITX_ASR_MENU_TRIGGER",
                             std::move(asr_menu_triggers)));
}

FcitxTriggerAction FcitxKeyTriggerPolicy::Classify(const fcitx::KeyEvent &event) const {
  if (event.key().checkKeyList(normal_triggers_)) {
    return event.isRelease() ? FcitxTriggerAction::StopNormal
                             : FcitxTriggerAction::StartNormal;
  }
  if (event.key().checkKeyList(command_triggers_)) {
    return event.isRelease() ? FcitxTriggerAction::StopCommand
                             : FcitxTriggerAction::StartCommand;
  }
  if (event.key().checkKeyList(scene_menu_triggers_)) {
    return event.isRelease() ? FcitxTriggerAction::ConsumeSceneMenuRelease
                             : FcitxTriggerAction::ShowSceneMenu;
  }
  if (event.key().checkKeyList(asr_menu_triggers_)) {
    return event.isRelease() ? FcitxTriggerAction::ConsumeAsrMenuRelease
                             : FcitxTriggerAction::ShowAsrMenu;
  }
  return FcitxTriggerAction::None;
}

bool FcitxKeyTriggerPolicy::IsSceneMenuTrigger(const fcitx::KeyEvent &event) const {
  return event.key().checkKeyList(scene_menu_triggers_);
}

bool FcitxKeyTriggerPolicy::IsAsrMenuTrigger(const fcitx::KeyEvent &event) const {
  return event.key().checkKeyList(asr_menu_triggers_);
}

} // namespace vinpst_fcitx_bridge
