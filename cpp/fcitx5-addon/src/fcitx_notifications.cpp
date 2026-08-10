#include "vinpst_fcitx_bridge/fcitx_notifications.h"

#include "vinpst_fcitx_bridge/fcitx_i18n.h"
#include "vinpst_fcitx_bridge/rust_string.h"
#include "vinpst_fcitx_ffi.h"

#include <fcitx/addonmanager.h>
#include <fcitx/instance.h>
#include <notifications_public.h>

#include <vector>

namespace vinpst_fcitx_bridge {

FrontendNotification BuildFrontendNotification(FrontendNotificationKind kind,
                                               std::string_view body) {
  FrontendNotification notification{
      .app_name = "fcitx5-vinpst",
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

std::pair<FrontendNotificationKind, std::string>
PlanStructuredDaemonNotification(std::string_view code, std::string_view subject,
                                 std::string_view detail,
                                 std::string_view raw_message) {
  const VinpstFcitxDaemonNotificationView notification{
      .code = ToRustStringView(code),
      .subject = ToRustStringView(subject),
      .detail = ToRustStringView(detail),
      .raw = ToRustStringView(raw_message),
  };
  VinpstFcitxDaemonSignalPlanView plan{};
  if (vinpst_fcitx_daemon_notification_plan(&notification, &plan) == 0) {
    return {FrontendNotificationKind::Error, FrontendText("Unknown error.")};
  }
  const auto kind = plan.kind == VINPST_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_INFO
                        ? FrontendNotificationKind::Info
                        : FrontendNotificationKind::Error;
  auto text = CopyRustString(plan.text);
  if (plan.translate != 0) {
    text = FrontendText(text);
  }
  return {kind, std::move(text)};
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
    std::fprintf(fallback_stream, "vinpst: %s: %s\n", notification.summary.c_str(),
                 notification.body.c_str());
    std::fflush(fallback_stream);
  }
  return false;
}

} // namespace vinpst_fcitx_bridge
