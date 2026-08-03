#include "vinput_fcitx_bridge/fcitx_menu_projection.h"

#include "vinput_fcitx_bridge/fcitx_menu_filter.h"
#include "vinput_fcitx_bridge/menu_snapshot.h"
#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <memory>

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

struct AsrProjectionDeleter {
  void operator()(VinputFcitxAsrProjection *projection) const {
    vinput_fcitx_asr_projection_free(projection);
  }
};

struct SceneProjectionDeleter {
  void operator()(VinputFcitxSceneProjection *projection) const {
    vinput_fcitx_scene_projection_free(projection);
  }
};

} // namespace

std::optional<AsrMenuProjectionResult>
ProjectAsrMenu(const AsrDisplayMenuStateSnapshot &snapshot,
               const MenuFilterState &filter, const AsrMenuLocalization &localization) {
  std::unique_ptr<VinputFcitxAsrProjection, AsrProjectionDeleter> projection(
      vinput_fcitx_asr_projection_new(
          snapshot.raw_handle(), filter.raw_handle(), Bytes(localization.local),
          localization.local.size(), Bytes(localization.remote),
          localization.remote.size(), Bytes(localization.command),
          localization.command.size(), Bytes(localization.loading_suffix),
          localization.loading_suffix.size(), Bytes(localization.unavailable),
          localization.unavailable.size(), Bytes(localization.loading_prefix),
          localization.loading_prefix.size(), Bytes(localization.error_prefix),
          localization.error_prefix.size()));
  VinputFcitxProjectionView summary{};
  if (vinput_fcitx_asr_projection_view(projection.get(), &summary) == 0) {
    return std::nullopt;
  }
  auto items = CopyProjection(summary, projection.get());
  if (!items.has_value()) {
    return std::nullopt;
  }
  return AsrMenuProjectionResult{CopyText(summary.effective_label), std::move(*items)};
}

std::optional<SceneMenuProjectionResult>
ProjectSceneMenu(const SceneStateSnapshot &snapshot, const MenuFilterState &filter) {
  std::unique_ptr<VinputFcitxSceneProjection, SceneProjectionDeleter> projection(
      vinput_fcitx_scene_projection_new(snapshot.raw_handle(), filter.raw_handle()));
  VinputFcitxSceneProjectionView summary{};
  if (vinput_fcitx_scene_projection_view(projection.get(), &summary) == 0) {
    return std::nullopt;
  }

  SceneMenuProjectionResult result;
  result.active_label = CopyText(summary.active_label);
  result.items.reserve(summary.item_count);
  for (std::size_t index = 0; index < summary.item_count; ++index) {
    VinputFcitxProjectedMenuItemView view{};
    if (vinput_fcitx_scene_projection_item_view(projection.get(), index, &view) == 0) {
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
