#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include <fcitx-utils/i18n.h>

#include <algorithm>
#include <clocale>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <mutex>
#include <string>
#include <vector>

#ifndef VINPUT_FCITX_BUILD_LOCALEDIR
#define VINPUT_FCITX_BUILD_LOCALEDIR ""
#endif

#ifndef VINPUT_FCITX_INSTALL_LOCALEDIR
#define VINPUT_FCITX_INSTALL_LOCALEDIR ""
#endif

namespace vinput_fcitx_bridge {
namespace {

std::string NormalizeLocaleName(std::string locale) {
  if (const auto colon = locale.find(':'); colon != std::string::npos) {
    locale.resize(colon);
  }
  if (const auto dot = locale.find('.'); dot != std::string::npos) {
    locale.resize(dot);
  }
  if (const auto modifier = locale.find('@'); modifier != std::string::npos) {
    locale.resize(modifier);
  }
  if (locale.empty() || locale == "C" || locale == "POSIX") {
    return {};
  }
  return locale;
}

std::vector<std::string> LocaleCandidates() {
  std::vector<std::string> candidates;
  const auto add = [&candidates](const char *value) {
    if (value == nullptr || value[0] == '\0') {
      return;
    }
    auto locale = NormalizeLocaleName(value);
    if (locale.empty()) {
      return;
    }
    if (std::find(candidates.begin(), candidates.end(), locale) == candidates.end()) {
      candidates.push_back(locale);
    }
    if (const auto separator = locale.find('_'); separator != std::string::npos) {
      auto language = locale.substr(0, separator);
      if (!language.empty() && std::find(candidates.begin(), candidates.end(),
                                         language) == candidates.end()) {
        candidates.push_back(std::move(language));
      }
    }
  };
  add(std::getenv("LANGUAGE"));
  add(std::getenv("LC_ALL"));
  add(std::getenv("LC_MESSAGES"));
  add(std::getenv("LANG"));
  add(std::setlocale(LC_MESSAGES, nullptr));
  return candidates;
}

bool ContainsCatalog(const std::filesystem::path &locale_root) {
  if (locale_root.empty()) {
    return false;
  }
  std::error_code error;
  for (const auto &locale : LocaleCandidates()) {
    const auto catalog = locale_root / locale / "LC_MESSAGES" /
                         (std::string(kFrontendTranslationDomain) + ".mo");
    if (std::filesystem::is_regular_file(catalog, error) && !error) {
      return true;
    }
    error.clear();
  }
  return false;
}

std::filesystem::path ResolveLocaleRoot() {
  if (const auto *override_dir = std::getenv(kFrontendLocaleOverride);
      override_dir != nullptr && override_dir[0] != '\0') {
    return override_dir;
  }
  std::filesystem::path build_root = VINPUT_FCITX_BUILD_LOCALEDIR;
  if (ContainsCatalog(build_root)) {
    return build_root;
  }
  return VINPUT_FCITX_INSTALL_LOCALEDIR;
}

template <typename Value>
std::string FormatTranslated(std::string_view format, Value value) {
  auto translated = FrontendText(format);
  const int size = std::snprintf(nullptr, 0, translated.c_str(), value);
  if (size <= 0) {
    return translated;
  }
  std::vector<char> buffer(static_cast<std::size_t>(size) + 1U, '\0');
  std::snprintf(buffer.data(), buffer.size(), translated.c_str(), value);
  return std::string(buffer.data(), static_cast<std::size_t>(size));
}

} // namespace

void InitFrontendI18n() {
  static std::once_flag initialized;
  std::call_once(initialized, [] {
    std::setlocale(LC_ALL, "");
    fcitx::registerDomain(kFrontendTranslationDomain, ResolveLocaleRoot());
  });
}

std::string FrontendText(std::string_view message) {
  InitFrontendI18n();
  return fcitx::translateDomain(kFrontendTranslationDomain, std::string(message));
}

std::string FrontendCountText(std::string_view format, std::size_t count) {
  return FormatTranslated(format, count);
}

std::string FrontendValueText(std::string_view format, std::string_view value) {
  return FormatTranslated(format, std::string(value).c_str());
}

std::string FrontendPageText(int current_page, int total_pages) {
  auto translated = FrontendText(" (%d/%d)");
  const int size =
      std::snprintf(nullptr, 0, translated.c_str(), current_page, total_pages);
  if (size <= 0) {
    return translated;
  }
  std::vector<char> buffer(static_cast<std::size_t>(size) + 1U, '\0');
  std::snprintf(buffer.data(), buffer.size(), translated.c_str(), current_page,
                total_pages);
  return std::string(buffer.data(), static_cast<std::size_t>(size));
}

} // namespace vinput_fcitx_bridge
