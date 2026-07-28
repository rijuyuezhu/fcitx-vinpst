#include "vinput_fcitx_bridge/fcitx_trigger_mode.h"

namespace vinput_fcitx_bridge {

TriggerModeController::TriggerModeController(TriggerMode mode) : mode_(mode) {}

void TriggerModeController::SetMode(TriggerMode mode) {
  mode_ = mode;
  pending_start_.reset();
  last_press_time_.reset();
}

TriggerModeAction TriggerModeController::OnPress(TriggerKind kind,
                                                 const fcitx::Key &key, TimePoint now,
                                                 bool recording) {
  if (last_press_time_.has_value() && now - *last_press_time_ < kTriggerDebounce) {
    return TriggerModeAction::Consume;
  }
  last_press_time_ = now;
  stop_pending_ = false;

  if (active_trigger_.has_value()) {
    return active_trigger_->released ? TriggerModeAction::StopActive
                                     : TriggerModeAction::Consume;
  }
  if (recording) {
    return TriggerModeAction::StopActive;
  }
  if (pending_start_.has_value()) {
    return TriggerModeAction::Consume;
  }

  TriggerPress press{kind, key, now, false};
  if (mode_ == TriggerMode::Hold) {
    pending_start_ = press;
    return ScheduleStartAction(kind);
  }
  active_trigger_ = press;
  return StartAction(kind);
}

TriggerModeAction TriggerModeController::OnRelease(TriggerKind, const fcitx::Key &key,
                                                   TimePoint now) {
  if (mode_ == TriggerMode::Hold && pending_start_.has_value()) {
    pending_start_.reset();
    return TriggerModeAction::CancelPendingStart;
  }
  if (!active_trigger_.has_value()) {
    return TriggerModeAction::Consume;
  }

  auto &active = *active_trigger_;
  const bool active_release = IsReleaseOfTrigger(key, active.key);
  if (mode_ == TriggerMode::Tap) {
    active.released = true;
    return TriggerModeAction::Consume;
  }

  if (active_release) {
    active.released = true;
    if (now - active.pressed_at >= kTriggerHoldThreshold) {
      stop_pending_ = true;
      return TriggerModeAction::ScheduleStop;
    }
    return TriggerModeAction::Consume;
  }

  if (mode_ == TriggerMode::Both) {
    active.released = true;
  }
  return TriggerModeAction::Consume;
}

TriggerModeAction TriggerModeController::FirePendingStart() {
  if (!pending_start_.has_value()) {
    return TriggerModeAction::None;
  }
  const auto kind = pending_start_->kind;
  active_trigger_ = pending_start_;
  pending_start_.reset();
  return StartAction(kind);
}

TriggerModeAction TriggerModeController::FirePendingStop() {
  if (!stop_pending_ || !active_trigger_.has_value() || !active_trigger_->released) {
    return TriggerModeAction::None;
  }
  stop_pending_ = false;
  return TriggerModeAction::StopActive;
}

void TriggerModeController::ConfirmStart(bool recording_started) {
  if (!recording_started) {
    pending_start_.reset();
    active_trigger_.reset();
  }
}

void TriggerModeController::RecordingStopped() {
  pending_start_.reset();
  active_trigger_.reset();
  stop_pending_ = false;
}

TriggerModeAction TriggerModeController::StartAction(TriggerKind kind) {
  return kind == TriggerKind::Normal ? TriggerModeAction::StartNormal
                                     : TriggerModeAction::StartCommand;
}

TriggerModeAction TriggerModeController::ScheduleStartAction(TriggerKind kind) {
  return kind == TriggerKind::Normal ? TriggerModeAction::ScheduleNormalStart
                                     : TriggerModeAction::ScheduleCommandStart;
}

bool TriggerModeController::IsReleaseOfTrigger(const fcitx::Key &release,
                                               const fcitx::Key &trigger) {
  const auto release_key = release.normalize();
  const auto trigger_key = trigger.normalize();

  if (trigger_key.isModifier() && release_key.isReleaseOfModifier(trigger_key)) {
    return true;
  }
  if (release_key.sym() == trigger_key.sym()) {
    if (trigger_key.states().toInteger() == 0) {
      return true;
    }
    return release_key.states().testAny(trigger_key.states()) &&
           (release_key.states() & trigger_key.states()) == trigger_key.states();
  }
  const auto released_modifier_state = fcitx::Key::keySymToStates(release_key.sym());
  return released_modifier_state.toInteger() != 0 &&
         trigger_key.states().testAny(released_modifier_state);
}

} // namespace vinput_fcitx_bridge
