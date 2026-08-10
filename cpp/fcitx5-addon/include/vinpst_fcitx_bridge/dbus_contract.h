#pragma once

#include <string_view>

namespace vinpst_fcitx_bridge::dbus {

inline constexpr std::string_view kServiceBusName = "org.fcitx.Vinpst";
inline constexpr std::string_view kServiceObjectPath = "/org/fcitx/Vinpst";
inline constexpr std::string_view kServiceInterface = "org.fcitx.Vinpst.Service";
inline constexpr std::string_view kFcitxBusName = "org.fcitx.Fcitx5";
inline constexpr std::string_view kNotifierObjectPath = "/org/fcitx/Fcitx5/Vinpst";
inline constexpr std::string_view kNotifierInterface = "org.fcitx.Fcitx5.Vinpst1";
inline constexpr char kMethodNotify[] = "Notify";
inline constexpr std::string_view kSignalRecognitionResult = "RecognitionResult";
inline constexpr std::string_view kSignalRecognitionPartial = "RecognitionPartial";
inline constexpr std::string_view kSignalStatusChanged = "StatusChanged";
inline constexpr std::string_view kSignalDaemonNotification = "DaemonNotification";
inline constexpr std::string_view kStatusRecording = "recording";

} // namespace vinpst_fcitx_bridge::dbus
