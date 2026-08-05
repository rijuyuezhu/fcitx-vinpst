#include "vinpst_fcitx_bridge/fcitx_addon.h"
#include "vinpst_fcitx_bridge/sd_bus_daemon_client.h"

#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <string_view>
#include <thread>

using vinpst_fcitx_bridge::AppliedOutcome;
using vinpst_fcitx_bridge::BridgeOutcome;
using vinpst_fcitx_bridge::FcitxTriggerAction;
using vinpst_fcitx_bridge::FcitxVinpstAddon;
using vinpst_fcitx_bridge::FrontendBridge;
using vinpst_fcitx_bridge::SdBusDaemonClient;

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
      g_last_outcome.replace_selection == command_mode) {
    return true;
  }
  std::cerr << label << " produced unexpected bridge outcome: " << g_last_outcome.text
            << '\n';
  return false;
}

bool ExpectIgnoredTrigger(FcitxVinpstAddon *addon, FcitxTriggerAction action,
                          std::string_view label) {
  const auto applied =
      addon->ApplyTriggerAction(nullptr, action, "ignored selected text");
  if (applied == AppliedOutcome::None) {
    return true;
  }
  std::cerr << label
            << " did not ignore trigger action: applied=" << static_cast<int>(applied)
            << '\n';
  return false;
}

} // namespace

namespace vinpst_fcitx_bridge {

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

} // namespace vinpst_fcitx_bridge

int main() {
  FcitxVinpstAddon addon(nullptr);

  std::string error;
  auto client = ConnectWithRetry(&error);
  if (client == nullptr) {
    std::cerr << "connect failed: " << error << '\n';
    return 1;
  }

  vinpst_fcitx_bridge::SceneMenuController scene_controller;
  if (!client->RefreshSceneMenuController(&scene_controller, &error)) {
    std::cerr << "scene state failed: " << error << '\n';
    return 1;
  }

  FrontendBridge external_bridge;
  const auto external_start =
      external_bridge.StartNormal(client->raw_handle(), scene_controller);
  if (external_start.kind != BridgeOutcome::Kind::Preedit ||
      !external_bridge.recording()) {
    std::cerr << "external normal frontend start failed: " << external_start.text
              << '\n';
    return 1;
  }
  std::string external_status;
  if (!client->GetStatus(&external_status, &error) || external_status != "recording") {
    std::cerr << "external normal status check failed: " << error << '\n';
    return 1;
  }

  const auto expected_normal_text =
      ExpectedText("VINPST_DBUS_SMOKE_EXPECTED_NORMAL", "mock recognition result");
  const char *expected_takeover_env =
      std::getenv("VINPST_DBUS_SMOKE_EXPECTED_TAKEOVER");
  const auto expected_takeover_text = expected_takeover_env == nullptr
                                          ? expected_normal_text
                                          : std::string(expected_takeover_env);
  const auto recovered_stop =
      addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StartNormal);
  if (!ExpectApplied(recovered_stop, AppliedOutcome::Commit,
                     "cross-client normal takeover") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Commit, expected_takeover_text, false,
                         "cross-client normal takeover")) {
    std::cerr << "addon did not stop externally started normal recording\n";
    return 1;
  }
  if (!client->GetStatus(&external_status, &error) || external_status != "idle") {
    std::cerr << "external normal takeover did not return daemon to idle: " << error
              << '\n';
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal,
                            "normal stop while idle") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand,
                            "command stop while idle")) {
    return 1;
  }

  const auto command_start = addon.ApplyTriggerAction(
      nullptr, FcitxTriggerAction::StartCommand, "selected text");
  if (!ExpectApplied(command_start, AppliedOutcome::Preedit, "command start") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Preedit, "... Commanding ...", false,
                         "command start")) {
    std::cerr << "addon command trigger did not enter command recording mode\n";
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartCommand,
                            "duplicate command start") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartNormal,
                            "normal start while command recording") ||
      !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal,
                            "normal stop while command recording")) {
    return 1;
  }

  if (!client->GetStatus(&external_status, &error) || external_status != "recording") {
    std::cerr << "daemon status command recording check failed: " << error << '\n';
    return 1;
  }

  const auto expected_command_text = ExpectedText(
      "VINPST_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");
  const auto command_stop =
      addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StopCommand);
  if (!ExpectApplied(command_stop, AppliedOutcome::Commit, "command stop") ||
      !ExpectLastOutcome(BridgeOutcome::Kind::Commit, expected_command_text, true,
                         "command stop")) {
    std::cerr << "addon command trigger did not commit and reset\n";
    return 1;
  }

  if (!client->GetStatus(&external_status, &error) || external_status != "idle") {
    std::cerr << "daemon status after command stop failed: " << error << '\n';
    return 1;
  }

  if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand,
                            "command stop after reset")) {
    return 1;
  }

  std::cout << expected_normal_text << '\n' << expected_command_text << '\n';
  return 0;
}
