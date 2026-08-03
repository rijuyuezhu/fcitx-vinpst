#pragma once

#include "vinput_fcitx_bridge/rust_handle.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>

struct VinputFcitxMenuProjection;
struct VinputFcitxAsrMenuController;
struct VinputFcitxSceneMenuController;

extern "C" void
vinput_fcitx_menu_projection_free(VinputFcitxMenuProjection *projection);
extern "C" VinputFcitxAsrMenuController *vinput_fcitx_asr_menu_controller_new();
extern "C" void
vinput_fcitx_asr_menu_controller_free(VinputFcitxAsrMenuController *controller);
extern "C" VinputFcitxSceneMenuController *vinput_fcitx_scene_menu_controller_new();
extern "C" void
vinput_fcitx_scene_menu_controller_free(VinputFcitxSceneMenuController *controller);

namespace vinput_fcitx_bridge {

class MenuSessionState;

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
      RustOwnedHandle<::VinputFcitxMenuProjection, vinput_fcitx_menu_projection_free>;

  friend class AsrMenuController;
  friend class SceneMenuController;

  explicit MenuProjection(::VinputFcitxMenuProjection *projection);

  Handle projection_;
};

class SceneMenuController final {
public:
  SceneMenuController()
      : controller_(Handle::Adopt(vinput_fcitx_scene_menu_controller_new())) {}
  ~SceneMenuController() = default;

  SceneMenuController(const SceneMenuController &) = delete;
  SceneMenuController &operator=(const SceneMenuController &) = delete;
  SceneMenuController(SceneMenuController &&) = delete;
  SceneMenuController &operator=(SceneMenuController &&) = delete;

  std::shared_ptr<MenuProjection> Project(const MenuSessionState &session) const;
  const ::VinputFcitxSceneMenuController *raw_handle() const {
    return controller_.raw_handle();
  }
  ::VinputFcitxSceneMenuController *mutable_raw_handle() {
    return controller_.mutable_raw_handle();
  }

private:
  using Handle = RustOwnedHandle<::VinputFcitxSceneMenuController,
                                 vinput_fcitx_scene_menu_controller_free>;

  Handle controller_;
};

class AsrMenuController final {
public:
  AsrMenuController()
      : controller_(Handle::Adopt(vinput_fcitx_asr_menu_controller_new())) {}
  ~AsrMenuController() = default;

  AsrMenuController(const AsrMenuController &) = delete;
  AsrMenuController &operator=(const AsrMenuController &) = delete;
  AsrMenuController(AsrMenuController &&) = delete;
  AsrMenuController &operator=(AsrMenuController &&) = delete;

  std::shared_ptr<MenuProjection>
  Project(const MenuSessionState &session,
          const AsrMenuLocalization &localization) const;
  const ::VinputFcitxAsrMenuController *raw_handle() const {
    return controller_.raw_handle();
  }
  ::VinputFcitxAsrMenuController *mutable_raw_handle() {
    return controller_.mutable_raw_handle();
  }

private:
  using Handle = RustOwnedHandle<::VinputFcitxAsrMenuController,
                                 vinput_fcitx_asr_menu_controller_free>;

  Handle controller_;
};

} // namespace vinput_fcitx_bridge
