#include "vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h"

#include "vinput_fcitx_bridge/dbus_contract.h"
#include "vinput_fcitx_bridge/fcitx_i18n.h"
#include "vinput_fcitx_bridge/rust_handle.h"
#include "vinput_fcitx_bridge/rust_string.h"
#include "vinput_fcitx_ffi.h"

#include <fcitx-utils/dbus/matchrule.h>
#include <fcitx-utils/dbus/message.h>

#include <cstdint>
#include <tuple>
#include <type_traits>
#include <utility>
#include <vector>

namespace vinput_fcitx_bridge {
namespace {

constexpr std::string_view kDbusService = "org.freedesktop.DBus";
constexpr std::string_view kDbusPath = "/org/freedesktop/DBus";
constexpr std::string_view kDbusInterface = "org.freedesktop.DBus";
constexpr std::string_view kNameOwnerChanged = "NameOwnerChanged";

template <typename Rule = fcitx::dbus::MatchRule>
Rule SignalMatchRule(std::string service, std::string path, std::string interface,
                     std::string name, std::vector<std::string> argument_match = {}) {
  if constexpr (std::is_constructible_v<Rule, fcitx::dbus::MessageType, std::string,
                                        std::string, std::string, std::string,
                                        std::string, std::vector<std::string>>) {
    return Rule{fcitx::dbus::MessageType::Signal,
                std::move(service),
                {},
                std::move(path),
                std::move(interface),
                std::move(name),
                std::move(argument_match)};
  } else {
    return Rule{std::move(service), std::move(path), std::move(interface),
                std::move(name), std::move(argument_match)};
  }
}

std::string RenderPlan(const VinputFcitxDaemonSignalPlanView &plan) {
  const auto text = CopyRustString(plan.text);
  return plan.translate != 0 ? FrontendText(text) : text;
}

std::pair<FrontendNotificationKind, std::string>
PresentDaemonNotification(std::string_view code, std::string_view subject,
                          std::string_view detail, std::string_view raw_message) {
  const VinputFcitxDaemonNotificationView notification{
      .code = ToRustStringView(code),
      .subject = ToRustStringView(subject),
      .detail = ToRustStringView(detail),
      .raw = ToRustStringView(raw_message),
  };
  VinputFcitxDaemonSignalPlanView plan{};
  if (vinput_fcitx_daemon_notification_plan(&notification, &plan) == 0) {
    return {FrontendNotificationKind::Error, FrontendText("Unknown error.")};
  }
  const auto kind = plan.kind == VINPUT_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_INFO
                        ? FrontendNotificationKind::Info
                        : FrontendNotificationKind::Error;
  return {kind, RenderPlan(plan)};
}

std::unique_ptr<fcitx::dbus::Slot>
AddStringSignalMatch(fcitx::dbus::Bus *bus, std::string_view signal,
                     const std::function<bool(const fcitx::dbus::Message &)> &accept,
                     const std::function<void(std::string_view)> &callback) {
  if (!callback) {
    return nullptr;
  }
  const auto rule =
      SignalMatchRule({}, std::string(dbus::kServiceObjectPath),
                      std::string(dbus::kServiceInterface), std::string(signal));
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

struct DaemonLivePresentationState::Impl {
  using Handle =
      RustOwnedHandle<VinputFcitxDaemonLiveState, vinput_fcitx_daemon_live_state_free>;

  Impl() : state(Handle::Adopt(vinput_fcitx_daemon_live_state_new())) {}

  Handle state;
};

DaemonLivePresentationState::DaemonLivePresentationState()
    : impl_(std::make_unique<Impl>()) {}

DaemonLivePresentationState::~DaemonLivePresentationState() = default;

void DaemonLivePresentationState::Reset() {
  static_cast<void>(
      vinput_fcitx_daemon_live_state_reset(impl_->state.mutable_raw_handle()));
}

void DaemonLivePresentationState::BeginStatus(std::string_view status,
                                              bool command_mode) {
  static_cast<void>(vinput_fcitx_daemon_live_state_begin_status(
      impl_->state.mutable_raw_handle(), RustBytes(status), status.size(),
      static_cast<std::uint8_t>(command_mode)));
}

void DaemonLivePresentationState::UpdateStatus(std::string_view status) {
  static_cast<void>(vinput_fcitx_daemon_live_state_update_status(
      impl_->state.mutable_raw_handle(), RustBytes(status), status.size()));
}

bool DaemonLivePresentationState::UpdatePartial(std::string_view partial_text,
                                                bool recording) {
  return vinput_fcitx_daemon_live_state_update_partial(
             impl_->state.mutable_raw_handle(), RustBytes(partial_text),
             partial_text.size(), static_cast<std::uint8_t>(recording)) != 0;
}

bool DaemonLivePresentationState::CommandMode() const {
  return vinput_fcitx_daemon_live_state_command_mode(impl_->state.raw_handle()) != 0;
}

std::string DaemonLivePresentationState::Preedit() const {
  VinputFcitxDaemonSignalPlanView plan{};
  if (vinput_fcitx_daemon_live_state_preedit_plan(impl_->state.raw_handle(), &plan) ==
      0) {
    return {};
  }
  return RenderPlan(plan);
}

std::string ComposeDaemonStatusPreedit(std::string_view status, bool command_mode,
                                       std::string_view partial_text) {
  const VinputFcitxDaemonStatusView status_view{
      .status = ToRustStringView(status),
      .command_mode = static_cast<std::uint8_t>(command_mode),
      .partial = ToRustStringView(partial_text),
  };
  VinputFcitxDaemonSignalPlanView plan{};
  if (vinput_fcitx_daemon_status_preedit_plan(&status_view, &plan) == 0) {
    return {};
  }
  return RenderPlan(plan);
}

FcitxDaemonSignalMonitor::FcitxDaemonSignalMonitor(fcitx::dbus::Bus *bus,
                                                   DaemonSignalCallbacks callbacks)
    : callbacks_(std::move(callbacks)) {
  if (bus == nullptr) {
    return;
  }

  const auto owner_change_rule = SignalMatchRule(
      std::string(kDbusService), std::string(kDbusPath), std::string(kDbusInterface),
      std::string(kNameOwnerChanged), {std::string(dbus::kServiceBusName)});
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
  const auto notification_rule = SignalMatchRule(
      {}, std::string(dbus::kServiceObjectPath), std::string(dbus::kServiceInterface),
      std::string(dbus::kSignalDaemonNotification));
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

        const auto &[code, subject, detail, raw_message] = wire_payload;
        if (!code.empty() || !subject.empty() || !detail.empty() ||
            !raw_message.empty()) {
          auto [kind, rendered] =
              PresentDaemonNotification(code, subject, detail, raw_message);
          callbacks_.notification(kind, rendered);
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
