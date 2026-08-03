#include "vinput_fcitx_bridge/fcitx_trigger_mode.h"

#include "vinput_fcitx_ffi.h"

#include <chrono>

namespace vinput_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(TriggerMode::Tap) ==
              VINPUT_FCITX_TRIGGER_MODE_TAP);
static_assert(static_cast<std::uint8_t>(TriggerMode::Hold) ==
              VINPUT_FCITX_TRIGGER_MODE_HOLD);
static_assert(static_cast<std::uint8_t>(TriggerMode::Both) ==
              VINPUT_FCITX_TRIGGER_MODE_BOTH);
static_assert(static_cast<std::uint8_t>(TriggerKind::Normal) ==
              VINPUT_FCITX_TRIGGER_KIND_NORMAL);
static_assert(static_cast<std::uint8_t>(TriggerKind::Command) ==
              VINPUT_FCITX_TRIGGER_KIND_COMMAND);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::None) ==
              VINPUT_FCITX_TRIGGER_ACTION_NONE);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::Consume) ==
              VINPUT_FCITX_TRIGGER_ACTION_CONSUME);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::StartNormal) ==
              VINPUT_FCITX_TRIGGER_ACTION_START_NORMAL);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::StartCommand) ==
              VINPUT_FCITX_TRIGGER_ACTION_START_COMMAND);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::StopActive) ==
              VINPUT_FCITX_TRIGGER_ACTION_STOP_ACTIVE);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::ScheduleNormalStart) ==
              VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_NORMAL_START);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::ScheduleCommandStart) ==
              VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_COMMAND_START);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::CancelPendingStart) ==
              VINPUT_FCITX_TRIGGER_ACTION_CANCEL_PENDING_START);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::ScheduleStop) ==
              VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_STOP);

TriggerModeAction ActionFromWire(std::uint8_t action) {
  switch (action) {
  case VINPUT_FCITX_TRIGGER_ACTION_CONSUME:
    return TriggerModeAction::Consume;
  case VINPUT_FCITX_TRIGGER_ACTION_START_NORMAL:
    return TriggerModeAction::StartNormal;
  case VINPUT_FCITX_TRIGGER_ACTION_START_COMMAND:
    return TriggerModeAction::StartCommand;
  case VINPUT_FCITX_TRIGGER_ACTION_STOP_ACTIVE:
    return TriggerModeAction::StopActive;
  case VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_NORMAL_START:
    return TriggerModeAction::ScheduleNormalStart;
  case VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_COMMAND_START:
    return TriggerModeAction::ScheduleCommandStart;
  case VINPUT_FCITX_TRIGGER_ACTION_CANCEL_PENDING_START:
    return TriggerModeAction::CancelPendingStart;
  case VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_STOP:
    return TriggerModeAction::ScheduleStop;
  case VINPUT_FCITX_TRIGGER_ACTION_NONE:
  default:
    return TriggerModeAction::None;
  }
}

bool StartsImmediately(TriggerModeAction action) {
  return action == TriggerModeAction::StartNormal ||
         action == TriggerModeAction::StartCommand;
}

bool SchedulesStart(TriggerModeAction action) {
  return action == TriggerModeAction::ScheduleNormalStart ||
         action == TriggerModeAction::ScheduleCommandStart;
}

} // namespace

TriggerModeController::TriggerModeController(TriggerMode mode)
    : state_(vinput_fcitx_trigger_state_new(static_cast<std::uint8_t>(mode))) {}

TriggerModeController::~TriggerModeController() {
  vinput_fcitx_trigger_state_free(state_);
}

void TriggerModeController::SetMode(TriggerMode mode) {
  if (state_ != nullptr && vinput_fcitx_trigger_state_set_mode(
                               state_, static_cast<std::uint8_t>(mode)) != 0) {
    pending_key_.reset();
  }
}

TriggerMode TriggerModeController::mode() const {
  if (state_ == nullptr) {
    return TriggerMode::Both;
  }
  switch (vinput_fcitx_trigger_state_mode(state_)) {
  case VINPUT_FCITX_TRIGGER_MODE_TAP:
    return TriggerMode::Tap;
  case VINPUT_FCITX_TRIGGER_MODE_HOLD:
    return TriggerMode::Hold;
  case VINPUT_FCITX_TRIGGER_MODE_BOTH:
  default:
    return TriggerMode::Both;
  }
}

TriggerModeAction TriggerModeController::OnPress(TriggerKind kind,
                                                 const fcitx::Key &key, TimePoint now,
                                                 bool recording) {
  if (state_ == nullptr) {
    return TriggerModeAction::None;
  }

  const auto action = ActionFromWire(
      vinput_fcitx_trigger_state_on_press(state_, static_cast<std::uint8_t>(kind),
                                          ToNanoseconds(now), recording ? 1U : 0U));
  if (SchedulesStart(action)) {
    pending_key_ = key;
  } else if (StartsImmediately(action)) {
    active_key_ = key;
    pending_key_.reset();
  }
  return action;
}

TriggerModeAction TriggerModeController::OnRelease(TriggerKind, const fcitx::Key &key,
                                                   TimePoint now) {
  if (state_ == nullptr) {
    return TriggerModeAction::None;
  }

  const bool active_release =
      active_key_.has_value() && IsReleaseOfTrigger(key, *active_key_);
  const auto action = ActionFromWire(vinput_fcitx_trigger_state_on_release(
      state_, ToNanoseconds(now), active_release ? 1U : 0U));
  if (action == TriggerModeAction::CancelPendingStart) {
    pending_key_.reset();
  }
  return action;
}

TriggerModeAction TriggerModeController::FirePendingStart() {
  if (state_ == nullptr) {
    return TriggerModeAction::None;
  }

  const auto action =
      ActionFromWire(vinput_fcitx_trigger_state_fire_pending_start(state_));
  if (StartsImmediately(action)) {
    active_key_ = pending_key_;
    pending_key_.reset();
  }
  return action;
}

TriggerModeAction TriggerModeController::FirePendingStop() {
  if (state_ == nullptr) {
    return TriggerModeAction::None;
  }
  return ActionFromWire(vinput_fcitx_trigger_state_fire_pending_stop(state_));
}

void TriggerModeController::ConfirmStart(bool recording_started) {
  if (state_ != nullptr) {
    static_cast<void>(
        vinput_fcitx_trigger_state_confirm_start(state_, recording_started ? 1U : 0U));
  }
  if (!recording_started) {
    pending_key_.reset();
    active_key_.reset();
  }
}

void TriggerModeController::RecordingStopped() {
  if (state_ != nullptr) {
    static_cast<void>(vinput_fcitx_trigger_state_recording_stopped(state_));
  }
  pending_key_.reset();
  active_key_.reset();
}

bool TriggerModeController::has_pending_start() const {
  return state_ != nullptr && vinput_fcitx_trigger_state_has_pending_start(state_) != 0;
}

bool TriggerModeController::has_active_trigger() const {
  return state_ != nullptr &&
         vinput_fcitx_trigger_state_has_active_trigger(state_) != 0;
}

std::int64_t TriggerModeController::ToNanoseconds(TimePoint now) {
  return std::chrono::duration_cast<std::chrono::nanoseconds>(now.time_since_epoch())
      .count();
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
