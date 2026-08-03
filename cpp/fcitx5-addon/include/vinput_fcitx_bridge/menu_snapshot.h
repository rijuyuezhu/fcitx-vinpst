#pragma once

struct VinputFcitxAsrDisplaySnapshot;
struct VinputFcitxSceneSnapshot;

namespace vinput_fcitx_bridge {

class SdBusDaemonClient;

class SceneStateSnapshot {
public:
  SceneStateSnapshot() = default;
  static SceneStateSnapshot Adopt(::VinputFcitxSceneSnapshot *snapshot);
  ~SceneStateSnapshot();

  SceneStateSnapshot(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot &operator=(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot(SceneStateSnapshot &&other) noexcept;
  SceneStateSnapshot &operator=(SceneStateSnapshot &&other) noexcept;

  const ::VinputFcitxSceneSnapshot *raw_handle() const;

private:
  friend class SdBusDaemonClient;
  explicit SceneStateSnapshot(::VinputFcitxSceneSnapshot *snapshot);
  ::VinputFcitxSceneSnapshot *mutable_raw_handle();
  ::VinputFcitxSceneSnapshot *snapshot_ = nullptr;
};

class AsrDisplayMenuStateSnapshot {
public:
  AsrDisplayMenuStateSnapshot() = default;
  static AsrDisplayMenuStateSnapshot Adopt(::VinputFcitxAsrDisplaySnapshot *snapshot);
  ~AsrDisplayMenuStateSnapshot();

  AsrDisplayMenuStateSnapshot(const AsrDisplayMenuStateSnapshot &) = delete;
  AsrDisplayMenuStateSnapshot &operator=(const AsrDisplayMenuStateSnapshot &) = delete;
  AsrDisplayMenuStateSnapshot(AsrDisplayMenuStateSnapshot &&other) noexcept;
  AsrDisplayMenuStateSnapshot &operator=(AsrDisplayMenuStateSnapshot &&other) noexcept;

  const ::VinputFcitxAsrDisplaySnapshot *raw_handle() const;

private:
  explicit AsrDisplayMenuStateSnapshot(::VinputFcitxAsrDisplaySnapshot *snapshot);
  ::VinputFcitxAsrDisplaySnapshot *snapshot_ = nullptr;
};

} // namespace vinput_fcitx_bridge
