#include "vinpst_fcitx_bridge/fcitx_trigger_mode.h"

#include <cassert>
#include <chrono>

int main() {
  using namespace std::chrono_literals;
  using vinpst_fcitx_bridge::TriggerEventTimeMapper;
  using vinpst_fcitx_bridge::TriggerKind;
  using vinpst_fcitx_bridge::TriggerMode;
  using vinpst_fcitx_bridge::TriggerModeAction;
  using vinpst_fcitx_bridge::TriggerModeController;

  const auto base = TriggerModeController::TimePoint{};
  const fcitx::Key normal(FcitxKey_Control_R);
  const fcitx::Key command(FcitxKey_F10);

  TriggerModeController hold(TriggerMode::Hold);
  assert(hold.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::ScheduleNormalStart);
  assert(hold.OnRelease(TriggerKind::Normal, normal, base + 100ms) ==
         TriggerModeAction::CancelPendingStart);
  assert(hold.FirePendingStart() == TriggerModeAction::None);

  assert(hold.OnPress(TriggerKind::Command, command, base + 1s, false) ==
         TriggerModeAction::ScheduleCommandStart);
  assert(hold.FirePendingStart() == TriggerModeAction::StartCommand);
  hold.ConfirmStart(true);
  assert(hold.OnRelease(TriggerKind::Command, command, base + 1400ms) ==
         TriggerModeAction::ScheduleStop);
  assert(hold.FirePendingStop() == TriggerModeAction::StopActive);
  hold.RecordingStopped();

  TriggerModeController tap(TriggerMode::Tap);
  assert(tap.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  tap.ConfirmStart(true);
  assert(tap.OnRelease(TriggerKind::Normal, normal, base + 50ms) ==
         TriggerModeAction::Consume);
  assert(tap.OnPress(TriggerKind::Command, command, base + 200ms, true) ==
         TriggerModeAction::StopActive);
  tap.RecordingStopped();

  TriggerModeController both(TriggerMode::Both);
  assert(both.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  both.ConfirmStart(true);
  assert(both.OnRelease(TriggerKind::Normal, normal, base + 100ms) ==
         TriggerModeAction::Consume);
  assert(both.FirePendingStop() == TriggerModeAction::None);
  assert(both.OnPress(TriggerKind::Normal, normal, base + 200ms, true) ==
         TriggerModeAction::StopActive);
  both.RecordingStopped();

  assert(both.OnPress(TriggerKind::Normal, normal, base + 1s, false) ==
         TriggerModeAction::StartNormal);
  both.ConfirmStart(true);
  assert(both.OnRelease(TriggerKind::Normal, normal, base + 1400ms) ==
         TriggerModeAction::ScheduleStop);
  assert(both.FirePendingStop() == TriggerModeAction::StopActive);
  both.RecordingStopped();

  TriggerModeController result_during_release_tail(TriggerMode::Both);
  assert(result_during_release_tail.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  result_during_release_tail.ConfirmStart(true);
  assert(result_during_release_tail.OnRelease(
             TriggerKind::Normal, normal, base + 50ms) == TriggerModeAction::Consume);
  assert(result_during_release_tail.OnPress(TriggerKind::Normal, normal, base + 150ms,
                                            true) == TriggerModeAction::StopActive);
  assert(result_during_release_tail.OnRelease(
             TriggerKind::Normal, normal, base + 200ms) == TriggerModeAction::Consume);
  result_during_release_tail.RecordingStopped();
  assert(result_during_release_tail.FirePendingStop() == TriggerModeAction::None);
  assert(result_during_release_tail.OnPress(TriggerKind::Normal, normal, base + 240ms,
                                            false) == TriggerModeAction::StartNormal);
  result_during_release_tail.ConfirmStart(false);

  TriggerModeController debounce(TriggerMode::Tap);
  assert(debounce.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  debounce.ConfirmStart(false);
  assert(debounce.OnPress(TriggerKind::Normal, normal, base + 50ms, false) ==
         TriggerModeAction::Consume);
  assert(debounce.OnPress(TriggerKind::Normal, normal, base + 100ms, false) ==
         TriggerModeAction::StartNormal);
  debounce.ConfirmStart(false);

  TriggerModeController failed(TriggerMode::Both);
  assert(failed.OnPress(TriggerKind::Command, command, base, false) ==
         TriggerModeAction::StartCommand);
  failed.ConfirmStart(false);
  assert(failed.OnRelease(TriggerKind::Command, command, base + 400ms) ==
         TriggerModeAction::Consume);

  TriggerModeController modifier_release(TriggerMode::Both);
  assert(modifier_release.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  modifier_release.ConfirmStart(true);
  assert(modifier_release.OnRelease(TriggerKind::Normal, fcitx::Key(FcitxKey_Control_R),
                                    base + 400ms) == TriggerModeAction::ScheduleStop);

  TriggerEventTimeMapper event_time_mapper;
  const auto delayed_press_observed_at = base + 10s;
  const auto physical_press_at =
      event_time_mapper.Resolve(1000, delayed_press_observed_at);
  const auto physical_release_at =
      event_time_mapper.Resolve(1100, delayed_press_observed_at + 500ms);
  assert(physical_release_at - physical_press_at == 100ms);

  TriggerModeController delayed_tap(TriggerMode::Both);
  assert(delayed_tap.OnPress(TriggerKind::Normal, normal, physical_press_at, false) ==
         TriggerModeAction::StartNormal);
  delayed_tap.ConfirmStart(true);
  assert(delayed_tap.OnRelease(TriggerKind::Normal, normal, physical_release_at) ==
         TriggerModeAction::Consume);

  assert(event_time_mapper.Resolve(0, base + 20s) == base + 20s);
  const auto reanchored_after_zero =
      event_time_mapper.Resolve(1200, base + 20s + 100ms);
  assert(reanchored_after_zero == base + 20s + 100ms);
  assert(event_time_mapper.Resolve(1250, base + 20s + 200ms) - reanchored_after_zero ==
         50ms);

  TriggerEventTimeMapper wrapping_event_time_mapper;
  const auto before_wrap = wrapping_event_time_mapper.Resolve(-16, base + 30s);
  const auto after_wrap = wrapping_event_time_mapper.Resolve(16, base + 30s + 32ms);
  assert(after_wrap - before_wrap == 32ms);

  return 0;
}
