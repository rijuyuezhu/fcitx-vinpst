#pragma once

#include "vinpst_fcitx_bridge/dbus_contract.h"

#include <fcitx-utils/dbus/objectvtable.h>

#include <functional>
#include <string>
#include <string_view>
#include <utility>

namespace vinpst_fcitx_bridge {

class FcitxNotifierDbusObject
    : public fcitx::dbus::ObjectVTable<FcitxNotifierDbusObject> {
public:
  using Callback = std::function<void(std::string_view, std::string_view,
                                      std::string_view, std::string_view)>;

  explicit FcitxNotifierDbusObject(Callback callback)
      : callback_(std::move(callback)) {}

  void Notify(const std::string &code, const std::string &subject,
              const std::string &detail, const std::string &raw_message) {
    if (callback_) {
      callback_(code, subject, detail, raw_message);
    }
  }

private:
  FCITX_OBJECT_VTABLE_METHOD(Notify, dbus::kMethodNotify, "ssss", "");

  Callback callback_;
};

} // namespace vinpst_fcitx_bridge
