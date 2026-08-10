#include "vinpst_fcitx_bridge/dbus_contract.h"
#include "vinpst_fcitx_bridge/fcitx_notifications.h"
#include "vinpst_fcitx_bridge/fcitx_notifier_dbus_object.h"

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>

#include <array>
#include <cassert>
#include <cstdint>
#include <string>

int main() {
  using fcitx::dbus::Bus;
  using fcitx::dbus::BusType;
  using fcitx::dbus::Message;
  using fcitx::dbus::RequestNameFlag;
  using vinpst_fcitx_bridge::FcitxNotifierDbusObject;
  using vinpst_fcitx_bridge::FrontendNotificationKind;
  namespace dbus = vinpst_fcitx_bridge::dbus;

  fcitx::EventLoop loop;
  Bus receiver(BusType::Session);
  Bus sender(BusType::Session);
  assert(receiver.isOpen());
  assert(sender.isOpen());
  receiver.attachEventLoop(&loop);
  sender.attachEventLoop(&loop);

  std::array<std::string, 4> received;
  FcitxNotifierDbusObject object([&](std::string_view code, std::string_view subject,
                                     std::string_view detail,
                                     std::string_view raw_message) {
    received = {std::string(code), std::string(subject), std::string(detail),
                std::string(raw_message)};
  });
  assert(receiver.addObjectVTable(std::string(dbus::kNotifierObjectPath),
                                  std::string(dbus::kNotifierInterface), object));
  assert(receiver.requestName(
      std::string(dbus::kFcitxBusName),
      {RequestNameFlag::AllowReplacement, RequestNameFlag::ReplaceExisting}));

  const std::string service(dbus::kFcitxBusName);
  const std::string path(dbus::kNotifierObjectPath);
  const std::string interface(dbus::kNotifierInterface);
  const std::string method(dbus::kMethodNotify);
  auto message = sender.createMethodCall(service.c_str(), path.c_str(),
                                         interface.c_str(), method.c_str());
  message << std::string("daemon_restart_failed")
          << std::string("vinpst-daemon.service") << std::string("restart failed")
          << std::string("systemctl restart failed");

  bool replied = false;
  auto pending = message.callAsync(5'000'000, [&](Message &reply) {
    assert(reply);
    assert(!reply.isError());
    replied = true;
    loop.exit();
    return true;
  });
  assert(pending);

  bool timed_out = false;
  auto timeout =
      loop.addTimeEvent(CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 2'000'000, 0,
                        [&](fcitx::EventSourceTime *, std::uint64_t) {
                          timed_out = true;
                          loop.exit();
                          return false;
                        });
  timeout->setOneShot();

  assert(loop.exec());
  assert(!timed_out);
  assert(replied);
  assert((received ==
          std::array<std::string, 4>{"daemon_restart_failed", "vinpst-daemon.service",
                                     "restart failed", "systemctl restart failed"}));

  const auto [kind, text] = vinpst_fcitx_bridge::PlanStructuredDaemonNotification(
      received[0], received[1], received[2], received[3]);
  assert(kind == FrontendNotificationKind::Error);
  assert(text == "systemctl restart failed");
  return 0;
}
