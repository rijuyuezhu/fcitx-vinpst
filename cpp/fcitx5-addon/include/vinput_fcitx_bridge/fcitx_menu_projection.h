#pragma once

#include <cstddef>
#include <cstdint>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

struct VinputFcitxAsrProjection;
struct VinputFcitxSceneProjection;

namespace vinput_fcitx_bridge {

class AsrDisplayMenuStateSnapshot;
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

class AsrMenuProjectionBuilder {
public:
  AsrMenuProjectionBuilder(const AsrDisplayMenuStateSnapshot &snapshot,
                           std::string_view query);
  ~AsrMenuProjectionBuilder();

  AsrMenuProjectionBuilder(const AsrMenuProjectionBuilder &) = delete;
  AsrMenuProjectionBuilder &operator=(const AsrMenuProjectionBuilder &) = delete;
  AsrMenuProjectionBuilder(AsrMenuProjectionBuilder &&) = delete;
  AsrMenuProjectionBuilder &operator=(AsrMenuProjectionBuilder &&) = delete;

  bool SetLabel(std::size_t row_index, std::string_view rendered_label);
  std::optional<std::vector<ProjectedMenuItem>> Finish();

private:
  ::VinputFcitxAsrProjection *projection_ = nullptr;
};

class SceneMenuProjectionBuilder {
public:
  SceneMenuProjectionBuilder(const SceneStateSnapshot &snapshot,
                             std::string_view query);
  ~SceneMenuProjectionBuilder();

  SceneMenuProjectionBuilder(const SceneMenuProjectionBuilder &) = delete;
  SceneMenuProjectionBuilder &operator=(const SceneMenuProjectionBuilder &) = delete;
  SceneMenuProjectionBuilder(SceneMenuProjectionBuilder &&) = delete;
  SceneMenuProjectionBuilder &operator=(SceneMenuProjectionBuilder &&) = delete;

  std::optional<SceneMenuProjectionResult> Finish();

private:
  ::VinputFcitxSceneProjection *projection_ = nullptr;
};

} // namespace vinput_fcitx_bridge
