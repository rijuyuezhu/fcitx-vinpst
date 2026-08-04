#include "vinput_fcitx_bridge/fcitx_menu_projection.h"

#include "vinput_fcitx_bridge/fcitx_menu_filter.h"
#include "vinput_fcitx_bridge/rust_string.h"
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

std::optional<ProjectedMenuItem>
CopyProjectedItem(const VinputFcitxProjectedMenuItemView &view) {
  if (view.control_kind == VINPUT_FCITX_MENU_CONTROL_NONE ||
      view.control_kind > VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_ASR_TARGET) {
    return std::nullopt;
  }
  return ProjectedMenuItem{
      CopyRustString(view.label),
      ProjectedMenuControl{
          static_cast<ProjectedMenuControlKind>(view.control_kind),
          CopyRustString(view.control_first),
          CopyRustString(view.control_second),
          CopyRustString(view.control_label),
      },
  };
}

} // namespace

MenuProjection::MenuProjection(VinputFcitxMenuProjection *projection)
    : projection_(Handle::Adopt(projection)) {}

std::shared_ptr<MenuProjection>
MenuProjection::Adopt(VinputFcitxMenuProjection *raw_projection) {
  if (raw_projection == nullptr) {
    return {};
  }
  auto projection = std::shared_ptr<MenuProjection>(new MenuProjection(raw_projection));
  if (!projection->size().has_value() || !projection->summary().has_value()) {
    return {};
  }
  return projection;
}

std::optional<std::string> MenuProjection::summary() const {
  VinputFcitxMenuProjectionView view{};
  if (vinput_fcitx_menu_projection_view(projection_.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  return CopyRustString(view.summary);
}

std::optional<std::size_t> MenuProjection::size() const {
  VinputFcitxMenuProjectionView view{};
  if (vinput_fcitx_menu_projection_view(projection_.raw_handle(), &view) == 0) {
    return std::nullopt;
  }
  return view.item_count;
}

std::optional<ProjectedMenuItem> MenuProjection::item(std::size_t index) const {
  VinputFcitxProjectedMenuItemView view{};
  if (vinput_fcitx_menu_projection_item_view(projection_.raw_handle(), index, &view) ==
      0) {
    return std::nullopt;
  }
  return CopyProjectedItem(view);
}

std::shared_ptr<MenuProjection>
AsrMenuController::Project(const MenuSessionState &session,
                           const AsrMenuLocalization &localization) const {
  const VinputFcitxAsrMenuTextView text{
      .local = ToRustStringView(localization.local),
      .remote = ToRustStringView(localization.remote),
      .command = ToRustStringView(localization.command),
      .loading_suffix = ToRustStringView(localization.loading_suffix),
      .unavailable = ToRustStringView(localization.unavailable),
      .loading_prefix = ToRustStringView(localization.loading_prefix),
      .error_prefix = ToRustStringView(localization.error_prefix),
  };
  return MenuProjection::Adopt(vinput_fcitx_asr_menu_controller_projection_new(
      raw_handle(), session.raw_handle(), &text));
}

std::shared_ptr<MenuProjection>
SceneMenuController::Project(const MenuSessionState &session) const {
  return MenuProjection::Adopt(vinput_fcitx_scene_menu_controller_projection_new(
      raw_handle(), session.raw_handle()));
}

} // namespace vinput_fcitx_bridge
