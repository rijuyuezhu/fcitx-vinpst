#include "vinput_fcitx_bridge/menu_snapshot.h"

#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

const std::uint8_t *Bytes(std::string_view value) {
  return reinterpret_cast<const std::uint8_t *>(value.data());
}

std::string CopyBytes(const std::uint8_t *data, std::size_t size) {
  if (data == nullptr || size == 0) {
    return {};
  }
  return std::string(reinterpret_cast<const char *>(data), size);
}

} // namespace

SceneStateSnapshot::SceneStateSnapshot(std::string_view active_scene_id)
    : snapshot_(vinput_fcitx_scene_snapshot_new(Bytes(active_scene_id),
                                                active_scene_id.size())) {}

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

std::string SceneStateSnapshot::active_scene_id() const {
  return CopyBytes(vinput_fcitx_scene_snapshot_active_id_data(snapshot_),
                   vinput_fcitx_scene_snapshot_active_id_len(snapshot_));
}

std::string SceneStateSnapshot::active_label() const {
  return CopyBytes(vinput_fcitx_scene_snapshot_active_label_data(snapshot_),
                   vinput_fcitx_scene_snapshot_active_label_len(snapshot_));
}

std::size_t SceneStateSnapshot::size() const {
  return vinput_fcitx_scene_snapshot_item_count(snapshot_);
}

std::optional<SceneStateItem> SceneStateSnapshot::item(std::size_t index) const {
  if (index >= size()) {
    return std::nullopt;
  }
  return SceneStateItem{
      CopyBytes(vinput_fcitx_scene_snapshot_item_id_data(snapshot_, index),
                vinput_fcitx_scene_snapshot_item_id_len(snapshot_, index)),
      CopyBytes(vinput_fcitx_scene_snapshot_item_label_data(snapshot_, index),
                vinput_fcitx_scene_snapshot_item_label_len(snapshot_, index))};
}

const ::VinputFcitxSceneSnapshot *SceneStateSnapshot::raw_handle() const {
  return snapshot_;
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

std::string AsrDisplayMenuStateSnapshot::target_provider_id() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_target_provider_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_target_provider_len(snapshot_));
}

std::string AsrDisplayMenuStateSnapshot::target_model_id() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_target_model_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_target_model_len(snapshot_));
}

std::string AsrDisplayMenuStateSnapshot::effective_provider_id() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_effective_provider_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_effective_provider_len(snapshot_));
}

std::string AsrDisplayMenuStateSnapshot::effective_model_id() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_effective_model_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_effective_model_len(snapshot_));
}

bool AsrDisplayMenuStateSnapshot::reload_in_progress() const {
  return vinput_fcitx_asr_display_snapshot_reload_in_progress(snapshot_) != 0;
}

std::string AsrDisplayMenuStateSnapshot::last_error() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_last_error_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_last_error_len(snapshot_));
}

std::string AsrDisplayMenuStateSnapshot::effective_base_label() const {
  return CopyBytes(
      vinput_fcitx_asr_display_snapshot_effective_base_label_data(snapshot_),
      vinput_fcitx_asr_display_snapshot_effective_base_label_len(snapshot_));
}

std::string AsrDisplayMenuStateSnapshot::target_base_label() const {
  return CopyBytes(vinput_fcitx_asr_display_snapshot_target_base_label_data(snapshot_),
                   vinput_fcitx_asr_display_snapshot_target_base_label_len(snapshot_));
}

std::size_t AsrDisplayMenuStateSnapshot::size() const {
  return vinput_fcitx_asr_display_snapshot_item_count(snapshot_);
}

std::optional<AsrDisplayMenuPresentation>
AsrDisplayMenuStateSnapshot::presentation(std::size_t index) const {
  if (index >= size()) {
    return std::nullopt;
  }
  return AsrDisplayMenuPresentation{
      CopyBytes(vinput_fcitx_asr_display_snapshot_item_kind_data(snapshot_, index),
                vinput_fcitx_asr_display_snapshot_item_kind_len(snapshot_, index)),
      CopyBytes(
          vinput_fcitx_asr_display_snapshot_item_base_label_data(snapshot_, index),
          vinput_fcitx_asr_display_snapshot_item_base_label_len(snapshot_, index)),
      vinput_fcitx_asr_display_snapshot_item_is_loading(snapshot_, index) != 0};
}

std::optional<AsrDisplayMenuItem>
AsrDisplayMenuStateSnapshot::item(std::size_t index) const {
  if (index >= size()) {
    return std::nullopt;
  }
  return AsrDisplayMenuItem{
      CopyBytes(vinput_fcitx_asr_display_snapshot_item_provider_data(snapshot_, index),
                vinput_fcitx_asr_display_snapshot_item_provider_len(snapshot_, index)),
      CopyBytes(vinput_fcitx_asr_display_snapshot_item_kind_data(snapshot_, index),
                vinput_fcitx_asr_display_snapshot_item_kind_len(snapshot_, index)),
      CopyBytes(vinput_fcitx_asr_display_snapshot_item_id_data(snapshot_, index),
                vinput_fcitx_asr_display_snapshot_item_id_len(snapshot_, index)),
      CopyBytes(
          vinput_fcitx_asr_display_snapshot_item_display_title_data(snapshot_, index),
          vinput_fcitx_asr_display_snapshot_item_display_title_len(snapshot_, index)),
      CopyBytes(
          vinput_fcitx_asr_display_snapshot_item_model_value_data(snapshot_, index),
          vinput_fcitx_asr_display_snapshot_item_model_value_len(snapshot_, index)),
      CopyBytes(
          vinput_fcitx_asr_display_snapshot_item_base_label_data(snapshot_, index),
          vinput_fcitx_asr_display_snapshot_item_base_label_len(snapshot_, index)),
      vinput_fcitx_asr_display_snapshot_item_is_loading(snapshot_, index) != 0};
}

const ::VinputFcitxAsrDisplaySnapshot *AsrDisplayMenuStateSnapshot::raw_handle() const {
  return snapshot_;
}

} // namespace vinput_fcitx_bridge
