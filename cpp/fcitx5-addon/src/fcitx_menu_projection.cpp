#include "vinput_fcitx_bridge/fcitx_menu_projection.h"
#include "vinput_fcitx_bridge/menu_snapshot.h"

#include "vinput_fcitx_ffi.h"

#include <cstdint>

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

AsrMenuProjectionBuilder::AsrMenuProjectionBuilder(
    const AsrDisplayMenuStateSnapshot &snapshot, std::string_view query)
    : projection_(vinput_fcitx_asr_projection_new_from_snapshot(
          snapshot.raw_handle(), Bytes(query), query.size())) {}

AsrMenuProjectionBuilder::AsrMenuProjectionBuilder(
    std::string_view target_provider_id, std::string_view target_model_id,
    std::string_view effective_provider_id, std::string_view effective_model_id,
    bool reload_in_progress, std::string_view last_error, std::string_view query)
    : projection_(vinput_fcitx_asr_projection_new(
          Bytes(target_provider_id), target_provider_id.size(), Bytes(target_model_id),
          target_model_id.size(), Bytes(effective_provider_id),
          effective_provider_id.size(), Bytes(effective_model_id),
          effective_model_id.size(), reload_in_progress ? 1 : 0, Bytes(last_error),
          last_error.size(), Bytes(query), query.size())) {}

AsrMenuProjectionBuilder::~AsrMenuProjectionBuilder() {
  vinput_fcitx_asr_projection_free(projection_);
}

bool AsrMenuProjectionBuilder::Add(std::size_t source_index,
                                   std::string_view provider_id, std::string_view kind,
                                   std::string_view item_id,
                                   std::string_view display_title,
                                   std::string_view model_value,
                                   std::string_view rendered_label) {
  return vinput_fcitx_asr_projection_add(
             projection_, source_index, Bytes(provider_id), provider_id.size(),
             Bytes(kind), kind.size(), Bytes(item_id), item_id.size(),
             Bytes(display_title), display_title.size(), Bytes(model_value),
             model_value.size(), Bytes(rendered_label), rendered_label.size()) != 0;
}

bool AsrMenuProjectionBuilder::Add(const AsrDisplayMenuStateSnapshot &snapshot,
                                   std::size_t source_index,
                                   std::string_view rendered_label) {
  return vinput_fcitx_asr_projection_add_snapshot_item(
             projection_, snapshot.raw_handle(), source_index, Bytes(rendered_label),
             rendered_label.size()) != 0;
}

std::optional<std::vector<ProjectedMenuItem>> AsrMenuProjectionBuilder::Finish() {
  if (vinput_fcitx_asr_projection_finish(projection_) == 0) {
    return std::nullopt;
  }

  std::vector<ProjectedMenuItem> items;
  const auto item_count = vinput_fcitx_asr_projection_item_count(projection_);
  items.reserve(item_count);
  for (std::size_t index = 0; index < item_count; ++index) {
    const auto source_index =
        vinput_fcitx_asr_projection_item_source_index(projection_, index);
    if (source_index == static_cast<std::size_t>(-1)) {
      return std::nullopt;
    }
    items.push_back(ProjectedMenuItem{
        source_index,
        CopyBytes(vinput_fcitx_asr_projection_item_label_data(projection_, index),
                  vinput_fcitx_asr_projection_item_label_len(projection_, index)),
    });
  }
  return items;
}

SceneMenuProjectionBuilder::SceneMenuProjectionBuilder(
    const SceneStateSnapshot &snapshot, std::string_view query)
    : projection_(vinput_fcitx_scene_projection_from_snapshot(
          snapshot.raw_handle(), Bytes(query), query.size())) {}

SceneMenuProjectionBuilder::SceneMenuProjectionBuilder(std::string_view active_scene_id,
                                                       std::string_view query)
    : projection_(vinput_fcitx_scene_projection_new(
          Bytes(active_scene_id), active_scene_id.size(), Bytes(query), query.size())) {
}

SceneMenuProjectionBuilder::~SceneMenuProjectionBuilder() {
  vinput_fcitx_scene_projection_free(projection_);
}

bool SceneMenuProjectionBuilder::Add(std::size_t source_index, std::string_view id,
                                     std::string_view label) {
  return vinput_fcitx_scene_projection_add(projection_, source_index, Bytes(id),
                                           id.size(), Bytes(label), label.size()) != 0;
}

std::optional<SceneMenuProjectionResult> SceneMenuProjectionBuilder::Finish() {
  if (vinput_fcitx_scene_projection_finish(projection_) == 0) {
    return std::nullopt;
  }

  SceneMenuProjectionResult result;
  result.active_label =
      CopyBytes(vinput_fcitx_scene_projection_active_label_data(projection_),
                vinput_fcitx_scene_projection_active_label_len(projection_));
  const auto item_count = vinput_fcitx_scene_projection_item_count(projection_);
  result.items.reserve(item_count);
  for (std::size_t index = 0; index < item_count; ++index) {
    const auto source_index =
        vinput_fcitx_scene_projection_item_source_index(projection_, index);
    if (source_index == static_cast<std::size_t>(-1)) {
      return std::nullopt;
    }
    result.items.push_back(ProjectedMenuItem{
        source_index,
        CopyBytes(vinput_fcitx_scene_projection_item_label_data(projection_, index),
                  vinput_fcitx_scene_projection_item_label_len(projection_, index)),
    });
  }
  return result;
}

} // namespace vinput_fcitx_bridge
