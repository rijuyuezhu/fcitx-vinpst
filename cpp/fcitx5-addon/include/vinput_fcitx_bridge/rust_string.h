#pragma once

#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <string>
#include <string_view>

namespace vinput_fcitx_bridge {

inline const std::uint8_t *RustBytes(std::string_view value) {
  return value.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(value.data());
}

inline std::string_view BorrowRustString(VinputFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

inline std::string CopyRustString(VinputFcitxStringView view) {
  return std::string(BorrowRustString(view));
}

} // namespace vinput_fcitx_bridge
