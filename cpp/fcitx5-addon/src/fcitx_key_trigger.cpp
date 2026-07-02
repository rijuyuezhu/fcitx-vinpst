#include "vinput_fcitx_bridge/fcitx_key_trigger.h"

namespace vinput_fcitx_bridge {

FcitxKeyTriggerPolicy::FcitxKeyTriggerPolicy(fcitx::Key normal_trigger,
                                             fcitx::Key command_trigger)
    : normal_trigger_(normal_trigger), command_trigger_(command_trigger) {}

FcitxTriggerAction FcitxKeyTriggerPolicy::Classify(const fcitx::KeyEvent &event) const {
  if (event.key().check(normal_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::StopNormal
                             : FcitxTriggerAction::StartNormal;
  }
  if (event.key().check(command_trigger_)) {
    return event.isRelease() ? FcitxTriggerAction::StopCommand
                             : FcitxTriggerAction::StartCommand;
  }
  return FcitxTriggerAction::None;
}

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

bool FcitxKeyTriggerPolicy::IsCommandTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(command_trigger_);
}

} // namespace vinput_fcitx_bridge
