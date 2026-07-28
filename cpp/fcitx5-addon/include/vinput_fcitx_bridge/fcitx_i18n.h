#pragma once

#include <cstddef>
#include <string>
#include <string_view>

namespace vinput_fcitx_bridge {

inline constexpr const char *kFrontendTranslationDomain = "fcitx5-vinput";
inline constexpr const char *kFrontendLocaleOverride = "VINPUT_FCITX_LOCALEDIR";

void InitFrontendI18n();
std::string FrontendText(std::string_view message);
std::string FrontendCountText(std::string_view format, std::size_t count);
std::string FrontendPageText(int current_page, int total_pages);

} // namespace vinput_fcitx_bridge
