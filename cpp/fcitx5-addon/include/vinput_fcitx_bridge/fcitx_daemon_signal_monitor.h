#pragma once

#include "vinput_fcitx_bridge/fcitx_notifications.h"

#include <fcitx-utils/dbus/bus.h>

#include <cstdint>
#include <functional>
#include <memory>
#include <string>
#include <string_view>

namespace vinput_fcitx_bridge {

std::string ComposeDaemonStatusPreedit(std::string_view status, bool command_mode,
                                       std::string_view partial_text);

class DaemonLivePresentationState final {
public:
  DaemonLivePresentationState();
  ~DaemonLivePresentationState();

  DaemonLivePresentationState(const DaemonLivePresentationState &) = delete;
  DaemonLivePresentationState &operator=(const DaemonLivePresentationState &) = delete;
  DaemonLivePresentationState(DaemonLivePresentationState &&) = delete;
  DaemonLivePresentationState &operator=(DaemonLivePresentationState &&) = delete;

  void Reset();
  void BeginStatus(std::string_view status, bool command_mode);
  void UpdateStatus(std::string_view status);
  bool UpdatePartial(std::string_view partial_text, bool recording);
  bool CommandMode() const;
  std::string Preedit() const;

private:
  struct Impl;

  std::unique_ptr<Impl> impl_;
};

struct DaemonSignalCallbacks {
  std::function<void(bool available)> service_availability_changed;
  std::function<void(std::string_view status)> status_changed;
  std::function<void(std::string_view partial_text)> recognition_partial;
  std::function<void(FrontendNotificationKind kind, std::string_view message)>
      notification;
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
  bool AcceptSignal(const fcitx::dbus::Message &message) const;
  void UpdateServiceOwner(std::string_view owner);

  DaemonSignalCallbacks callbacks_;
  std::string service_owner_;
  std::unique_ptr<fcitx::dbus::Slot> owner_change_slot_;
  std::unique_ptr<fcitx::dbus::Slot> status_slot_;
  std::unique_ptr<fcitx::dbus::Slot> partial_slot_;
  std::unique_ptr<fcitx::dbus::Slot> notification_slot_;
};

} // namespace vinput_fcitx_bridge
