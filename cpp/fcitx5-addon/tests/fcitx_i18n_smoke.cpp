#include "vinpst_fcitx_bridge/fcitx_i18n.h"

#include <cassert>

int main() {
  using vinpst_fcitx_bridge::FrontendCountText;
  using vinpst_fcitx_bridge::FrontendPageText;
  using vinpst_fcitx_bridge::FrontendText;
  using vinpst_fcitx_bridge::FrontendValueText;
  using vinpst_fcitx_bridge::InitFrontendI18n;

  InitFrontendI18n();
  assert(FrontendText("Scenes /filter") == "场景 /过滤");
  assert(FrontendText("Models /filter") == "模型 /过滤");
  assert(FrontendText("Current: ") == "当前：");
  assert(FrontendText("Original") == "原文");
  assert(FrontendText("Voice Command") == "语音命令");
  assert(FrontendCountText("Choose Result (%zu)", 6) == "选择结果（6项）");
  assert(FrontendPageText(2, 4) == "（2/4页）");
  assert(FrontendText("Voice Input") == "语音输入");
  assert(FrontendText("Tap") == "单击");
  assert(FrontendText("Hold") == "长按");
  assert(FrontendText("Both") == "两者");
  assert(FrontendText("Unknown error.") == "未知错误。");
  assert(FrontendText("... Recording ...") == "... 正在录音 ...");
  assert(FrontendText("... Commanding ...") == "... 正在命令 ...");
  assert(FrontendText("... Recognizing ...") == "... 正在识别 ...");
  assert(FrontendText("... Postprocessing ...") == "... 正在后处理 ...");
  assert(FrontendValueText("Switched scene to '%s'.", "工作") ==
         "已切换场景到“工作”。");
  assert(FrontendValueText("ASR switch requested for '%s'.", "月光") ==
         "已请求切换语音识别到“月光”。");
  assert(FrontendText("translation-fallback") == "translation-fallback");
  return 0;
}
