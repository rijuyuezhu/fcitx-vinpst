#include "vinput_fcitx_bridge/fcitx_addon.h"
#include "vinput_fcitx_bridge/sd_bus_daemon_client.h"

#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <string_view>
#include <thread>

using vinput_fcitx_bridge::AppliedOutcome;
using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::FcitxTriggerAction;
using vinput_fcitx_bridge::FcitxVinputAddon;
using vinput_fcitx_bridge::SdBusDaemonClient;

namespace {

BridgeOutcome g_last_outcome;

std::unique_ptr<SdBusDaemonClient> ConnectWithRetry(std::string *error) {
  for (int attempt = 0; attempt < 50; ++attempt) {
    auto client = SdBusDaemonClient::ConnectSession(error);
    if (client != nullptr) {
      return client;
    }
    std::this_thread::sleep_for(std::chrono::milliseconds(100));
  }
  return nullptr;
}

std::string ExpectedText(const char *env_name, const char *fallback) {
  const char *value = std::getenv(env_name);
  return value == nullptr ? std::string(fallback) : std::string(value);
}

bool Contains(std::string_view haystack, std::string_view needle) {
  return haystack.find(needle) != std::string_view::npos;
}

bool GetRuntimeStatus(SdBusDaemonClient *client, std::string *status_json,
                      std::string *error) {
  if (!client->GetRuntimeStatus(status_json, error)) {
    return false;
  }
  return true;
}

bool ExpectRuntimeStatus(SdBusDaemonClient *client, std::string_view expected,
                         std::string *error) {
  std::string status_json;
  if (!GetRuntimeStatus(client, &status_json, error)) {
    return false;
  }
  if (!Contains(status_json, expected)) {
    if (error != nullptr) {
      *error = "runtime status missing expected marker: ";
      *error += expected;
      *error += " in ";
      *error += status_json;
    }
    return false;
  }
  return true;
}

bool ExpectSanitizedCommandStatus(SdBusDaemonClient *client, std::string *error) {
  std::string status_json;
  if (!GetRuntimeStatus(client, &status_json, error)) {
    return false;
  }
  if (!Contains(status_json, "\"status\":\"recording\"") ||
      !Contains(status_json, "\"selected_text_present\":true") ||
      Contains(status_json, "selected text")) {
    if (error != nullptr) {
      *error = "runtime status command snapshot was not sanitized: ";
      *error += status_json;
    }
    return false;
  }
  return true;
}

bool ExpectApplied(AppliedOutcome actual, AppliedOutcome expected,
                   std::string_view label) {
  if (actual == expected) {
    return true;
  }
  std::cerr << label << " produced unexpected applied outcome\n";
  return false;
}

bool ExpectLastOutcome(BridgeOutcome::Kind kind, std::string_view text,
                       bool command_mode, std::string_view label) {
  if (g_last_outcome.kind == kind && g_last_outcome.text == text &&
      g_last_outcome.command_mode == command_mode) {
    return true;
  }
  std::cerr << label << " produced unexpected bridge outcome: " << g_last_outcome.text
            << '\n';
  return false;
}

bool ExpectIgnoredTrigger(FcitxVinputAddon *addon, FcitxTriggerAction action,
                          bool expected_recording, bool expected_command_mode,
                          std::string_view label) {
  const auto applied =
      addon->ApplyTriggerAction(nullptr, action, "ignored selected text");
  if (applied == AppliedOutcome::None &&
      addon->bridge().recording() == expected_recording &&
      addon->bridge().command_mode() == expected_command_mode) {
    return true;
  }
  std::cerr << label << " did not ignore trigger action without changing mode"
            << ": applied=" << static_cast<int>(applied)
            << " recording=" << addon->bridge().recording()
            << " command_mode=" << addon->bridge().command_mode() << '\n';
  return false;
}

} // namespace

namespace vinput_fcitx_bridge {

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *) {
  g_last_outcome = outcome;
  switch (outcome.kind) {
  case BridgeOutcome::Kind::None:
    return AppliedOutcome::None;
  case BridgeOutcome::Kind::Preedit:
  case BridgeOutcome::Kind::Error:
    return AppliedOutcome::Preedit;
  case BridgeOutcome::Kind::Clear:
    return AppliedOutcome::Clear;
  case BridgeOutcome::Kind::Commit:
    return AppliedOutcome::Commit;
  case BridgeOutcome::Kind::CandidateMenu:
    return AppliedOutcome::CandidateMenu;
  }
  return AppliedOutcome::None;
}

} // namespace vinput_fcitx_bridge

int main() {
  FcitxVinputAddon addon(nullptr);

  std::string error;
  auto client = ConnectWithRetry(&error);
  if (client == nullptr) {
    std::cerr << "connect failed: " << error << '\n';
    return 1;
  }

  if (!client->StartRecording(&error)) {
    std::cerr << "external normal start failed: " << error << '\n';
    return 1;
  }
  std::string external_status;
  if (!client->GetStatus(&external_status, &error) || external_status != "recording") {
    std::cerr << "external normal status check failed: " << error << '\n';
    return 1;
  }
  if (!ExpectRuntimeStatus(client.get(), "\"status\":\"recording\"", &error) ||
      !ExpectRuntimeStatus(client.get(), "\"selected_text_present\":false", &error)) {
    std::cerr << "runtime status external normal check failed: " << error << '\n';
    return 1;
  }

  const auto expected_normal_text =
      ExpectedText("VINPUT_DBUS_SMOKE_EXPECTED_NORMAL", "mock recognition result");
  const auto recovered_stop =
      addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StartNormal);
  if (!ExpectApplied(recovered_stop, AppliedOutcome::Commit,
                     "cross-client normal takeover") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Commit, expected_normal_text, false,
                         "cross-client normal takeover") ||
      addon.bridge().recording() || addon.bridge().command_mode()) {
    std::cerr << "addon did not stop externally started normal recording\n";
    return 1;
  }
  if (!client->GetStatus(&external_status, &error) || external_status != "idle") {
    std::cerr << "external normal takeover did not return daemon to idle: " << error
              << '\n';
    return 1;
  }
  if (!ExpectRuntimeStatus(client.get(), "\"status\":\"idle\"", &error)) {
    std::cerr << "runtime status after normal takeover failed: " << error << '\n';
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal, false, false,
                            "normal stop while idle") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand, false, false,
                            "command stop while idle")) {
    return 1;
  }

  const auto command_start = addon.ApplyTriggerAction(
      nullptr, FcitxTriggerAction::StartCommand, "selected text");
  if (!ExpectApplied(command_start, AppliedOutcome::Preedit, "command start") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Preedit, "... Commanding ...", false,
                         "command start") ||
      !addon.bridge().recording() || !addon.bridge().command_mode()) {
    std::cerr << "addon command trigger did not enter command recording mode\n";
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartCommand, true, true,
                            "duplicate command start") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartNormal, true, true,
                            "normal start while command recording") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal, true, true,
                            "normal stop while command recording")) {
    return 1;
  }

  if (!ExpectSanitizedCommandStatus(client.get(), &error)) {
    std::cerr << "runtime status command check failed: " << error << '\n';
    return 1;
  }

  const auto expected_command_text = ExpectedText(
      "VINPUT_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");
  const auto command_stop =
      addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StopCommand);
  if (!ExpectApplied(command_stop, AppliedOutcome::Commit, "command stop") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Commit, expected_command_text, true,
                         "command stop") ||
      addon.bridge().recording() || addon.bridge().command_mode()) {
    std::cerr << "addon command trigger did not commit and reset\n";
    return 1;
  }

  if (!ExpectRuntimeStatus(client.get(), "\"status\":\"idle\"", &error) ||
      !ExpectRuntimeStatus(client.get(), "\"selected_text_present\":false", &error)) {
    std::cerr << "runtime status after command stop failed: " << error << '\n';
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand, false, false,
                            "command stop after reset")) {
    return 1;
  }

  std::cout << expected_normal_text << '\n' << expected_command_text << '\n';
  return 0;
}
