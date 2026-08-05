#pragma once

#include <cstdint>
#include <cstdio>
#include <string>
#include <string_view>

namespace fcitx {
class Instance;
}

namespace vinpst_fcitx_bridge {

inline constexpr int kInfoNotificationTimeoutMs = 3000;
inline constexpr int kErrorNotificationTimeoutMs = 5000;

enum class FrontendNotificationKind : std::uint8_t {
  Info,
  Warning,
  Error,
};

struct FrontendNotification {
  std::string app_name;
  std::string icon;
  std::string summary;
  std::string body;
  int timeout_ms = 0;
};

FrontendNotification BuildFrontendNotification(FrontendNotificationKind kind,
                                               std::string_view body);
bool SendFrontendNotification(fcitx::Instance *instance,
                              const FrontendNotification &notification,
                              std::FILE *fallback_stream = stderr);

} // namespace vinpst_fcitx_bridge
