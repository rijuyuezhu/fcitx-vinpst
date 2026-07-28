#include "vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h"

#include "vinput_fcitx_bridge/dbus_contract.h"
#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include <fcitx-utils/dbus/matchrule.h>
#include <fcitx-utils/dbus/message.h>

#include <tuple>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr std::string_view kUnknownErrorCode = "unknown";

} // namespace

bool DaemonNotificationPayload::empty() const {
  return code.empty() && subject.empty() && detail.empty() && raw_message.empty();
}

FrontendNotificationKind
ClassifyDaemonNotification(const DaemonNotificationPayload &payload) {
  if ((!payload.code.empty() && payload.code != kUnknownErrorCode) ||
      !payload.subject.empty() || !payload.detail.empty()) {
    return FrontendNotificationKind::Error;
  }
  return FrontendNotificationKind::Info;
}

std::string RenderDaemonNotification(const DaemonNotificationPayload &payload) {
  if (!payload.raw_message.empty()) {
    return payload.raw_message;
  }
  if (!payload.detail.empty()) {
    return payload.detail;
  }
  if (!payload.subject.empty()) {
    return payload.subject;
  }
  if (!payload.code.empty() && payload.code != kUnknownErrorCode) {
    return payload.code;
  }
  return FrontendText("Unknown error.");
}

FcitxDaemonSignalMonitor::FcitxDaemonSignalMonitor(
    fcitx::dbus::Bus *bus, NotificationCallback notification_callback)
    : notification_callback_(std::move(notification_callback)) {
  if (bus == nullptr || !notification_callback_) {
    return;
  }

  const fcitx::dbus::MatchRule rule{std::string(dbus::kServiceBusName),
                                    std::string(dbus::kServiceObjectPath),
                                    std::string(dbus::kServiceInterface),
                                    std::string(dbus::kSignalDaemonNotification)};
  notification_slot_ = bus->addMatch(rule, [this](fcitx::dbus::Message &message) {
    std::tuple<std::string, std::string, std::string, std::string> wire_payload;
    message >> wire_payload;
    if (!message) {
      return true;
    }

    auto payload = DaemonNotificationPayload{
        .code = std::move(std::get<0>(wire_payload)),
        .subject = std::move(std::get<1>(wire_payload)),
        .detail = std::move(std::get<2>(wire_payload)),
        .raw_message = std::move(std::get<3>(wire_payload)),
    };
    if (!payload.empty()) {
      notification_callback_(payload);
    }
    return true;
  });
}

bool FcitxDaemonSignalMonitor::active() const {
  return notification_slot_ != nullptr;
}

} // namespace vinput_fcitx_bridge
