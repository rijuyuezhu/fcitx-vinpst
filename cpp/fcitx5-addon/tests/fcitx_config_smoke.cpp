#include "vinput_fcitx_bridge/fcitx_config.h"

#include <cassert>
#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

namespace {

std::filesystem::path UniqueConfigPath() {
  auto root = std::filesystem::temp_directory_path() /
              ("vinput-fcitx-config-smoke-" + std::to_string(getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  return root / "vinput.conf";
}

bool Contains(const std::string &text, const std::string &needle) {
  return text.find(needle) != std::string::npos;
}

} // namespace

int main() {
  using vinput_fcitx_bridge::FrontendSettings;
  using vinput_fcitx_bridge::LoadFrontendSettingsFromPath;
  using vinput_fcitx_bridge::SaveFrontendSettingsToPath;
  using vinput_fcitx_bridge::VinputFrontendConfig;

  const auto path = UniqueConfigPath();
  const auto defaults = LoadFrontendSettingsFromPath(path);
  assert(defaults == FrontendSettings{});

  FrontendSettings settings;
  settings.normal_triggers = {fcitx::Key(FcitxKey_F6), fcitx::Key(FcitxKey_F7)};
  settings.command_triggers = {fcitx::Key(FcitxKey_F9)};
  settings.scene_menu_triggers = {fcitx::Key(FcitxKey_Shift_R)};
  settings.asr_menu_triggers = {fcitx::Key(FcitxKey_F8), fcitx::Key("Control+F8")};
  settings.trigger_mode = vinput_fcitx_bridge::TriggerMode::Hold;
  assert(SaveFrontendSettingsToPath(settings, path));
  assert(LoadFrontendSettingsFromPath(path) == settings);

  std::ifstream input(path);
  const std::string contents((std::istreambuf_iterator<char>(input)),
                             std::istreambuf_iterator<char>());
  assert(Contains(contents, "[TriggerKey]"));
  assert(Contains(contents, "[CommandKeys]"));
  assert(Contains(contents, "[SceneMenuKey]"));
  assert(Contains(contents, "[AsrMenuKey]"));
  assert(Contains(contents, "F6"));
  assert(Contains(contents, "F7"));
  assert(Contains(contents, "Control+F8"));
  assert(Contains(contents, "TriggerMode=Hold"));

  {
    std::ofstream rewrite(path, std::ios::trunc);
    rewrite << contents << "\n[PagePrevKey]\n0=Page_Up\n";
  }
  settings.command_triggers = {fcitx::Key(FcitxKey_F10)};
  assert(SaveFrontendSettingsToPath(settings, path));
  std::ifstream merged_input(path);
  const std::string merged_contents((std::istreambuf_iterator<char>(merged_input)),
                                    std::istreambuf_iterator<char>());
  assert(Contains(merged_contents, "TriggerMode=Hold"));
  assert(Contains(merged_contents, "[PagePrevKey]"));
  assert(Contains(merged_contents, "0=Page_Up"));
  assert(LoadFrontendSettingsFromPath(path) == settings);

  VinputFrontendConfig config(settings);
  fcitx::RawConfig raw;
  config.save(raw);
  VinputFrontendConfig roundtrip;
  roundtrip.load(raw, true);
  assert(roundtrip.settings() == settings);

  std::filesystem::remove_all(path.parent_path());
  return 0;
}
