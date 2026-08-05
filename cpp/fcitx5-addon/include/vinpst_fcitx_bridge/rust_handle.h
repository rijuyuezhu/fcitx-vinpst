#pragma once

#include <utility>

namespace vinpst_fcitx_bridge {

template <typename Handle, void (*Free)(Handle *)> class RustOwnedHandle final {
public:
  RustOwnedHandle() = default;

  static RustOwnedHandle Adopt(Handle *handle) {
    return RustOwnedHandle(handle);
  }

  ~RustOwnedHandle() {
    Free(handle_);
  }

  RustOwnedHandle(const RustOwnedHandle &) = delete;
  RustOwnedHandle &operator=(const RustOwnedHandle &) = delete;

  RustOwnedHandle(RustOwnedHandle &&other) noexcept
      : handle_(std::exchange(other.handle_, nullptr)) {}

  RustOwnedHandle &operator=(RustOwnedHandle &&other) noexcept {
    if (this != &other) {
      Free(handle_);
      handle_ = std::exchange(other.handle_, nullptr);
    }
    return *this;
  }

  const Handle *raw_handle() const {
    return handle_;
  }

  Handle *mutable_raw_handle() {
    return handle_;
  }

  explicit operator bool() const {
    return handle_ != nullptr;
  }

private:
  explicit RustOwnedHandle(Handle *handle) : handle_(handle) {}

  Handle *handle_ = nullptr;
};

} // namespace vinpst_fcitx_bridge
