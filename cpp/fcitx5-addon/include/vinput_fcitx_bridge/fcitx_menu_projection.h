#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>
#include <vector>

struct VinputFcitxAsrProjection;
struct VinputFcitxSceneProjection;

namespace vinput_fcitx_bridge {

class AsrDisplayMenuStateSnapshot;
class SceneStateSnapshot;

struct ProjectedMenuItem {
  std::size_t source_index = 0;
  std::string label;
};

struct SceneMenuProjectionResult {
  std::string active_label;
  std::vector<ProjectedMenuItem> items;
};

class AsrMenuProjectionBuilder {
public:
  AsrMenuProjectionBuilder(const AsrDisplayMenuStateSnapshot &snapshot,
                           std::string_view query);
  AsrMenuProjectionBuilder(std::string_view target_provider_id,
                           std::string_view target_model_id,
                           std::string_view effective_provider_id,
                           std::string_view effective_model_id, bool reload_in_progress,
                           std::string_view last_error, std::string_view query);
  ~AsrMenuProjectionBuilder();

  AsrMenuProjectionBuilder(const AsrMenuProjectionBuilder &) = delete;
  AsrMenuProjectionBuilder &operator=(const AsrMenuProjectionBuilder &) = delete;
  AsrMenuProjectionBuilder(AsrMenuProjectionBuilder &&) = delete;
  AsrMenuProjectionBuilder &operator=(AsrMenuProjectionBuilder &&) = delete;

  bool Add(std::size_t source_index, std::string_view provider_id,
           std::string_view kind, std::string_view item_id,
           std::string_view display_title, std::string_view model_value,
           std::string_view rendered_label);
  bool Add(const AsrDisplayMenuStateSnapshot &snapshot, std::size_t source_index,
           std::string_view rendered_label);
  std::optional<std::vector<ProjectedMenuItem>> Finish();

private:
  ::VinputFcitxAsrProjection *projection_ = nullptr;
};

class SceneMenuProjectionBuilder {
public:
  SceneMenuProjectionBuilder(const SceneStateSnapshot &snapshot,
                             std::string_view query);
  SceneMenuProjectionBuilder(std::string_view active_scene_id, std::string_view query);
  ~SceneMenuProjectionBuilder();

  SceneMenuProjectionBuilder(const SceneMenuProjectionBuilder &) = delete;
  SceneMenuProjectionBuilder &operator=(const SceneMenuProjectionBuilder &) = delete;
  SceneMenuProjectionBuilder(SceneMenuProjectionBuilder &&) = delete;
  SceneMenuProjectionBuilder &operator=(SceneMenuProjectionBuilder &&) = delete;

  bool Add(std::size_t source_index, std::string_view id, std::string_view label);
  std::optional<SceneMenuProjectionResult> Finish();

private:
  ::VinputFcitxSceneProjection *projection_ = nullptr;
};

} // namespace vinput_fcitx_bridge
