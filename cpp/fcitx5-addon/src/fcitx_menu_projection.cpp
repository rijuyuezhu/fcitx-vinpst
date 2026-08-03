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
    const AsrDisplayMenuStateSnapshot &snapshot, std::string_view query,
    const AsrMenuLocalization &localization)
    : projection_(vinput_fcitx_asr_projection_new(
          snapshot.raw_handle(), Bytes(query), query.size(), Bytes(localization.local),
          localization.local.size(), Bytes(localization.remote),
          localization.remote.size(), Bytes(localization.command),
          localization.command.size(), Bytes(localization.loading_suffix),
          localization.loading_suffix.size(), Bytes(localization.unavailable),
          localization.unavailable.size(), Bytes(localization.loading_prefix),
          localization.loading_prefix.size(), Bytes(localization.error_prefix),
          localization.error_prefix.size())) {}

AsrMenuProjectionBuilder::~AsrMenuProjectionBuilder() {
  vinput_fcitx_asr_projection_free(projection_);
}

std::optional<AsrMenuProjectionResult> AsrMenuProjectionBuilder::Finish() {
  VinputFcitxProjectionView summary{};
  if (vinput_fcitx_asr_projection_view(projection_, &summary) == 0) {
    return std::nullopt;
  }
  auto items = CopyProjection(summary, projection_);
  if (!items.has_value()) {
    return std::nullopt;
  }
  return AsrMenuProjectionResult{CopyText(summary.effective_label), std::move(*items)};
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
