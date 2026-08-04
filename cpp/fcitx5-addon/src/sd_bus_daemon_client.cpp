#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"
#include "vinput_fcitx_bridge/fcitx_menu_projection.h"
#include "vinput_fcitx_bridge/rust_handle.h"
#include "vinput_fcitx_bridge/rust_string.h"

#include "vinput_fcitx_ffi.h"

#include <cstdint>
#include <memory>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

using OwnedStringHandle =
    RustOwnedHandle<VinputFcitxOwnedString, vinput_fcitx_owned_string_free>;
using DaemonClientHandle =
    RustOwnedHandle<VinputFcitxDaemonClient, vinput_fcitx_daemon_client_free>;

std::string CopyOwnedString(const VinputFcitxOwnedString *value) {
  VinputFcitxStringView view{};
  if (value == nullptr || vinput_fcitx_owned_string_view(value, &view) == 0) {
    return {};
  }
  return CopyRustString(view);
}

void SetError(std::string *error, std::string message) {
  if (error != nullptr) {
    *error = std::move(message);
  }
}

void SetRustError(std::string *error, VinputFcitxOwnedString *raw_error,
                  std::string_view fallback) {
  auto error_text = OwnedStringHandle::Adopt(raw_error);
  auto message = CopyOwnedString(error_text.raw_handle());
  SetError(error, message.empty() ? std::string(fallback) : std::move(message));
}

} // namespace

struct SdBusDaemonClient::Impl {
  explicit Impl(VinputFcitxDaemonClient *client)
      : client(DaemonClientHandle::Adopt(client)) {}

  DaemonClientHandle client;
};

std::unique_ptr<SdBusDaemonClient>
SdBusDaemonClient::ConnectSession(std::string *error) {
  VinputFcitxOwnedString *raw_error = nullptr;
  auto *client = vinput_fcitx_daemon_client_connect(&raw_error);
  if (client == nullptr) {
    SetRustError(error, raw_error, "Failed to connect to the session D-Bus.");
    return nullptr;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  return std::unique_ptr<SdBusDaemonClient>(new SdBusDaemonClient(client));
}

SdBusDaemonClient::SdBusDaemonClient(VinputFcitxDaemonClient *client)
    : impl_(std::make_unique<Impl>(client)) {}

SdBusDaemonClient::~SdBusDaemonClient() = default;

bool SdBusDaemonClient::GetStatus(std::string *status, std::string *error) {
  if (status == nullptr) {
    SetError(error, "Missing string response output.");
    return false;
  }
  VinputFcitxOwnedString *raw_error = nullptr;
  auto status_text = OwnedStringHandle::Adopt(
      vinput_fcitx_daemon_client_get_status(impl_->client.raw_handle(), &raw_error));
  if (!status_text) {
    SetRustError(error, raw_error, "Voice input daemon request failed.");
    return false;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  *status = CopyOwnedString(status_text.raw_handle());
  return true;
}

bool SdBusDaemonClient::RefreshSceneMenuController(SceneMenuController *controller,
                                                   std::string *error) {
  if (controller == nullptr) {
    SetError(error, "Missing scene menu controller.");
    return false;
  }
  VinputFcitxOwnedString *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_refresh_scene_menu_controller(
          impl_->client.raw_handle(), controller->mutable_raw_handle(), &raw_error) ==
      0) {
    SetRustError(error, raw_error,
                 "Voice input daemon returned an invalid scene snapshot.");
    return false;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  return true;
}

bool SdBusDaemonClient::SetActiveScene(SceneMenuController *controller,
                                       std::string_view scene_id, bool *persisted,
                                       std::string *error) {
  if (controller == nullptr || persisted == nullptr) {
    SetError(error, "Missing boolean response output.");
    return false;
  }
  std::uint8_t persisted_value = 0;
  VinputFcitxOwnedString *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_set_active_scene(
          impl_->client.raw_handle(), controller->mutable_raw_handle(),
          RustBytes(scene_id), scene_id.size(), &persisted_value, &raw_error) == 0) {
    SetRustError(error, raw_error,
                 "Voice input daemon failed to set the active scene.");
    return false;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  *persisted = persisted_value != 0;
  return true;
}

bool SdBusDaemonClient::RefreshAsrMenuController(AsrMenuController *controller,
                                                 std::string *error) {
  if (controller == nullptr) {
    SetError(error, "Missing ASR menu controller.");
    return false;
  }
  VinputFcitxOwnedString *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_refresh_asr_menu_controller(
          impl_->client.raw_handle(), controller->mutable_raw_handle(), &raw_error) ==
      0) {
    SetRustError(error, raw_error,
                 "Voice input daemon returned an invalid ASR snapshot.");
    return false;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  return true;
}

bool SdBusDaemonClient::SetActiveAsrTarget(std::string_view provider_id,
                                           std::string_view model_value,
                                           bool *persisted, std::string *error) {
  if (persisted == nullptr) {
    SetError(error, "Missing boolean response output.");
    return false;
  }
  const VinputFcitxAsrTargetView target{
      .provider = ToRustStringView(provider_id),
      .model = ToRustStringView(model_value),
  };
  std::uint8_t persisted_value = 0;
  VinputFcitxOwnedString *raw_error = nullptr;
  if (vinput_fcitx_daemon_client_set_active_asr_target(
          impl_->client.raw_handle(), &target, &persisted_value, &raw_error) == 0) {
    SetRustError(error, raw_error,
                 "Voice input daemon failed to set the active ASR target.");
    return false;
  }
  auto ignored_error = OwnedStringHandle::Adopt(raw_error);
  *persisted = persisted_value != 0;
  return true;
}

const VinputFcitxDaemonClient *SdBusDaemonClient::raw_handle() const {
  return impl_->client.raw_handle();
}

} // namespace vinput_fcitx_bridge
