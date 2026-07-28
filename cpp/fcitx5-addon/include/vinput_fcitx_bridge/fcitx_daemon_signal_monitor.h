#pragma once

#include "vinput_fcitx_bridge/fcitx_notifications.h"

#include <fcitx-utils/dbus/bus.h>

#include <functional>
#include <memory>
#include <string>
#include <string_view>

namespace vinput_fcitx_bridge {

struct DaemonNotificationPayload {
  std::string code;
  std::string subject;
  std::string detail;
  std::string raw_message;

  bool empty() const;
  bool operator==(const DaemonNotificationPayload &) const = default;
};

FrontendNotificationKind
ClassifyDaemonNotification(const DaemonNotificationPayload &payload);
std::string RenderDaemonNotification(const DaemonNotificationPayload &payload);

class FcitxDaemonSignalMonitor final {
public:
  using NotificationCallback =
      std::function<void(const DaemonNotificationPayload &payload)>;

  FcitxDaemonSignalMonitor(fcitx::dbus::Bus *bus,
                           NotificationCallback notification_callback);
  ~FcitxDaemonSignalMonitor() = default;

  FcitxDaemonSignalMonitor(const FcitxDaemonSignalMonitor &) = delete;
  FcitxDaemonSignalMonitor &operator=(const FcitxDaemonSignalMonitor &) = delete;
  FcitxDaemonSignalMonitor(FcitxDaemonSignalMonitor &&) = delete;
  FcitxDaemonSignalMonitor &operator=(FcitxDaemonSignalMonitor &&) = delete;

  bool active() const;

private:
  NotificationCallback notification_callback_;
  std::unique_ptr<fcitx::dbus::Slot> notification_slot_;
};

} // namespace vinput_fcitx_bridge
