#include "vinput_fcitx_bridge/fcitx_trigger_mode.h"

#include <cassert>
#include <chrono>

int main() {
  using namespace std::chrono_literals;
  using vinput_fcitx_bridge::TriggerKind;
  using vinput_fcitx_bridge::TriggerMode;
  using vinput_fcitx_bridge::TriggerModeAction;
  using vinput_fcitx_bridge::TriggerModeController;

  const auto base = TriggerModeController::TimePoint{};
  const fcitx::Key normal(FcitxKey_Control_R);
  const fcitx::Key command(FcitxKey_F10);

  TriggerModeController hold(TriggerMode::Hold);
  assert(hold.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::ScheduleNormalStart);
  assert(hold.has_pending_start());
  assert(hold.OnRelease(TriggerKind::Normal, normal, base + 100ms) ==
         TriggerModeAction::CancelPendingStart);
  assert(!hold.has_pending_start());
  assert(hold.FirePendingStart() == TriggerModeAction::None);

  assert(hold.OnPress(TriggerKind::Command, command, base + 1s, false) ==
         TriggerModeAction::ScheduleCommandStart);
  assert(hold.FirePendingStart() == TriggerModeAction::StartCommand);
  hold.ConfirmStart(true);
  assert(hold.has_active_trigger());
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
  assert(!failed.has_active_trigger());

  TriggerModeController modifier_release(TriggerMode::Both);
  assert(modifier_release.OnPress(TriggerKind::Normal, normal, base, false) ==
         TriggerModeAction::StartNormal);
  modifier_release.ConfirmStart(true);
  assert(modifier_release.OnRelease(TriggerKind::Normal, fcitx::Key(FcitxKey_Control_R),
                                    base + 400ms) == TriggerModeAction::ScheduleStop);

  return 0;
}
