#pragma once

#include "vinput_fcitx_bridge/fcitx_config.h"

#include <chrono>
#include <cstdint>
#include <optional>

#include <fcitx-utils/key.h>

namespace vinput_fcitx_bridge {

inline constexpr auto kTriggerDebounce = std::chrono::milliseconds(80);
inline constexpr auto kTriggerHoldThreshold = std::chrono::milliseconds(300);
inline constexpr auto kTriggerReleaseTail = std::chrono::milliseconds(500);

enum class TriggerKind : std::uint8_t {
  Normal,
  Command,
};

enum class TriggerModeAction : std::uint8_t {
  None,
  Consume,
  StartNormal,
  StartCommand,
  StopActive,
  ScheduleNormalStart,
  ScheduleCommandStart,
  CancelPendingStart,
  ScheduleStop,
};

class TriggerModeController {
public:
  using Clock = std::chrono::steady_clock;
  using TimePoint = Clock::time_point;

  explicit TriggerModeController(TriggerMode mode = TriggerMode::Both);

  void SetMode(TriggerMode mode);
  TriggerMode mode() const {
    return mode_;
  }

  TriggerModeAction OnPress(TriggerKind kind, const fcitx::Key &key, TimePoint now,
                            bool recording);
  TriggerModeAction OnRelease(TriggerKind kind, const fcitx::Key &key, TimePoint now);
  TriggerModeAction FirePendingStart();
  TriggerModeAction FirePendingStop();
  void ConfirmStart(bool recording_started);
  void RecordingStopped();

  bool has_pending_start() const {
    return pending_start_.has_value();
  }
  bool has_active_trigger() const {
    return active_trigger_.has_value();
  }

private:
  struct TriggerPress {
    TriggerKind kind;
    fcitx::Key key;
    TimePoint pressed_at;
    bool released = false;
  };

  static TriggerModeAction StartAction(TriggerKind kind);
  static TriggerModeAction ScheduleStartAction(TriggerKind kind);
  static bool IsReleaseOfTrigger(const fcitx::Key &release, const fcitx::Key &trigger);

  TriggerMode mode_;
  std::optional<TimePoint> last_press_time_;
  std::optional<TriggerPress> pending_start_;
  std::optional<TriggerPress> active_trigger_;
  bool stop_pending_ = false;
};

} // namespace vinput_fcitx_bridge
