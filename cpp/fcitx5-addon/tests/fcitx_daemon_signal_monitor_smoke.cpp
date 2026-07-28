#include "vinput_fcitx_bridge/dbus_contract.h"
#include "vinput_fcitx_bridge/fcitx_daemon_signal_monitor.h"

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>

#include <cassert>
#include <cstdint>
#include <string>
#include <vector>

int main() {
  using fcitx::dbus::Bus;
  using fcitx::dbus::BusType;
  using fcitx::dbus::RequestNameFlag;
  using vinput_fcitx_bridge::ClassifyDaemonNotification;
  using vinput_fcitx_bridge::ComposeDaemonStatusPreedit;
  using vinput_fcitx_bridge::DaemonNotificationPayload;
  using vinput_fcitx_bridge::DaemonSignalCallbacks;
  using vinput_fcitx_bridge::FcitxDaemonSignalMonitor;
  using vinput_fcitx_bridge::FrontendNotificationKind;
  using vinput_fcitx_bridge::RenderDaemonNotification;
  namespace dbus = vinput_fcitx_bridge::dbus;

  assert(ComposeDaemonStatusPreedit("recording", false, "") == "... Recording ...");
  assert(ComposeDaemonStatusPreedit("recording", true, "") == "... Commanding ...");
  assert(ComposeDaemonStatusPreedit("inferring", false, "") == "... Recognizing ...");
  assert(ComposeDaemonStatusPreedit("postprocessing", false, "") ==
         "... Postprocessing ...");
  assert(ComposeDaemonStatusPreedit("idle", false, "").empty());
  assert(ComposeDaemonStatusPreedit("recording", false, "live partial") ==
         "live partial");

  const DaemonNotificationPayload info{
      .code = "unknown",
      .subject = "",
      .detail = "",
      .raw_message = "registry cache refreshed",
  };
  assert(!info.empty());
  assert(ClassifyDaemonNotification(info) == FrontendNotificationKind::Info);
  assert(RenderDaemonNotification(info) == "registry cache refreshed");

  const DaemonNotificationPayload error{
      .code = "asr_backend_reload_failed",
      .subject = "sherpa-onnx",
      .detail = "model metadata is invalid",
      .raw_message = "",
  };
  assert(ClassifyDaemonNotification(error) == FrontendNotificationKind::Error);
  assert(RenderDaemonNotification(error) == "model metadata is invalid");
  assert(RenderDaemonNotification({}) == "Unknown error.");

  fcitx::EventLoop loop;
  Bus receiver(BusType::Session);
  Bus sender(BusType::Session);
  assert(receiver.isOpen());
  assert(sender.isOpen());
  receiver.attachEventLoop(&loop);
  assert(sender.requestName(
      std::string(dbus::kServiceBusName),
      {RequestNameFlag::AllowReplacement, RequestNameFlag::ReplaceExisting}));

  const std::string object_path(dbus::kServiceObjectPath);
  const std::string interface(dbus::kServiceInterface);
  std::vector<std::string> statuses;
  std::vector<std::string> partials;
  std::vector<DaemonNotificationPayload> notifications;
  auto finish_when_complete = [&] {
    if (statuses.size() == 1 && partials.size() == 1 && notifications.size() == 2) {
      loop.exit();
    }
  };
  FcitxDaemonSignalMonitor monitor(
      &receiver, DaemonSignalCallbacks{
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
                         [&](const DaemonNotificationPayload &payload) {
                           notifications.push_back(payload);
                           finish_when_complete();
                         },
                 });
  assert(monitor.active());

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

  assert(loop.exec());
  assert(!timed_out);
  assert((statuses == std::vector<std::string>{"recording"}));
  assert((partials == std::vector<std::string>{"live partial"}));
  assert(notifications.size() == 2);
  assert(notifications[0] == info);
  assert(notifications[1] == error);
  return 0;
}
