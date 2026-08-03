#include "vinput_fcitx_bridge/dbus_contract.h"
#include "vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h"

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>

#include <cassert>
#include <cstdint>
#include <string>
#include <utility>
#include <vector>

namespace {

struct WireNotification {
  std::string code;
  std::string subject;
  std::string detail;
  std::string raw_message;
};

} // namespace

int main() {
  using fcitx::dbus::Bus;
  using fcitx::dbus::BusType;
  using fcitx::dbus::RequestNameFlag;
  using vinput_fcitx_bridge::ComposeDaemonStatusPreedit;
  using vinput_fcitx_bridge::DaemonSignalCallbacks;
  using vinput_fcitx_bridge::FcitxDaemonSignalMonitor;
  using vinput_fcitx_bridge::FrontendNotificationKind;
  namespace dbus = vinput_fcitx_bridge::dbus;

  assert(ComposeDaemonStatusPreedit("recording", false, "") == "... Recording ...");
  assert(ComposeDaemonStatusPreedit("recording", true, "") == "... Commanding ...");
  assert(ComposeDaemonStatusPreedit("inferring", false, "") == "... Recognizing ...");
  assert(ComposeDaemonStatusPreedit("postprocessing", false, "") ==
         "... Postprocessing ...");
  assert(ComposeDaemonStatusPreedit("idle", false, "").empty());
  assert(ComposeDaemonStatusPreedit("recording", false, "live partial") ==
         "live partial");

  const WireNotification info{
      .code = "unknown",
      .subject = "",
      .detail = "",
      .raw_message = "registry cache refreshed",
  };

  const WireNotification error{
      .code = "asr_backend_reload_failed",
      .subject = "sherpa-onnx",
      .detail = "model metadata is invalid",
      .raw_message = "",
  };

  fcitx::EventLoop loop;
  Bus receiver(BusType::Session);
  Bus sender(BusType::Session);
  assert(receiver.isOpen());
  assert(sender.isOpen());
  receiver.attachEventLoop(&loop);

  const std::string object_path(dbus::kServiceObjectPath);
  const std::string interface(dbus::kServiceInterface);
  std::vector<bool> service_availability;
  std::vector<std::string> statuses;
  std::vector<std::string> partials;
  std::vector<std::pair<FrontendNotificationKind, std::string>> notifications;
  auto finish_when_complete = [&] {
    if (service_availability.size() == 2 && statuses.size() == 1 &&
        partials.size() == 1 && notifications.size() == 2) {
      loop.exit();
    }
  };
  FcitxDaemonSignalMonitor monitor(
      &receiver, DaemonSignalCallbacks{
                     .service_availability_changed =
                         [&](bool available) {
                           service_availability.push_back(available);
                           finish_when_complete();
                         },
                     .status_changed =
                         [&](std::string_view status) {
                           statuses.emplace_back(status);
                           finish_when_complete();
                         },
                     .recognition_partial =
                         [&](std::string_view partial_text) {
                           partials.emplace_back(partial_text);
                           finish_when_complete();
                         },
                     .notification =
                         [&](FrontendNotificationKind kind, std::string_view message) {
                           notifications.emplace_back(kind, message);
                           finish_when_complete();
                         },
                 });
  assert(sender.requestName(
      std::string(dbus::kServiceBusName),
      {RequestNameFlag::AllowReplacement, RequestNameFlag::ReplaceExisting}));

  bool timed_out = false;
  auto timeout =
      loop.addTimeEvent(CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 2'000'000, 0,
                        [&timed_out, &loop](fcitx::EventSourceTime *, std::uint64_t) {
                          timed_out = true;
                          loop.exit();
                          return false;
                        });
  timeout->setOneShot();

  auto send_string_signal = [&](std::string_view signal_name, std::string_view value) {
    const std::string signal(signal_name);
    auto message =
        sender.createSignal(object_path.c_str(), interface.c_str(), signal.c_str());
    message << std::string(value);
    assert(message.send());
  };
  send_string_signal(dbus::kSignalStatusChanged, "recording");
  send_string_signal(dbus::kSignalRecognitionPartial, "live partial");

  const std::string notification_signal(dbus::kSignalDaemonNotification);
  auto info_signal = sender.createSignal(object_path.c_str(), interface.c_str(),
                                         notification_signal.c_str());
  info_signal << info.code << info.subject << info.detail << info.raw_message;
  assert(info_signal.send());

  auto error_signal = sender.createSignal(object_path.c_str(), interface.c_str(),
                                          notification_signal.c_str());
  error_signal << error.code << error.subject << error.detail << error.raw_message;
  assert(error_signal.send());
  sender.flush();
  auto release_name =
      loop.addTimeEvent(CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 50'000, 0,
                        [&sender](fcitx::EventSourceTime *, std::uint64_t) {
                          assert(sender.releaseName(
                              std::string(vinput_fcitx_bridge::dbus::kServiceBusName)));
                          return false;
                        });
  release_name->setOneShot();

  assert(loop.exec());
  assert(!timed_out);
  assert((service_availability == std::vector<bool>{true, false}));
  assert((statuses == std::vector<std::string>{"recording"}));
  assert((partials == std::vector<std::string>{"live partial"}));
  assert(notifications.size() == 2);
  assert(notifications[0].first == FrontendNotificationKind::Info);
  assert(notifications[0].second == "registry cache refreshed");
  assert(notifications[1].first == FrontendNotificationKind::Error);
  assert(notifications[1].second == "model metadata is invalid");
  return 0;
}
