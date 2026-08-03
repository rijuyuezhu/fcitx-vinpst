#include "vinput_fcitx_bridge/fcitx_menu_projection.h"

#include "vinput_fcitx_bridge/menu_snapshot.h"
#include "vinput_fcitx_ffi.h"

#include <cstdint>

namespace vinput_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(ProjectedMenuControlKind::None) ==
              VINPUT_FCITX_MENU_CONTROL_NONE);
static_assert(static_cast<std::uint8_t>(ProjectedMenuControlKind::SetActiveScene) ==
              VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_SCENE);
static_assert(static_cast<std::uint8_t>(ProjectedMenuControlKind::SetActiveAsrTarget) ==
              VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_ASR_TARGET);

const std::uint8_t *Bytes(std::string_view value) {
  return value.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(value.data());
}

std::string CopyText(VinputFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

std::optional<ProjectedMenuItem>
CopyProjectedItem(const VinputFcitxProjectedMenuItemView &view) {
  if (view.control_kind == VINPUT_FCITX_MENU_CONTROL_NONE ||
      view.control_kind > VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_ASR_TARGET) {
    return std::nullopt;
  }
  return ProjectedMenuItem{
      CopyText(view.label),
      ProjectedMenuControl{
          static_cast<ProjectedMenuControlKind>(view.control_kind),
          CopyText(view.control_first),
          CopyText(view.control_second),
          CopyText(view.control_label),
      },
  };
}

std::optional<std::vector<ProjectedMenuItem>>
CopyProjection(const VinputFcitxProjectionView &summary,
               const VinputFcitxAsrProjection *projection) {
  std::vector<ProjectedMenuItem> items;
  items.reserve(summary.item_count);
  for (std::size_t index = 0; index < summary.item_count; ++index) {
    VinputFcitxProjectedMenuItemView view{};
    if (vinput_fcitx_asr_projection_item_view(projection, index, &view) == 0) {
      return std::nullopt;
    }
    auto item = CopyProjectedItem(view);
    if (!item.has_value()) {
      return std::nullopt;
    }
    items.push_back(std::move(*item));
  }
  return items;
}

} // namespace

AsrMenuProjectionBuilder::AsrMenuProjectionBuilder(
    const AsrDisplayMenuStateSnapshot &snapshot, std::string_view query)
    : projection_(vinput_fcitx_asr_projection_new(snapshot.raw_handle(), Bytes(query),
                                                  query.size())) {}

AsrMenuProjectionBuilder::~AsrMenuProjectionBuilder() {
  vinput_fcitx_asr_projection_free(projection_);
}

bool AsrMenuProjectionBuilder::SetLabel(std::size_t row_index,
                                        std::string_view rendered_label) {
  return vinput_fcitx_asr_projection_set_label(
             projection_, row_index, Bytes(rendered_label), rendered_label.size()) != 0;
}

std::optional<std::vector<ProjectedMenuItem>> AsrMenuProjectionBuilder::Finish() {
  if (vinput_fcitx_asr_projection_finish(projection_) == 0) {
    return std::nullopt;
  }
  VinputFcitxProjectionView summary{};
  if (vinput_fcitx_asr_projection_view(projection_, &summary) == 0) {
    return std::nullopt;
  }
  return CopyProjection(summary, projection_);
}

SceneMenuProjectionBuilder::SceneMenuProjectionBuilder(
    const SceneStateSnapshot &snapshot, std::string_view query)
    : projection_(vinput_fcitx_scene_projection_new(snapshot.raw_handle(), Bytes(query),
                                                    query.size())) {}

SceneMenuProjectionBuilder::~SceneMenuProjectionBuilder() {
  vinput_fcitx_scene_projection_free(projection_);
}

std::optional<SceneMenuProjectionResult> SceneMenuProjectionBuilder::Finish() {
  VinputFcitxSceneProjectionView summary{};
  if (vinput_fcitx_scene_projection_view(projection_, &summary) == 0) {
    return std::nullopt;
  }

  SceneMenuProjectionResult result;
  result.active_label = CopyText(summary.active_label);
  result.items.reserve(summary.item_count);
  for (std::size_t index = 0; index < summary.item_count; ++index) {
    VinputFcitxProjectedMenuItemView view{};
    if (vinput_fcitx_scene_projection_item_view(projection_, index, &view) == 0) {
      return std::nullopt;
    }
    auto item = CopyProjectedItem(view);
    if (!item.has_value()) {
      return std::nullopt;
    }
    result.items.push_back(std::move(*item));
  }
  return result;
}

} // namespace vinput_fcitx_bridge
