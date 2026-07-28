#pragma once

#include <cstdint>
#include <filesystem>
#include <memory>

#include <fcitx-config/configuration.h>
#include <fcitx-config/enum.h>
#include <fcitx-config/option.h>
#include <fcitx-utils/key.h>

namespace vinput_fcitx_bridge {

inline constexpr const char *kFrontendConfigPath = "conf/vinput.conf";

enum class TriggerMode : std::uint8_t {
  Tap,
  Hold,
  Both,
};
FCITX_CONFIG_ENUM_NAME(TriggerMode, "Tap", "Hold", "Both")

struct FrontendSettings {
  fcitx::KeyList normal_triggers{fcitx::Key(FcitxKey_Control_R)};
  fcitx::KeyList command_triggers{fcitx::Key(FcitxKey_F10)};
  fcitx::KeyList scene_menu_triggers{fcitx::Key(FcitxKey_Shift_R)};
  fcitx::KeyList asr_menu_triggers{fcitx::Key(FcitxKey_F8)};
  TriggerMode trigger_mode{TriggerMode::Both};

  bool operator==(const FrontendSettings &) const = default;
};

class VinputFrontendConfig final : public fcitx::Configuration {
public:
  explicit VinputFrontendConfig(const FrontendSettings &settings = {});

  const char *typeName() const override {
    return "VinputFrontendConfig";
  }

  FrontendSettings settings() const;

  fcitx::Option<fcitx::KeyList, fcitx::ListConstrain<fcitx::KeyConstrain>>
      normal_triggers;
  fcitx::Option<fcitx::KeyList, fcitx::ListConstrain<fcitx::KeyConstrain>>
      command_triggers;
  fcitx::Option<fcitx::KeyList, fcitx::ListConstrain<fcitx::KeyConstrain>>
      scene_menu_triggers;
  fcitx::Option<fcitx::KeyList, fcitx::ListConstrain<fcitx::KeyConstrain>>
      asr_menu_triggers;
  fcitx::Option<TriggerMode> trigger_mode;
};

FrontendSettings LoadFrontendSettings();
bool SaveFrontendSettings(const FrontendSettings &settings);
FrontendSettings LoadFrontendSettingsFromPath(const std::filesystem::path &path);
bool SaveFrontendSettingsToPath(const FrontendSettings &settings,
                                const std::filesystem::path &path);
std::unique_ptr<VinputFrontendConfig>
BuildFrontendConfig(const FrontendSettings &settings);

} // namespace vinput_fcitx_bridge
