#pragma once

#include <cstddef>
#include <optional>
#include <string>
#include <string_view>

struct VinputFcitxAsrDisplaySnapshot;
struct VinputFcitxSceneSnapshot;

namespace vinput_fcitx_bridge {

struct SceneStateItem {
  std::string id;
  std::string label;
};

struct SceneState {
  std::string active_scene_id;
  std::size_t item_count = 0;
};

class SceneStateSnapshot {
public:
  SceneStateSnapshot() = default;
  explicit SceneStateSnapshot(std::string_view active_scene_id);
  static SceneStateSnapshot Adopt(::VinputFcitxSceneSnapshot *snapshot);
  ~SceneStateSnapshot();

  SceneStateSnapshot(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot &operator=(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot(SceneStateSnapshot &&other) noexcept;
  SceneStateSnapshot &operator=(SceneStateSnapshot &&other) noexcept;

  bool valid() const;
  bool Add(std::string_view id, std::string_view label);
  bool SetActive(std::string_view active_scene_id);
  std::optional<SceneState> state() const;
  std::optional<SceneStateItem> item(std::size_t index) const;
  const ::VinputFcitxSceneSnapshot *raw_handle() const;

private:
  explicit SceneStateSnapshot(::VinputFcitxSceneSnapshot *snapshot);
  ::VinputFcitxSceneSnapshot *snapshot_ = nullptr;
};

struct AsrDisplayMenuItem {
  std::string provider_id;
  std::string kind;
  std::string item_id;
  std::string display_title;
  std::string model_value;
  std::string base_label;
  bool loading = false;
};

struct AsrDisplayMenuPresentation {
  std::string kind;
  std::string base_label;
  bool loading = false;
};

struct AsrDisplayMenuState {
  std::string target_provider_id;
  std::string target_model_id;
  std::string effective_provider_id;
  std::string effective_model_id;
  std::string last_error;
  std::string effective_base_label;
  std::string target_base_label;
  bool reload_in_progress = false;
  std::size_t item_count = 0;
};

class AsrDisplayMenuStateSnapshot {
public:
  AsrDisplayMenuStateSnapshot() = default;
  static AsrDisplayMenuStateSnapshot Adopt(::VinputFcitxAsrDisplaySnapshot *snapshot);
  AsrDisplayMenuStateSnapshot(std::string_view target_provider_id,
                              std::string_view target_model_id,
                              std::string_view effective_provider_id,
                              std::string_view effective_model_id,
                              bool reload_in_progress, std::string_view last_error);
  ~AsrDisplayMenuStateSnapshot();

  AsrDisplayMenuStateSnapshot(const AsrDisplayMenuStateSnapshot &) = delete;
  AsrDisplayMenuStateSnapshot &operator=(const AsrDisplayMenuStateSnapshot &) = delete;
  AsrDisplayMenuStateSnapshot(AsrDisplayMenuStateSnapshot &&other) noexcept;
  AsrDisplayMenuStateSnapshot &operator=(AsrDisplayMenuStateSnapshot &&other) noexcept;

  bool valid() const;
  bool Add(std::string_view provider_id, std::string_view kind,
           std::string_view item_id, std::string_view display_title,
           std::string_view model_value);
  std::optional<AsrDisplayMenuState> state() const;
  std::optional<AsrDisplayMenuPresentation> presentation(std::size_t index) const;
  std::optional<AsrDisplayMenuItem> item(std::size_t index) const;
  const ::VinputFcitxAsrDisplaySnapshot *raw_handle() const;

private:
  explicit AsrDisplayMenuStateSnapshot(::VinputFcitxAsrDisplaySnapshot *snapshot);
  ::VinputFcitxAsrDisplaySnapshot *snapshot_ = nullptr;
};

} // namespace vinput_fcitx_bridge
