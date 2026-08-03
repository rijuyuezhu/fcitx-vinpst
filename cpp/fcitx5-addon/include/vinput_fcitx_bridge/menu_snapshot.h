#pragma once

#include <optional>
#include <string>
#include <string_view>

struct VinputFcitxAsrDisplaySnapshot;
struct VinputFcitxSceneSnapshot;

namespace vinput_fcitx_bridge {

class SceneStateSnapshot {
public:
  SceneStateSnapshot() = default;
  static SceneStateSnapshot Adopt(::VinputFcitxSceneSnapshot *snapshot);
  ~SceneStateSnapshot();

  SceneStateSnapshot(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot &operator=(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot(SceneStateSnapshot &&other) noexcept;
  SceneStateSnapshot &operator=(SceneStateSnapshot &&other) noexcept;

  bool SetActive(std::string_view active_scene_id);
  std::optional<std::string> active_scene_id() const;
  const ::VinputFcitxSceneSnapshot *raw_handle() const;

private:
  explicit SceneStateSnapshot(::VinputFcitxSceneSnapshot *snapshot);
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
