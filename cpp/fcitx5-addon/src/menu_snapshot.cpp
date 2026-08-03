#include "vinput_fcitx_bridge/menu_snapshot.h"

#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

const std::uint8_t *Bytes(std::string_view value) {
  return value.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(value.data());
}

std::string CopyText(VinputFcitxStringView view) {
  return view.data == nullptr || view.len == 0
             ? std::string{}
             : std::string(reinterpret_cast<const char *>(view.data), view.len);
}

} // namespace

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

bool SceneStateSnapshot::SetActive(std::string_view active_scene_id) {
  return vinput_fcitx_scene_snapshot_set_active(snapshot_, Bytes(active_scene_id),
                                                active_scene_id.size()) != 0;
}

std::optional<std::string> SceneStateSnapshot::active_scene_id() const {
  VinputFcitxSceneSnapshotView view{};
  if (vinput_fcitx_scene_snapshot_view(snapshot_, &view) == 0) {
    return std::nullopt;
  }
  return CopyText(view.active_scene_id);
}

const ::VinputFcitxSceneSnapshot *SceneStateSnapshot::raw_handle() const {
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
