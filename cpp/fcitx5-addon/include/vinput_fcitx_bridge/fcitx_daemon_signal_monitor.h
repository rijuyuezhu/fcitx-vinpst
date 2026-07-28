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
std::string ComposeDaemonStatusPreedit(std::string_view status, bool command_mode,
                                       std::string_view partial_text);

struct DaemonSignalCallbacks {
  std::function<void(std::string_view status)> status_changed;
  std::function<void(std::string_view partial_text)> recognition_partial;
  std::function<void(const DaemonNotificationPayload &payload)> notification;
};

class FcitxDaemonSignalMonitor final {
public:
  FcitxDaemonSignalMonitor(fcitx::dbus::Bus *bus, DaemonSignalCallbacks callbacks);
  ~FcitxDaemonSignalMonitor() = default;

  FcitxDaemonSignalMonitor(const FcitxDaemonSignalMonitor &) = delete;
  FcitxDaemonSignalMonitor &operator=(const FcitxDaemonSignalMonitor &) = delete;
  FcitxDaemonSignalMonitor(FcitxDaemonSignalMonitor &&) = delete;
  FcitxDaemonSignalMonitor &operator=(FcitxDaemonSignalMonitor &&) = delete;

  bool active() const;

private:
  DaemonSignalCallbacks callbacks_;
  std::unique_ptr<fcitx::dbus::Slot> status_slot_;
  std::unique_ptr<fcitx::dbus::Slot> partial_slot_;
  std::unique_ptr<fcitx::dbus::Slot> notification_slot_;
};

} // namespace vinput_fcitx_bridge
