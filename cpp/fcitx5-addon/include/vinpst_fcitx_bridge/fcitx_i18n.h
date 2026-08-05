#pragma once

#include <cstddef>
#include <string>
#include <string_view>

namespace vinpst_fcitx_bridge {

inline constexpr const char *kFrontendTranslationDomain = "fcitx5-vinpst";
inline constexpr const char *kFrontendLocaleOverride = "VINPST_FCITX_LOCALEDIR";

void InitFrontendI18n();
std::string FrontendText(std::string_view message);
std::string FrontendCountText(std::string_view format, std::size_t count);
std::string FrontendValueText(std::string_view format, std::string_view value);
std::string FrontendPageText(int current_page, int total_pages);

} // namespace vinpst_fcitx_bridge
