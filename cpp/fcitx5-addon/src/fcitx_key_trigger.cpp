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
                                             fcitx::Key command_trigger)
    : normal_trigger_(normal_trigger), command_trigger_(command_trigger) {}

FcitxKeyTriggerPolicy FcitxKeyTriggerPolicy::FromEnvironment() {
  return FcitxKeyTriggerPolicy(
      KeyFromEnvironment("VINPUT_FCITX_NORMAL_TRIGGER", fcitx::Key(FcitxKey_Control_R)),
      KeyFromEnvironment("VINPUT_FCITX_COMMAND_TRIGGER", fcitx::Key(FcitxKey_F10)));
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
  return FcitxTriggerAction::None;
}

bool FcitxKeyTriggerPolicy::IsNormalTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(normal_trigger_);
}

bool FcitxKeyTriggerPolicy::IsCommandTrigger(const fcitx::KeyEvent &event) const {
  return event.isRelease() && event.key().check(command_trigger_);
}

} // namespace vinput_fcitx_bridge
