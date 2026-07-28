#include "vinput_fcitx_bridge/fcitx_config.h"

#include <fcitx-config/iniparser.h>
#include <fcitx-utils/standardpaths.h>

namespace vinput_fcitx_bridge {
namespace {

fcitx::ListConstrain<fcitx::KeyConstrain> TriggerConstrain() {
  return fcitx::KeyListConstrain(fcitx::KeyConstrainFlags{
      fcitx::KeyConstrainFlag::AllowModifierOnly,
      fcitx::KeyConstrainFlag::AllowModifierLess,
  });
}

} // namespace

VinputFrontendConfig::VinputFrontendConfig(const FrontendSettings &settings)
    : normal_triggers(this, "TriggerKey", "Normal Dictation Keys",
                      settings.normal_triggers, TriggerConstrain()),
      command_triggers(this, "CommandKeys", "Command Dictation Keys",
                       settings.command_triggers, TriggerConstrain()),
      scene_menu_triggers(this, "SceneMenuKey", "Scene Menu Keys",
                          settings.scene_menu_triggers, TriggerConstrain()),
      asr_menu_triggers(this, "AsrMenuKey", "ASR Menu Keys", settings.asr_menu_triggers,
                        TriggerConstrain()),
      trigger_mode(this, "TriggerMode", "Trigger Mode", settings.trigger_mode) {}

FrontendSettings VinputFrontendConfig::settings() const {
  return FrontendSettings{
      .normal_triggers = normal_triggers.value(),
      .command_triggers = command_triggers.value(),
      .scene_menu_triggers = scene_menu_triggers.value(),
      .asr_menu_triggers = asr_menu_triggers.value(),
      .trigger_mode = trigger_mode.value(),
  };
}

FrontendSettings LoadFrontendSettings() {
  VinputFrontendConfig config;
  fcitx::readAsIni(config, fcitx::StandardPathsType::PkgConfig, kFrontendConfigPath);
  return config.settings();
}

bool SaveFrontendSettings(const FrontendSettings &settings) {
  VinputFrontendConfig config(settings);
  fcitx::RawConfig raw;
  fcitx::readAsIni(raw, fcitx::StandardPathsType::PkgConfig, kFrontendConfigPath);
  config.save(raw);
  return fcitx::safeSaveAsIni(raw, fcitx::StandardPathsType::PkgConfig,
                              kFrontendConfigPath);
}

FrontendSettings LoadFrontendSettingsFromPath(const std::filesystem::path &path) {
  VinputFrontendConfig config;
  fcitx::readAsIni(config, path.string());
  return config.settings();
}

bool SaveFrontendSettingsToPath(const FrontendSettings &settings,
                                const std::filesystem::path &path) {
  VinputFrontendConfig config(settings);
  fcitx::RawConfig raw;
  fcitx::readAsIni(raw, path.string());
  config.save(raw);
  return fcitx::safeSaveAsIni(raw, path.string());
}

std::unique_ptr<VinputFrontendConfig>
BuildFrontendConfig(const FrontendSettings &settings) {
  return std::make_unique<VinputFrontendConfig>(settings);
}

} // namespace vinput_fcitx_bridge
