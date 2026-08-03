#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

namespace vinput_fcitx_bridge {

class AsrDisplayMenuStateSnapshot;
class MenuFilterState;
class SceneStateSnapshot;

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

struct SceneMenuProjectionResult {
  std::string active_label;
  std::vector<ProjectedMenuItem> items;
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

struct AsrMenuProjectionResult {
  std::string effective_label;
  std::vector<ProjectedMenuItem> items;
};

std::optional<AsrMenuProjectionResult>
ProjectAsrMenu(const AsrDisplayMenuStateSnapshot &snapshot,
               const MenuFilterState &filter, const AsrMenuLocalization &localization);

std::optional<SceneMenuProjectionResult>
ProjectSceneMenu(const SceneStateSnapshot &snapshot, const MenuFilterState &filter);

} // namespace vinput_fcitx_bridge
