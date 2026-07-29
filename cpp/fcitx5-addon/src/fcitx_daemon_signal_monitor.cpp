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
constexpr std::string_view kDbusService = "org.freedesktop.DBus";
constexpr std::string_view kDbusPath = "/org/freedesktop/DBus";
constexpr std::string_view kDbusInterface = "org.freedesktop.DBus";
constexpr std::string_view kNameOwnerChanged = "NameOwnerChanged";

std::unique_ptr<fcitx::dbus::Slot>
AddStringSignalMatch(fcitx::dbus::Bus *bus, std::string_view signal,
                     const std::function<bool(const fcitx::dbus::Message &)> &accept,
                     const std::function<void(std::string_view)> &callback) {
  if (!callback) {
    return nullptr;
  }
  const fcitx::dbus::MatchRule rule{fcitx::dbus::MessageType::Signal,
                                    {},
                                    {},
                                    std::string(dbus::kServiceObjectPath),
                                    std::string(dbus::kServiceInterface),
                                    std::string(signal)};
  return bus->addMatch(rule, [accept, callback](fcitx::dbus::Message &message) {
    if (!accept(message)) {
      return true;
    }
    std::string value;
    message >> value;
    if (message) {
      callback(value);
    }
    return true;
  });
}

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

std::string ComposeDaemonStatusPreedit(std::string_view status, bool command_mode,
                                       std::string_view partial_text) {
  if (!partial_text.empty()) {
    return std::string(partial_text);
  }
  if (status == dbus::kStatusRecording) {
    return FrontendText(command_mode ? "... Commanding ..." : "... Recording ...");
  }
  if (status == dbus::kStatusInferring) {
    return FrontendText("... Recognizing ...");
  }
  if (status == dbus::kStatusPostprocessing) {
    return FrontendText("... Postprocessing ...");
  }
  return {};
}

FcitxDaemonSignalMonitor::FcitxDaemonSignalMonitor(fcitx::dbus::Bus *bus,
                                                   DaemonSignalCallbacks callbacks)
    : callbacks_(std::move(callbacks)) {
  if (bus == nullptr) {
    return;
  }

  const fcitx::dbus::MatchRule owner_change_rule{fcitx::dbus::MessageType::Signal,
                                                 std::string(kDbusService),
                                                 {},
                                                 std::string(kDbusPath),
                                                 std::string(kDbusInterface),
                                                 std::string(kNameOwnerChanged),
                                                 {std::string(dbus::kServiceBusName)}};
  owner_change_slot_ =
      bus->addMatch(owner_change_rule, [this](fcitx::dbus::Message &message) {
        std::tuple<std::string, std::string, std::string> owners;
        message >> owners;
        if (message && std::get<0>(owners) == dbus::kServiceBusName) {
          UpdateServiceOwner(std::get<2>(owners));
        }
        return true;
      });
  const auto accept = [this](const fcitx::dbus::Message &message) {
    return AcceptSignal(message);
  };
  status_slot_ = AddStringSignalMatch(bus, dbus::kSignalStatusChanged, accept,
                                      callbacks_.status_changed);
  partial_slot_ = AddStringSignalMatch(bus, dbus::kSignalRecognitionPartial, accept,
                                       callbacks_.recognition_partial);
  if (!callbacks_.notification) {
    return;
  }
  const fcitx::dbus::MatchRule notification_rule{
      fcitx::dbus::MessageType::Signal,
      {},
      {},
      std::string(dbus::kServiceObjectPath),
      std::string(dbus::kServiceInterface),
      std::string(dbus::kSignalDaemonNotification)};
  notification_slot_ =
      bus->addMatch(notification_rule, [this](fcitx::dbus::Message &message) {
        if (!AcceptSignal(message)) {
          return true;
        }
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
          callbacks_.notification(payload);
        }
        return true;
      });
  UpdateServiceOwner(bus->serviceOwner(std::string(dbus::kServiceBusName), 100'000));
}

bool FcitxDaemonSignalMonitor::AcceptSignal(const fcitx::dbus::Message &message) const {
  return !service_owner_.empty() && message.sender() == service_owner_;
}

void FcitxDaemonSignalMonitor::UpdateServiceOwner(std::string_view owner) {
  if (service_owner_ == owner) {
    return;
  }
  service_owner_ = owner;
  if (callbacks_.service_availability_changed) {
    callbacks_.service_availability_changed(!service_owner_.empty());
  }
}

bool FcitxDaemonSignalMonitor::active() const {
  return owner_change_slot_ != nullptr && status_slot_ != nullptr &&
         partial_slot_ != nullptr && notification_slot_ != nullptr;
}

} // namespace vinput_fcitx_bridge
