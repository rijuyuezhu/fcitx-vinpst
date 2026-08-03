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

class SceneStateSnapshot {
public:
  SceneStateSnapshot() = default;
  explicit SceneStateSnapshot(std::string_view active_scene_id);
  ~SceneStateSnapshot();

  SceneStateSnapshot(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot &operator=(const SceneStateSnapshot &) = delete;
  SceneStateSnapshot(SceneStateSnapshot &&other) noexcept;
  SceneStateSnapshot &operator=(SceneStateSnapshot &&other) noexcept;

  bool valid() const;
  bool Add(std::string_view id, std::string_view label);
  bool SetActive(std::string_view active_scene_id);
  std::string active_scene_id() const;
  std::string active_label() const;
  std::size_t size() const;
  std::optional<SceneStateItem> item(std::size_t index) const;
  const ::VinputFcitxSceneSnapshot *raw_handle() const;

private:
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

class AsrDisplayMenuStateSnapshot {
public:
  AsrDisplayMenuStateSnapshot() = default;
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
  std::string target_provider_id() const;
  std::string target_model_id() const;
  std::string effective_provider_id() const;
  std::string effective_model_id() const;
  bool reload_in_progress() const;
  std::string last_error() const;
  std::string effective_base_label() const;
  std::string target_base_label() const;
  std::size_t size() const;
  std::optional<AsrDisplayMenuPresentation> presentation(std::size_t index) const;
  std::optional<AsrDisplayMenuItem> item(std::size_t index) const;
  const ::VinputFcitxAsrDisplaySnapshot *raw_handle() const;

private:
  ::VinputFcitxAsrDisplaySnapshot *snapshot_ = nullptr;
};

} // namespace vinput_fcitx_bridge
