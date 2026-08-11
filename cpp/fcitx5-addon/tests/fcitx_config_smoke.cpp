#include "vinpst_fcitx_bridge/fcitx_config.h"
#include "vinpst_fcitx_bridge/fcitx_i18n.h"

#include <cassert>
#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

namespace {

std::filesystem::path UniqueConfigRoot() {
  auto root = std::filesystem::temp_directory_path() /
              ("vinpst-fcitx-config-smoke-" + std::to_string(getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  return root;
}

bool Contains(const std::string &text, const std::string &needle) {
  return text.find(needle) != std::string::npos;
}

bool HasValueByPath(const fcitx::RawConfig &config, const std::string &path,
                    const std::string &expected) {
  const auto *value = config.valueByPath(path);
  return value != nullptr && *value == expected;
}

} // namespace

int main() {
  using vinpst_fcitx_bridge::FrontendSettings;
  using vinpst_fcitx_bridge::InitFrontendI18n;
  using vinpst_fcitx_bridge::kFrontendConfigPath;
  using vinpst_fcitx_bridge::LoadFrontendSettings;
  using vinpst_fcitx_bridge::SaveFrontendSettings;
  using vinpst_fcitx_bridge::VinpstFrontendConfig;

  const auto root = UniqueConfigRoot();
  const auto system_root = root / "system";
  std::filesystem::create_directories(system_root);
  assert(setenv("XDG_CONFIG_HOME", root.c_str(), 1) == 0);
  assert(setenv("XDG_CONFIG_DIRS", system_root.c_str(), 1) == 0);
  const auto path = root / "fcitx5" / kFrontendConfigPath;

  InitFrontendI18n();
  const auto defaults = LoadFrontendSettings();
  assert(defaults == FrontendSettings{});

  FrontendSettings settings;
  settings.normal_triggers = {fcitx::Key(FcitxKey_F6), fcitx::Key(FcitxKey_F7)};
  settings.command_triggers = {fcitx::Key(FcitxKey_F9)};
  settings.scene_menu_triggers = {fcitx::Key(FcitxKey_Shift_R)};
  settings.asr_menu_triggers = {fcitx::Key(FcitxKey_F8), fcitx::Key("Control+F8")};
  settings.page_prev_keys = {fcitx::Key(FcitxKey_F5), fcitx::Key(FcitxKey_KP_Page_Up)};
  settings.page_next_keys = {fcitx::Key(FcitxKey_F6), fcitx::Key(FcitxKey_KP_Next)};
  settings.trigger_mode = vinpst_fcitx_bridge::TriggerMode::Hold;
  assert(SaveFrontendSettings(settings));
  assert(LoadFrontendSettings() == settings);

  std::ifstream input(path);
  const std::string contents((std::istreambuf_iterator<char>(input)),
                             std::istreambuf_iterator<char>());
  assert(Contains(contents, "[TriggerKey]"));
  assert(Contains(contents, "[CommandKeys]"));
  assert(Contains(contents, "[SceneMenuKey]"));
  assert(Contains(contents, "[AsrMenuKey]"));
  assert(Contains(contents, "[PagePrevKeys]"));
  assert(Contains(contents, "[PageNextKeys]"));
  assert(Contains(contents, "F6"));
  assert(Contains(contents, "F7"));
  assert(Contains(contents, "Control+F8"));
  assert(Contains(contents, "KP_Page_Up"));
  assert(Contains(contents, "KP_Next"));
  assert(Contains(contents, "TriggerMode=Hold"));

  {
    std::ofstream rewrite(path, std::ios::trunc);
    rewrite << "LegacySearchMode=enabled\n\n"
            << contents << "\n[LegacySearchKeys]\n0=Control+F\n";
  }
  settings.command_triggers = {fcitx::Key(FcitxKey_F10)};
  assert(SaveFrontendSettings(settings));
  std::ifstream merged_input(path);
  const std::string merged_contents((std::istreambuf_iterator<char>(merged_input)),
                                    std::istreambuf_iterator<char>());
  assert(Contains(merged_contents, "TriggerMode=Hold"));
  assert(Contains(merged_contents, "LegacySearchMode=enabled"));
  assert(Contains(merged_contents, "[LegacySearchKeys]"));
  assert(Contains(merged_contents, "0=Control+F"));
  assert(LoadFrontendSettings() == settings);

  VinpstFrontendConfig config(settings);
  fcitx::RawConfig raw;
  config.save(raw);
  VinpstFrontendConfig roundtrip;
  roundtrip.load(raw, true);
  assert(roundtrip.settings() == settings);

  fcitx::RawConfig description;
  config.dumpDescription(description);
  constexpr auto prefix = "VinpstFrontendConfig/TriggerMode/";
  assert(HasValueByPath(description, std::string(prefix) + "Enum/0", "Tap"));
  assert(HasValueByPath(description, std::string(prefix) + "Enum/1", "Hold"));
  assert(HasValueByPath(description, std::string(prefix) + "Enum/2", "Both"));
  assert(HasValueByPath(description, std::string(prefix) + "EnumI18n/0", "单击"));
  assert(HasValueByPath(description, std::string(prefix) + "EnumI18n/1", "长按"));
  assert(HasValueByPath(description, std::string(prefix) + "EnumI18n/2", "两者"));
  constexpr auto gui_prefix = "VinpstFrontendConfig/ManagementGui/";
  assert(HasValueByPath(description, std::string(gui_prefix) + "Type", "External"));
  assert(
      HasValueByPath(description, std::string(gui_prefix) + "External", "vinpst-gui"));

  std::filesystem::remove_all(root);
  return 0;
}
