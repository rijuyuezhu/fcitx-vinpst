#pragma once

#include "vinput_fcitx_bridge/menu_snapshot.h"
#include "vinput_fcitx_bridge/rust_handle.h"

#include <cstddef>
#include <cstdint>
#include <memory>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

struct VinputFcitxSceneProjection;
struct VinputFcitxAsrProjection;

extern "C" {
void vinput_fcitx_asr_projection_free(VinputFcitxAsrProjection *projection);
void vinput_fcitx_scene_projection_free(VinputFcitxSceneProjection *projection);
}

namespace vinput_fcitx_bridge {

class MenuFilterState;

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

class SceneMenuProjection final {
public:
  SceneMenuProjection(const SceneMenuProjection &) = delete;
  SceneMenuProjection &operator=(const SceneMenuProjection &) = delete;
  SceneMenuProjection(SceneMenuProjection &&) = delete;
  SceneMenuProjection &operator=(SceneMenuProjection &&) = delete;

  std::optional<std::string> active_label() const;
  std::optional<std::size_t> size() const;
  std::optional<ProjectedMenuItem> item(std::size_t index) const;

private:
  using Handle =
      RustOwnedHandle<::VinputFcitxSceneProjection, vinput_fcitx_scene_projection_free>;

  friend std::shared_ptr<SceneMenuProjection>
  ProjectSceneMenu(const SceneStateSnapshot &, const MenuFilterState &);

  explicit SceneMenuProjection(::VinputFcitxSceneProjection *projection);

  Handle projection_;
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

class AsrMenuProjection final {
public:
  AsrMenuProjection(const AsrMenuProjection &) = delete;
  AsrMenuProjection &operator=(const AsrMenuProjection &) = delete;
  AsrMenuProjection(AsrMenuProjection &&) = delete;
  AsrMenuProjection &operator=(AsrMenuProjection &&) = delete;

  std::optional<std::string> effective_label() const;
  std::optional<std::size_t> size() const;
  std::optional<ProjectedMenuItem> item(std::size_t index) const;

private:
  using Handle =
      RustOwnedHandle<::VinputFcitxAsrProjection, vinput_fcitx_asr_projection_free>;

  friend std::shared_ptr<AsrMenuProjection>
  ProjectAsrMenu(const AsrDisplayMenuStateSnapshot &, const MenuFilterState &,
                 const AsrMenuLocalization &);

  explicit AsrMenuProjection(::VinputFcitxAsrProjection *projection);

  Handle projection_;
};

std::shared_ptr<AsrMenuProjection>
ProjectAsrMenu(const AsrDisplayMenuStateSnapshot &snapshot,
               const MenuFilterState &filter, const AsrMenuLocalization &localization);

std::shared_ptr<SceneMenuProjection>
ProjectSceneMenu(const SceneStateSnapshot &snapshot, const MenuFilterState &filter);

} // namespace vinput_fcitx_bridge
