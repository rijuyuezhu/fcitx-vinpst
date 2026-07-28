#include "vinput_fcitx_bridge/fcitx_notifications.h"

#include <cassert>
#include <cstdio>
#include <string>

namespace {

std::string ReadStream(std::FILE *stream) {
  assert(stream != nullptr);
  std::fflush(stream);
  assert(std::fseek(stream, 0, SEEK_END) == 0);
  const long size = std::ftell(stream);
  assert(size >= 0);
  assert(std::fseek(stream, 0, SEEK_SET) == 0);
  std::string text(static_cast<std::size_t>(size), '\0');
  if (!text.empty()) {
    assert(std::fread(text.data(), 1, text.size(), stream) == text.size());
  }
  return text;
}

} // namespace

int main() {
  using vinput_fcitx_bridge::BuildFrontendNotification;
  using vinput_fcitx_bridge::FrontendNotificationKind;
  using vinput_fcitx_bridge::kErrorNotificationTimeoutMs;
  using vinput_fcitx_bridge::kInfoNotificationTimeoutMs;
  using vinput_fcitx_bridge::SendFrontendNotification;

  const auto info = BuildFrontendNotification(FrontendNotificationKind::Info, "done");
  assert(info.app_name == "fcitx5-vinput");
  assert(info.icon == "dialog-information");
  assert(info.summary == "Voice Input");
  assert(info.body == "done");
  assert(info.timeout_ms == kInfoNotificationTimeoutMs);

  const auto warning =
      BuildFrontendNotification(FrontendNotificationKind::Warning, "careful");
  assert(warning.icon == "dialog-warning");
  assert(warning.timeout_ms == kErrorNotificationTimeoutMs);

  const auto error =
      BuildFrontendNotification(FrontendNotificationKind::Error, "failed");
  assert(error.icon == "dialog-error");
  assert(error.timeout_ms == kErrorNotificationTimeoutMs);

  std::FILE *fallback = std::tmpfile();
  assert(fallback != nullptr);
  assert(!SendFrontendNotification(nullptr, info, fallback));
  assert(ReadStream(fallback) == "vinput: Voice Input: done\n");
  std::fclose(fallback);

  const auto empty = BuildFrontendNotification(FrontendNotificationKind::Info, "");
  fallback = std::tmpfile();
  assert(fallback != nullptr);
  assert(!SendFrontendNotification(nullptr, empty, fallback));
  assert(ReadStream(fallback).empty());
  std::fclose(fallback);
  return 0;
}
