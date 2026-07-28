#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include <cassert>

int main() {
  using vinput_fcitx_bridge::FrontendCountText;
  using vinput_fcitx_bridge::FrontendPageText;
  using vinput_fcitx_bridge::FrontendText;
  using vinput_fcitx_bridge::InitFrontendI18n;

  InitFrontendI18n();
  assert(FrontendText("Scenes /filter") == "场景 /过滤");
  assert(FrontendText("Models /filter") == "模型 /过滤");
  assert(FrontendText("Current: ") == "当前：");
  assert(FrontendText("Original") == "原文");
  assert(FrontendText("Voice Command") == "语音命令");
  assert(FrontendCountText("Choose Result (%zu)", 6) == "选择结果（6项）");
  assert(FrontendPageText(2, 4) == "（2/4页）");
  assert(FrontendText("translation-fallback") == "translation-fallback");
  return 0;
}
