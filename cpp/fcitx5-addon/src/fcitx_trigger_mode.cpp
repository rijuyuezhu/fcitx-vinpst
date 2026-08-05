#include "vinpst_fcitx_bridge/fcitx_trigger_mode.h"

#include "vinpst_fcitx_ffi.h"

#include <chrono>

namespace vinpst_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(TriggerMode::Tap) ==
              VINPST_FCITX_TRIGGER_MODE_TAP);
static_assert(static_cast<std::uint8_t>(TriggerMode::Hold) ==
              VINPST_FCITX_TRIGGER_MODE_HOLD);
static_assert(static_cast<std::uint8_t>(TriggerMode::Both) ==
              VINPST_FCITX_TRIGGER_MODE_BOTH);
static_assert(static_cast<std::uint8_t>(TriggerKind::Normal) ==
              VINPST_FCITX_TRIGGER_KIND_NORMAL);
static_assert(static_cast<std::uint8_t>(TriggerKind::Command) ==
              VINPST_FCITX_TRIGGER_KIND_COMMAND);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::None) ==
              VINPST_FCITX_TRIGGER_ACTION_NONE);
static_assert(static_cast<std::uint8_t>(TriggerModeAction::ScheduleStop) ==
              VINPST_FCITX_TRIGGER_ACTION_SCHEDULE_STOP);

TriggerModeAction ActionFromWire(std::uint8_t action) {
  if (action > VINPST_FCITX_TRIGGER_ACTION_SCHEDULE_STOP) {
    return TriggerModeAction::None;
  }
  return static_cast<TriggerModeAction>(action);
}

TriggerModeAction Dispatch(VinpstFcitxTriggerState *state, std::uint8_t kind,
                           std::uint8_t value = 0, bool flag = false,
                           std::int64_t now_ns = 0) {
  if (state == nullptr) {
    return TriggerModeAction::None;
  }
  const VinpstFcitxTriggerEventView event{kind, value, static_cast<std::uint8_t>(flag),
                                          now_ns};
  std::uint8_t action = VINPST_FCITX_TRIGGER_ACTION_NONE;
  if (vinpst_fcitx_trigger_state_dispatch(state, &event, &action) == 0) {
    return TriggerModeAction::None;
  }
  return ActionFromWire(action);
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
    : state_(StateHandle::Adopt(
          vinpst_fcitx_trigger_state_new(static_cast<std::uint8_t>(mode)))) {}

void TriggerModeController::SetMode(TriggerMode mode) {
  static_cast<void>(Dispatch(state_.mutable_raw_handle(),
                             VINPST_FCITX_TRIGGER_EVENT_SET_MODE,
                             static_cast<std::uint8_t>(mode)));
  if (state_) {
    pending_key_.reset();
  }
}

TriggerModeAction TriggerModeController::OnPress(TriggerKind kind,
                                                 const fcitx::Key &key, TimePoint now,
                                                 bool recording) {
  const auto action =
      Dispatch(state_.mutable_raw_handle(), VINPST_FCITX_TRIGGER_EVENT_PRESS,
               static_cast<std::uint8_t>(kind), recording, ToNanoseconds(now));
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
  const bool active_release =
      active_key_.has_value() && IsReleaseOfTrigger(key, *active_key_);
  const auto action =
      Dispatch(state_.mutable_raw_handle(), VINPST_FCITX_TRIGGER_EVENT_RELEASE, 0,
               active_release, ToNanoseconds(now));
  if (action == TriggerModeAction::CancelPendingStart) {
    pending_key_.reset();
  }
  return action;
}

TriggerModeAction TriggerModeController::FirePendingStart() {
  const auto action = Dispatch(state_.mutable_raw_handle(),
                               VINPST_FCITX_TRIGGER_EVENT_FIRE_PENDING_START);
  if (StartsImmediately(action)) {
    active_key_ = pending_key_;
    pending_key_.reset();
  }
  return action;
}

TriggerModeAction TriggerModeController::FirePendingStop() {
  return Dispatch(state_.mutable_raw_handle(),
                  VINPST_FCITX_TRIGGER_EVENT_FIRE_PENDING_STOP);
}

void TriggerModeController::ConfirmStart(bool recording_started) {
  static_cast<void>(Dispatch(state_.mutable_raw_handle(),
                             VINPST_FCITX_TRIGGER_EVENT_CONFIRM_START, 0,
                             recording_started));
  if (!recording_started) {
    pending_key_.reset();
    active_key_.reset();
  }
}

void TriggerModeController::RecordingStopped() {
  static_cast<void>(Dispatch(state_.mutable_raw_handle(),
                             VINPST_FCITX_TRIGGER_EVENT_RECORDING_STOPPED));
  pending_key_.reset();
  active_key_.reset();
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

} // namespace vinpst_fcitx_bridge
