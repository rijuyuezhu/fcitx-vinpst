#include "vinpst_fcitx_bridge/fcitx_addon.h"
#include "vinpst_fcitx_bridge/sd_bus_daemon_client.h"

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>

#include <algorithm>
#include <chrono>
#include <cstdlib>
#include <iostream>
#include <memory>
#include <string>
#include <string_view>
#include <thread>
#include <utility>
#include <vector>

using vinpst_fcitx_bridge::AppliedOutcome;
using vinpst_fcitx_bridge::BridgeOutcome;
using vinpst_fcitx_bridge::FcitxTriggerAction;
using vinpst_fcitx_bridge::FcitxVinpstAddon;
using vinpst_fcitx_bridge::FrontendBridge;
using vinpst_fcitx_bridge::SdBusDaemonClient;

namespace {

BridgeOutcome g_last_outcome;
std::vector<BridgeOutcome> g_outcomes;

void ResetOutcomes() {
  g_last_outcome = {};
  g_outcomes.clear();
}

bool OutcomeSeen(BridgeOutcome::Kind kind, std::string_view text, bool command_mode) {
  return std::any_of(g_outcomes.begin(), g_outcomes.end(),
                     [&](const BridgeOutcome &outcome) {
                       return outcome.kind == kind && outcome.text == text &&
                              outcome.replace_selection == command_mode;
                     });
}

void Reschedule(fcitx::EventSourceTime *event, std::uint64_t delay_usec) {
  event->setTime(fcitx::now(CLOCK_MONOTONIC) + delay_usec);
  event->setEnabled(true);
}

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
  std::cerr << label << " produced unexpected applied outcome: actual="
            << static_cast<int>(actual) << " expected=" << static_cast<int>(expected)
            << " bridge-kind=" << static_cast<int>(g_last_outcome.kind)
            << " text=" << g_last_outcome.text << '\n';
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

AppliedOutcome
ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome, fcitx::InputContext *,
                                 ResultCandidateSelectCallback on_candidate_select) {
  auto ignored_callback = std::move(on_candidate_select);
  static_cast<void>(ignored_callback);
  g_last_outcome = outcome;
  g_outcomes.push_back(outcome);
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

std::string ResultCandidateMenuTitle(std::size_t) {
  return "Choose Result";
}

void ClearResultCandidateMenu(fcitx::InputContext *) {}

void ApplyResultCandidateSelection(fcitx::InputContext *, const PresentedCandidate &,
                                   bool) {}

} // namespace vinpst_fcitx_bridge

int main() {
  fcitx::EventLoop event_loop;
  fcitx::dbus::Bus signal_bus(fcitx::dbus::BusType::Session);
  if (!signal_bus.isOpen()) {
    std::cerr << "Fcitx session bus is unavailable\n";
    return 1;
  }
  signal_bus.attachEventLoop(&event_loop);
  FcitxVinpstAddon addon(nullptr, &signal_bus);

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
  const auto expected_command_text = ExpectedText(
      "VINPST_DBUS_SMOKE_EXPECTED_COMMAND", "mock command result for: selected text");

  AppliedOutcome recovered_stop = AppliedOutcome::None;
  AppliedOutcome command_start = AppliedOutcome::None;
  AppliedOutcome command_stop = AppliedOutcome::None;
  bool takeover_attempted = false;
  std::string stage = "waiting for takeover dispatch";
  std::string failure;
  auto fail = [&](std::string message) {
    if (failure.empty()) {
      failure = std::move(message);
    }
    event_loop.exit();
  };

  ResetOutcomes();
  std::unique_ptr<fcitx::EventSourceTime> command_stop_dispatch;
  std::unique_ptr<fcitx::EventSourceTime> final_check;
  auto takeover_dispatch =
      event_loop.addTimeEvent(CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 10'000, 0,
                              [&](fcitx::EventSourceTime *, std::uint64_t) {
                                takeover_attempted = true;
                                recovered_stop = addon.ApplyTriggerAction(
                                    nullptr, FcitxTriggerAction::StartNormal);
                                stage = "waiting for takeover result";
                                return false;
                              });
  takeover_dispatch->setOneShot();

  auto command_dispatch = event_loop.addTimeEvent(
      CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 50'000, 0,
      [&](fcitx::EventSourceTime *event, std::uint64_t) {
        if (!takeover_attempted) {
          Reschedule(event, 20'000);
          return true;
        }
        if (!ExpectApplied(recovered_stop, AppliedOutcome::None,
                           "cross-client normal takeover dispatch")) {
          fail("cross-client normal takeover did not dispatch asynchronously");
          return false;
        }
        if (!OutcomeSeen(BridgeOutcome::Kind::Commit, expected_takeover_text, false)) {
          Reschedule(event, 20'000);
          return true;
        }
        if (!client->GetStatus(&external_status, &error)) {
          fail("external normal takeover status query failed: " + error);
          return false;
        }
        if (external_status != "idle") {
          Reschedule(event, 20'000);
          return true;
        }
        if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal,
                                  "normal stop while idle") ||
            !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand,
                                  "command stop while idle")) {
          fail("idle trigger gating failed after normal takeover");
          return false;
        }
        ResetOutcomes();
        command_start = addon.ApplyTriggerAction(
            nullptr, FcitxTriggerAction::StartCommand, "selected text");
        stage = "waiting for command recording";
        command_stop_dispatch = event_loop.addTimeEvent(
            CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 20'000, 0,
            [&](fcitx::EventSourceTime *event, std::uint64_t) {
              if (!ExpectApplied(command_start, AppliedOutcome::Preedit,
                                 "command start dispatch") ||
                  !OutcomeSeen(BridgeOutcome::Kind::Preedit, "... Commanding ...",
                               false)) {
                fail("addon command trigger did not enter command recording mode");
                return false;
              }
              if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartCommand,
                                        "duplicate command start") ||
                  !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StartNormal,
                                        "normal start while command recording") ||
                  !ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopNormal,
                                        "normal stop while command recording")) {
                fail("recording trigger gating failed during command mode");
                return false;
              }
              if (!client->GetStatus(&external_status, &error)) {
                fail("daemon status command recording query failed: " + error);
                return false;
              }
              if (external_status != "recording") {
                Reschedule(event, 20'000);
                return true;
              }
              ResetOutcomes();
              command_stop =
                  addon.ApplyTriggerAction(nullptr, FcitxTriggerAction::StopCommand);
              stage = "waiting for command result";
              final_check = event_loop.addTimeEvent(
                  CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 20'000, 0,
                  [&](fcitx::EventSourceTime *event, std::uint64_t) {
                    if (!ExpectApplied(command_stop, AppliedOutcome::None,
                                       "command stop dispatch")) {
                      fail("addon command stop did not dispatch asynchronously");
                      return false;
                    }
                    if (!OutcomeSeen(BridgeOutcome::Kind::Commit, expected_command_text,
                                     true)) {
                      Reschedule(event, 20'000);
                      return true;
                    }
                    if (!client->GetStatus(&external_status, &error)) {
                      fail("daemon status after command stop query failed: " + error);
                      return false;
                    }
                    if (external_status != "idle") {
                      Reschedule(event, 20'000);
                      return true;
                    }
                    if (!ExpectIgnoredTrigger(&addon, FcitxTriggerAction::StopCommand,
                                              "command stop after reset")) {
                      fail("command stop after reset was not ignored");
                      return false;
                    }
                    event_loop.exit();
                    stage = "complete";
                    return false;
                  });
              return false;
            });
        return false;
      });

  auto timeout = event_loop.addTimeEvent(
      CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 3'000'000, 0,
      [&](fcitx::EventSourceTime *, std::uint64_t) {
        std::string observed_status;
        std::string observed_error;
        static_cast<void>(client->GetStatus(&observed_status, &observed_error));
        fail("addon async D-Bus smoke timed out: stage=" + stage +
             " status=" + observed_status + " status-error=" + observed_error +
             " outcomes=" + std::to_string(g_outcomes.size()) +
             " last-kind=" + std::to_string(static_cast<int>(g_last_outcome.kind)) +
             " last-text=" + g_last_outcome.text +
             " last-replace=" + std::to_string(g_last_outcome.replace_selection) +
             " expected-takeover=" + expected_takeover_text);
        return false;
      });
  timeout->setOneShot();

  if (!event_loop.exec() || !failure.empty()) {
    if (!failure.empty()) {
      std::cerr << failure << '\n';
    }
    return 1;
  }

  std::cout << expected_normal_text << '\n' << expected_command_text << '\n';
  return 0;
}
