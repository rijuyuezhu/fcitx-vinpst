#pragma once

#include "vinpst_fcitx_bridge/rust_handle.h"
#include "vinpst_fcitx_ffi.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>

namespace vinpst_fcitx_bridge {

class MenuSessionState;

namespace detail {

template <typename Controller, Controller *(*New)(), void (*Free)(Controller *)>
class RustMenuControllerOwner {
public:
  RustMenuControllerOwner(const RustMenuControllerOwner &) = delete;
  RustMenuControllerOwner &operator=(const RustMenuControllerOwner &) = delete;
  RustMenuControllerOwner(RustMenuControllerOwner &&) noexcept = default;
  RustMenuControllerOwner &operator=(RustMenuControllerOwner &&) noexcept = default;

  const Controller *raw_handle() const {
    return controller_.raw_handle();
  }
  Controller *mutable_raw_handle() {
    return controller_.mutable_raw_handle();
  }

protected:
  RustMenuControllerOwner() : controller_(Handle::Adopt(New())) {}
  ~RustMenuControllerOwner() = default;

private:
  using Handle = RustOwnedHandle<Controller, Free>;

  Handle controller_;
};

} // namespace detail

enum class ProjectedMenuControlKind : std::uint8_t {
  None,
  SetActiveScene,
  SetActiveAsrTarget,
};

struct ProjectedMenuControl {
  ProjectedMenuControlKind kind = ProjectedMenuControlKind::None;
  std::string first;
  std::string second;
  std::string display_label;
};

struct ProjectedMenuItem {
  std::string label;
  ProjectedMenuControl control;
};

struct AsrMenuLocalization {
  std::string_view local;
  std::string_view remote;
  std::string_view command;
  std::string_view loading_suffix;
  std::string_view unavailable;
  std::string_view loading_prefix;
  std::string_view error_prefix;
};

class MenuProjection final {
public:
  MenuProjection(const MenuProjection &) = delete;
  MenuProjection &operator=(const MenuProjection &) = delete;
  MenuProjection(MenuProjection &&) = delete;
  MenuProjection &operator=(MenuProjection &&) = delete;

  std::optional<std::string> summary() const;
  std::optional<std::size_t> size() const;
  std::optional<ProjectedMenuItem> item(std::size_t index) const;

private:
  using Handle =
      RustOwnedHandle<::VinpstFcitxMenuProjection, vinpst_fcitx_menu_projection_free>;

  friend class AsrMenuController;
  friend class SceneMenuController;

  explicit MenuProjection(::VinpstFcitxMenuProjection *projection);
  static std::shared_ptr<MenuProjection> Adopt(::VinpstFcitxMenuProjection *projection);

  Handle projection_;
};

class SceneMenuController final
    : private detail::RustMenuControllerOwner<::VinpstFcitxSceneMenuController,
                                              vinpst_fcitx_scene_menu_controller_new,
                                              vinpst_fcitx_scene_menu_controller_free> {
public:
  using Owner =
      detail::RustMenuControllerOwner<::VinpstFcitxSceneMenuController,
                                      vinpst_fcitx_scene_menu_controller_new,
                                      vinpst_fcitx_scene_menu_controller_free>;

  SceneMenuController() = default;
  SceneMenuController(SceneMenuController &&) noexcept = default;
  SceneMenuController &operator=(SceneMenuController &&) noexcept = default;

  std::shared_ptr<MenuProjection> Project(const MenuSessionState &session) const;
  using Owner::mutable_raw_handle;
  using Owner::raw_handle;
};

class AsrMenuController final
    : private detail::RustMenuControllerOwner<::VinpstFcitxAsrMenuController,
                                              vinpst_fcitx_asr_menu_controller_new,
                                              vinpst_fcitx_asr_menu_controller_free> {
public:
  using Owner = detail::RustMenuControllerOwner<::VinpstFcitxAsrMenuController,
                                                vinpst_fcitx_asr_menu_controller_new,
                                                vinpst_fcitx_asr_menu_controller_free>;

  AsrMenuController() = default;
  AsrMenuController(AsrMenuController &&) noexcept = default;
  AsrMenuController &operator=(AsrMenuController &&) noexcept = default;

  std::shared_ptr<MenuProjection>
  Project(const MenuSessionState &session,
          const AsrMenuLocalization &localization) const;
  using Owner::mutable_raw_handle;
  using Owner::raw_handle;
};

} // namespace vinpst_fcitx_bridge
