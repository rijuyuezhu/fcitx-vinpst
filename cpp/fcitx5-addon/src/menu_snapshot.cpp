#include "vinput_fcitx_bridge/menu_snapshot.h"

#include "vinput_fcitx_ffi.h"

#include <utility>

namespace vinput_fcitx_bridge {

SceneStateSnapshot::SceneStateSnapshot(VinputFcitxSceneSnapshot *snapshot)
    : snapshot_(snapshot) {}

SceneStateSnapshot SceneStateSnapshot::Adopt(VinputFcitxSceneSnapshot *snapshot) {
  return SceneStateSnapshot(snapshot);
}

SceneStateSnapshot::~SceneStateSnapshot() {
  vinput_fcitx_scene_snapshot_free(snapshot_);
}

SceneStateSnapshot::SceneStateSnapshot(SceneStateSnapshot &&other) noexcept
    : snapshot_(std::exchange(other.snapshot_, nullptr)) {}

SceneStateSnapshot &SceneStateSnapshot::operator=(SceneStateSnapshot &&other) noexcept {
  if (this != &other) {
    vinput_fcitx_scene_snapshot_free(snapshot_);
    snapshot_ = std::exchange(other.snapshot_, nullptr);
  }
  return *this;
}

const ::VinputFcitxSceneSnapshot *SceneStateSnapshot::raw_handle() const {
  return snapshot_;
}

::VinputFcitxSceneSnapshot *SceneStateSnapshot::mutable_raw_handle() {
  return snapshot_;
}

AsrDisplayMenuStateSnapshot::AsrDisplayMenuStateSnapshot(
    VinputFcitxAsrDisplaySnapshot *snapshot)
    : snapshot_(snapshot) {}

AsrDisplayMenuStateSnapshot
AsrDisplayMenuStateSnapshot::Adopt(VinputFcitxAsrDisplaySnapshot *snapshot) {
  return AsrDisplayMenuStateSnapshot(snapshot);
}

AsrDisplayMenuStateSnapshot::~AsrDisplayMenuStateSnapshot() {
  vinput_fcitx_asr_display_snapshot_free(snapshot_);
}

AsrDisplayMenuStateSnapshot::AsrDisplayMenuStateSnapshot(
    AsrDisplayMenuStateSnapshot &&other) noexcept
    : snapshot_(std::exchange(other.snapshot_, nullptr)) {}

AsrDisplayMenuStateSnapshot &
AsrDisplayMenuStateSnapshot::operator=(AsrDisplayMenuStateSnapshot &&other) noexcept {
  if (this != &other) {
    vinput_fcitx_asr_display_snapshot_free(snapshot_);
    snapshot_ = std::exchange(other.snapshot_, nullptr);
  }
  return *this;
}

const ::VinputFcitxAsrDisplaySnapshot *AsrDisplayMenuStateSnapshot::raw_handle() const {
  return snapshot_;
}

} // namespace vinput_fcitx_bridge
