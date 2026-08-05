#pragma once

#include "vinpst_fcitx_ffi.h"

#include <cstdint>
#include <string>
#include <string_view>

namespace vinpst_fcitx_bridge {

inline const std::uint8_t *RustBytes(std::string_view value) {
  return value.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(value.data());
}

inline VinpstFcitxStringView ToRustStringView(std::string_view value) {
  return {RustBytes(value), value.size()};
}

inline std::string_view BorrowRustString(VinpstFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

inline std::string CopyRustString(VinpstFcitxStringView view) {
  return std::string(BorrowRustString(view));
}

} // namespace vinpst_fcitx_bridge
