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
  using vinput_fcitx_bridge::DaemonNotificationPayload;
  using vinput_fcitx_bridge::FcitxDaemonSignalMonitor;
  using vinput_fcitx_bridge::FrontendNotificationKind;
  using vinput_fcitx_bridge::RenderDaemonNotification;
  namespace dbus = vinput_fcitx_bridge::dbus;

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
  const std::string signal_name(dbus::kSignalDaemonNotification);

  std::vector<DaemonNotificationPayload> received;
  FcitxDaemonSignalMonitor monitor(
      &receiver, [&received, &loop](const DaemonNotificationPayload &payload) {
        received.push_back(payload);
        if (received.size() == 2) {
          loop.exit();
        }
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

  auto info_signal =
      sender.createSignal(object_path.c_str(), interface.c_str(), signal_name.c_str());
  info_signal << info.code << info.subject << info.detail << info.raw_message;
  assert(info_signal.send());

  auto error_signal =
      sender.createSignal(object_path.c_str(), interface.c_str(), signal_name.c_str());
  error_signal << error.code << error.subject << error.detail << error.raw_message;
  assert(error_signal.send());
  sender.flush();

  assert(loop.exec());
  assert(!timed_out);
  assert(received.size() == 2);
  assert(received[0] == info);
  assert(received[1] == error);
  return 0;
}
