#include "vinput_fcitx_bridge/fcitx_config.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include <fcitx-config/iniparser.h>

#if __has_include(<fcitx-utils/standardpaths.h>)
#include <fcitx-utils/standardpaths.h>
#define VINPUT_FCITX_HAS_STANDARD_PATHS 1
#else
#include <fcitx-utils/standardpath.h>
#define VINPUT_FCITX_HAS_STANDARD_PATHS 0
#endif

namespace vinput_fcitx_bridge {
namespace {

#if VINPUT_FCITX_HAS_STANDARD_PATHS
constexpr auto kFrontendConfigPathType = fcitx::StandardPathsType::PkgConfig;
#else
constexpr auto kFrontendConfigPathType = fcitx::StandardPath::Type::PkgConfig;
#endif

fcitx::ListConstrain<fcitx::KeyConstrain> TriggerConstrain() {
  return fcitx::KeyListConstrain(fcitx::KeyConstrainFlags{
      fcitx::KeyConstrainFlag::AllowModifierOnly,
      fcitx::KeyConstrainFlag::AllowModifierLess,
  });
}

} // namespace

VinputFrontendConfig::VinputFrontendConfig(const FrontendSettings &settings)
    : normal_triggers(this, "TriggerKey", FrontendText("Normal Dictation Keys"),
                      settings.normal_triggers, TriggerConstrain()),
      command_triggers(this, "CommandKeys", FrontendText("Command Dictation Keys"),
                       settings.command_triggers, TriggerConstrain()),
      scene_menu_triggers(this, "SceneMenuKey", FrontendText("Scene Menu Keys"),
                          settings.scene_menu_triggers, TriggerConstrain()),
      asr_menu_triggers(this, "AsrMenuKey", FrontendText("ASR Menu Keys"),
                        settings.asr_menu_triggers, TriggerConstrain()),
      page_prev_keys(this, "PagePrevKeys", FrontendText("Previous Page Keys"),
                     settings.page_prev_keys, TriggerConstrain()),
      page_next_keys(this, "PageNextKeys", FrontendText("Next Page Keys"),
                     settings.page_next_keys, TriggerConstrain()),
      trigger_mode(this, "TriggerMode", FrontendText("Trigger Mode"),
                   settings.trigger_mode) {}

FrontendSettings VinputFrontendConfig::settings() const {
  return FrontendSettings{
      .normal_triggers = normal_triggers.value(),
      .command_triggers = command_triggers.value(),
      .scene_menu_triggers = scene_menu_triggers.value(),
      .asr_menu_triggers = asr_menu_triggers.value(),
      .page_prev_keys = page_prev_keys.value(),
      .page_next_keys = page_next_keys.value(),
      .trigger_mode = trigger_mode.value(),
  };
}

FrontendSettings LoadFrontendSettings() {
  VinputFrontendConfig config;
  fcitx::readAsIni(config, kFrontendConfigPathType, kFrontendConfigPath);
  return config.settings();
}

bool SaveFrontendSettings(const FrontendSettings &settings) {
  VinputFrontendConfig config(settings);
  fcitx::RawConfig raw;
  fcitx::readAsIni(raw, kFrontendConfigPathType, kFrontendConfigPath);
  config.save(raw);
  return fcitx::safeSaveAsIni(raw, kFrontendConfigPathType, kFrontendConfigPath);
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
