#pragma once

#include "vinput_fcitx_bridge/fcitx_config.h"

#include <chrono>
#include <cstdint>
#include <optional>

#include <fcitx-utils/key.h>

struct VinputFcitxTriggerState;

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
  ~TriggerModeController();

  TriggerModeController(const TriggerModeController &) = delete;
  TriggerModeController &operator=(const TriggerModeController &) = delete;
  TriggerModeController(TriggerModeController &&) = delete;
  TriggerModeController &operator=(TriggerModeController &&) = delete;

  void SetMode(TriggerMode mode);

  TriggerModeAction OnPress(TriggerKind kind, const fcitx::Key &key, TimePoint now,
                            bool recording);
  TriggerModeAction OnRelease(TriggerKind kind, const fcitx::Key &key, TimePoint now);
  TriggerModeAction FirePendingStart();
  TriggerModeAction FirePendingStop();
  void ConfirmStart(bool recording_started);
  void RecordingStopped();

private:
  static bool IsReleaseOfTrigger(const fcitx::Key &release, const fcitx::Key &trigger);
  static std::int64_t ToNanoseconds(TimePoint now);

  ::VinputFcitxTriggerState *state_ = nullptr;
  std::optional<fcitx::Key> pending_key_;
  std::optional<fcitx::Key> active_key_;
};

} // namespace vinput_fcitx_bridge
