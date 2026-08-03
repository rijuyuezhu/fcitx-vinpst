#pragma once

#include <string_view>

namespace vinput_fcitx_bridge::dbus {

inline constexpr std::string_view kServiceBusName = "org.fcitx.Vinput";
inline constexpr std::string_view kServiceObjectPath = "/org/fcitx/Vinput";
inline constexpr std::string_view kServiceInterface = "org.fcitx.Vinput.Service";
inline constexpr std::string_view kSignalRecognitionPartial = "RecognitionPartial";
inline constexpr std::string_view kSignalStatusChanged = "StatusChanged";
inline constexpr std::string_view kSignalDaemonNotification = "DaemonNotification";
inline constexpr std::string_view kStatusRecording = "recording";

} // namespace vinput_fcitx_bridge::dbus
