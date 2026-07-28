#include "vinput_fcitx_bridge/fcitx_notifications.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include <fcitx/addonmanager.h>
#include <fcitx/instance.h>
#include <notifications_public.h>

#include <vector>

namespace vinput_fcitx_bridge {

FrontendNotification BuildFrontendNotification(FrontendNotificationKind kind,
                                               std::string_view body) {
  FrontendNotification notification{
      .app_name = "fcitx5-vinput",
      .icon = "dialog-information",
      .summary = FrontendText("Voice Input"),
      .body = std::string(body),
      .timeout_ms = kInfoNotificationTimeoutMs,
  };
  switch (kind) {
  case FrontendNotificationKind::Info:
    break;
  case FrontendNotificationKind::Warning:
    notification.icon = "dialog-warning";
    notification.timeout_ms = kErrorNotificationTimeoutMs;
    break;
  case FrontendNotificationKind::Error:
    notification.icon = "dialog-error";
    notification.timeout_ms = kErrorNotificationTimeoutMs;
    break;
  }
  return notification;
}

bool SendFrontendNotification(fcitx::Instance *instance,
                              const FrontendNotification &notification,
                              std::FILE *fallback_stream) {
  if (notification.body.empty()) {
    return false;
  }
  if (instance != nullptr) {
    auto *notifications = instance->addonManager().addon("notifications", true);
    if (notifications != nullptr) {
      notifications->call<fcitx::INotifications::sendNotification>(
          notification.app_name, 0, notification.icon, notification.summary,
          notification.body, std::vector<std::string>{}, notification.timeout_ms,
          fcitx::NotificationActionCallback{}, fcitx::NotificationClosedCallback{});
      return true;
    }
  }
  if (fallback_stream != nullptr) {
    std::fprintf(fallback_stream, "vinput: %s: %s\n", notification.summary.c_str(),
                 notification.body.c_str());
    std::fflush(fallback_stream);
  }
  return false;
}

} // namespace vinput_fcitx_bridge
