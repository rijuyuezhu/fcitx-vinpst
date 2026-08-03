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

SceneStateSnapshot::SceneStateSnapshot(std::string_view active_scene_id)
    : snapshot_(vinput_fcitx_scene_snapshot_new(Bytes(active_scene_id),
                                                active_scene_id.size())) {}

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

bool SceneStateSnapshot::valid() const {
  return snapshot_ != nullptr;
}

bool SceneStateSnapshot::Add(std::string_view id, std::string_view label) {
  return vinput_fcitx_scene_snapshot_add(snapshot_, Bytes(id), id.size(), Bytes(label),
                                         label.size()) != 0;
}

bool SceneStateSnapshot::SetActive(std::string_view active_scene_id) {
  return vinput_fcitx_scene_snapshot_set_active(snapshot_, Bytes(active_scene_id),
                                                active_scene_id.size()) != 0;
}

std::optional<SceneState> SceneStateSnapshot::state() const {
  VinputFcitxSceneSnapshotView view{};
  if (vinput_fcitx_scene_snapshot_view(snapshot_, &view) == 0) {
    return std::nullopt;
  }
  return SceneState{CopyText(view.active_scene_id), view.item_count};
}

std::optional<SceneStateItem> SceneStateSnapshot::item(std::size_t index) const {
  VinputFcitxSceneSnapshotItemView view{};
  if (vinput_fcitx_scene_snapshot_item_view(snapshot_, index, &view) == 0) {
    return std::nullopt;
  }
  return SceneStateItem{CopyText(view.id), CopyText(view.label)};
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

AsrDisplayMenuStateSnapshot::AsrDisplayMenuStateSnapshot(
    std::string_view target_provider_id, std::string_view target_model_id,
    std::string_view effective_provider_id, std::string_view effective_model_id,
    bool reload_in_progress, std::string_view last_error)
    : snapshot_(vinput_fcitx_asr_display_snapshot_new(
          Bytes(target_provider_id), target_provider_id.size(), Bytes(target_model_id),
          target_model_id.size(), Bytes(effective_provider_id),
          effective_provider_id.size(), Bytes(effective_model_id),
          effective_model_id.size(), static_cast<std::uint8_t>(reload_in_progress),
          Bytes(last_error), last_error.size())) {}

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

bool AsrDisplayMenuStateSnapshot::valid() const {
  return snapshot_ != nullptr;
}

bool AsrDisplayMenuStateSnapshot::Add(std::string_view provider_id,
                                      std::string_view kind, std::string_view item_id,
                                      std::string_view display_title,
                                      std::string_view model_value) {
  return vinput_fcitx_asr_display_snapshot_add(
             snapshot_, Bytes(provider_id), provider_id.size(), Bytes(kind),
             kind.size(), Bytes(item_id), item_id.size(), Bytes(display_title),
             display_title.size(), Bytes(model_value), model_value.size()) != 0;
}

std::optional<AsrDisplayMenuState> AsrDisplayMenuStateSnapshot::state() const {
  VinputFcitxAsrDisplaySnapshotView view{};
  if (vinput_fcitx_asr_display_snapshot_view(snapshot_, &view) == 0) {
    return std::nullopt;
  }
  return AsrDisplayMenuState{CopyText(view.target_provider_id),
                             CopyText(view.target_model_id),
                             CopyText(view.effective_provider_id),
                             CopyText(view.effective_model_id),
                             CopyText(view.last_error),
                             CopyText(view.effective_base_label),
                             CopyText(view.target_base_label),
                             view.reload_in_progress != 0,
                             view.item_count};
}

std::optional<AsrDisplayMenuPresentation>
AsrDisplayMenuStateSnapshot::presentation(std::size_t index) const {
  VinputFcitxAsrDisplaySnapshotItemView view{};
  if (vinput_fcitx_asr_display_snapshot_item_view(snapshot_, index, &view) == 0) {
    return std::nullopt;
  }
  return AsrDisplayMenuPresentation{CopyText(view.kind), CopyText(view.base_label),
                                    view.is_loading != 0};
}

std::optional<AsrDisplayMenuItem>
AsrDisplayMenuStateSnapshot::item(std::size_t index) const {
  VinputFcitxAsrDisplaySnapshotItemView view{};
  if (vinput_fcitx_asr_display_snapshot_item_view(snapshot_, index, &view) == 0) {
    return std::nullopt;
  }
  return AsrDisplayMenuItem{CopyText(view.provider_id), CopyText(view.kind),
                            CopyText(view.item_id),     CopyText(view.display_title),
                            CopyText(view.model_value), CopyText(view.base_label),
                            view.is_loading != 0};
}

const ::VinputFcitxAsrDisplaySnapshot *AsrDisplayMenuStateSnapshot::raw_handle() const {
  return snapshot_;
}

} // namespace vinput_fcitx_bridge
