#include "vinpst_fcitx_bridge/fcitx_addon.h"
#include "vinpst_fcitx_bridge/fcitx_menu_projection.h"
#include "vinpst_fcitx_bridge/frontend_bridge.h"
#include "vinpst_fcitx_bridge/sd_bus_daemon_client.h"

#include <fcitx-utils/dbus/bus.h>
#include <fcitx-utils/event.h>
#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>

#include <cstdint>
#include <cstdlib>
#include <iostream>
#include <string>
#include <utility>
#include <vector>

namespace {

class TestInputContext final : public fcitx::InputContext {
public:
  explicit TestInputContext(fcitx::InputContextManager &manager)
      : fcitx::InputContext(manager, "vinpst-native-addon-smoke") {
    created();
  }

  ~TestInputContext() override {
    destroy();
  }

  const char *frontend() const override {
    return "vinpst-native-addon-smoke";
  }

  std::vector<std::string> committed;
  std::vector<std::pair<int, unsigned int>> deleted;

protected:
  void commitStringImpl(const std::string &text) override {
    committed.push_back(text);
  }

  void deleteSurroundingTextImpl(int offset, unsigned int size) override {
    deleted.emplace_back(offset, size);
  }

  void forwardKeyImpl(const fcitx::ForwardKeyEvent &) override {}

  void updatePreeditImpl() override {}
};

std::string RequiredEnvironment(const char *name) {
  const char *value = std::getenv(name);
  if (value == nullptr || value[0] == '\0') {
    std::cerr << name << " is required\n";
    std::exit(2);
  }
  return value;
}

std::string OptionalEnvironment(const char *name) {
  const char *value = std::getenv(name);
  return value == nullptr ? std::string{} : std::string(value);
}

} // namespace

int main() {
  using fcitx::dbus::Bus;
  using fcitx::dbus::BusType;
  using vinpst_fcitx_bridge::AppliedOutcome;
  using vinpst_fcitx_bridge::FrontendBridge;
  using vinpst_fcitx_bridge::FcitxTriggerAction;
  using vinpst_fcitx_bridge::FcitxVinpstAddon;
  using vinpst_fcitx_bridge::SceneMenuController;
  using vinpst_fcitx_bridge::SdBusDaemonClient;

  const auto expected_text =
      RequiredEnvironment("VINPST_NATIVE_FRONTEND_EXPECTED_TEXT");
  const auto selected_text = OptionalEnvironment("VINPST_NATIVE_ADDON_SELECTED_TEXT");
  const bool command_mode = !selected_text.empty();
  const bool expect_candidate_menu =
      !OptionalEnvironment("VINPST_NATIVE_ADDON_EXPECT_CANDIDATE_MENU").empty();
  const bool external_session =
      !OptionalEnvironment("VINPST_NATIVE_ADDON_EXTERNAL_SESSION").empty();

  fcitx::InputContextManager manager;
  TestInputContext input_context(manager);
  if (command_mode) {
    input_context.surroundingText().setText(selected_text, selected_text.size(), 0);
  }

  fcitx::EventLoop signal_loop;
  Bus signal_bus(BusType::Session);
  if (!signal_bus.isOpen()) {
    std::cerr << "native addon signal bus is unavailable\n";
    return 1;
  }
  signal_bus.attachEventLoop(&signal_loop);

  auto input_context_watch = input_context.watch();
  if (!input_context_watch.isValid() || input_context_watch.get() != &input_context) {
    std::cerr << "native addon test InputContext watch is invalid\n";
    return 1;
  }

  FcitxVinpstAddon addon(nullptr, &signal_bus);
  if (external_session) {
    if (command_mode) {
      std::cerr << "external-session smoke currently expects normal recording\n";
      return 1;
    }
    static_cast<void>(addon.ApplyTriggerAction(&input_context, FcitxTriggerAction::None));

    std::string client_error;
    auto client = SdBusDaemonClient::ConnectSession(&client_error);
    if (client == nullptr) {
      std::cerr << "external-session daemon client failed: " << client_error << '\n';
      return 1;
    }
    SceneMenuController external_scenes;
    if (!client->RefreshSceneMenuController(&external_scenes, &client_error)) {
      std::cerr << "external-session scene refresh failed: " << client_error << '\n';
      return 1;
    }
    FrontendBridge external_bridge;
    std::string failure;
    std::string partial_text;
    bool partial_seen = false;
    bool stop_attempted = false;
    std::unique_ptr<fcitx::EventSourceTime> partial_check;
    std::unique_ptr<fcitx::EventSourceTime> final_check;
    auto fail = [&](std::string message) {
      if (failure.empty()) {
        failure = std::move(message);
      }
      signal_loop.exit();
    };
    auto delayed_start = signal_loop.addTimeEvent(
        CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 50'000, 0,
        [&](fcitx::EventSourceTime *, std::uint64_t) {
          const auto outcome = external_bridge.StartNormal(client->raw_handle(), external_scenes);
          if (outcome.kind != vinpst_fcitx_bridge::BridgeOutcome::Kind::Preedit) {
            fail("external client failed to start normal recording");
            return false;
          }
          partial_check = signal_loop.addTimeEvent(
              CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 500'000, 0,
              [&](fcitx::EventSourceTime *, std::uint64_t) {
                const auto current = input_context.inputPanel().preedit().toString();
                partial_seen = !current.empty() && current != "... Recording ..." &&
                               current != "... Recognizing ..." &&
                               current != "... Postprocessing ...";
                partial_text = current;
                if (!partial_seen) {
                  fail("addon did not automatically follow external recording partials");
                  return false;
                }
                const auto stop = external_bridge.Stop(client->raw_handle(), external_scenes);
                stop_attempted = true;
                if (stop.kind != vinpst_fcitx_bridge::BridgeOutcome::Kind::Commit) {
                  fail("external client failed to stop normal recording");
                  return false;
                }
                final_check = signal_loop.addTimeEvent(
                    CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 500'000, 0,
                    [&](fcitx::EventSourceTime *, std::uint64_t) {
                      if (input_context.committed != std::vector<std::string>{expected_text}) {
                        fail("addon did not automatically commit external recording result");
                        return false;
                      }
                      signal_loop.exit();
                      return false;
                    });
                final_check->setOneShot();
                return false;
              });
          partial_check->setOneShot();
          return false;
        });
    delayed_start->setOneShot();
    const bool loop_result = signal_loop.exec();
    if (!loop_result || !failure.empty() || !partial_seen || !stop_attempted) {
      std::cerr << "native external-session flow failed: loop=" << loop_result
                << " failure=" << failure << " partial=" << partial_seen
                << " stop=" << stop_attempted << " preedit=" << partial_text << '\n';
      return 1;
    }
    if (!input_context.deleted.empty() ||
        input_context.committed != std::vector<std::string>{expected_text} ||
        input_context.inputPanel().candidateList() != nullptr ||
        !input_context.inputPanel().preedit().empty()) {
      std::cerr << "native external-session result left incorrect InputContext state\n";
      return 1;
    }
    std::cout << "native addon external-session commit: " << expected_text
              << " (partial: " << partial_text << ")\n";
    return 0;
  }

  AppliedOutcome start = AppliedOutcome::None;
  AppliedOutcome stop = AppliedOutcome::None;
  bool start_attempted = false;
  bool stop_attempted = false;
  bool partial_seen = false;
  bool post_stop_timer_fired = false;
  std::string partial_text;
  std::unique_ptr<fcitx::EventSourceTime> partial_check;
  std::unique_ptr<fcitx::EventSourceTime> post_stop_probe;
  std::unique_ptr<fcitx::EventSourceTime> final_check;
  std::string failure;
  auto fail = [&](std::string message) {
    if (failure.empty()) {
      failure = std::move(message);
    }
    signal_loop.exit();
  };
  auto delayed_start = signal_loop.addTimeEvent(
      CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 50'000, 0,
      [&](fcitx::EventSourceTime *, std::uint64_t) {
        start = addon.ApplyTriggerAction(&input_context,
                                         command_mode ? FcitxTriggerAction::StartCommand
                                                      : FcitxTriggerAction::StartNormal,
                                         selected_text);
        start_attempted = true;
        partial_check = signal_loop.addTimeEvent(
            CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 500'000, 0,
            [&](fcitx::EventSourceTime *, std::uint64_t) {
              const auto current = input_context.inputPanel().preedit().toString();
              partial_seen = !current.empty() && current != "... Recording ..." &&
                             current != "... Commanding ..." &&
                             current != "... Recognizing ..." &&
                             current != "... Postprocessing ...";
              partial_text = current;
              if (!partial_seen) {
                fail("native addon did not render a live partial preedit");
                return false;
              }
              stop = addon.ApplyTriggerAction(
                  &input_context,
                  command_mode ? FcitxTriggerAction::StopCommand
                               : FcitxTriggerAction::StopNormal);
              stop_attempted = true;
              post_stop_probe = signal_loop.addTimeEvent(
                  CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 10'000, 0,
                  [&](fcitx::EventSourceTime *, std::uint64_t) {
                    post_stop_timer_fired = true;
                    return false;
                  });
              post_stop_probe->setOneShot();
              final_check = signal_loop.addTimeEvent(
                  CLOCK_MONOTONIC, fcitx::now(CLOCK_MONOTONIC) + 1'500'000, 0,
                  [&](fcitx::EventSourceTime *, std::uint64_t) {
                    const bool final_ready = command_mode
                                                 ? (input_context.inputPanel().candidateList() !=
                                                        nullptr ||
                                                    (!expect_candidate_menu &&
                                                     input_context.committed ==
                                                         std::vector<std::string>{expected_text} &&
                                                     !input_context.deleted.empty()))
                                                 : input_context.committed ==
                                                       std::vector<std::string>{expected_text};
                    if (!final_ready) {
                      const auto candidate_list = input_context.inputPanel().candidateList();
                      fail("native addon final result did not arrive asynchronously: committed=" +
                           std::to_string(input_context.committed.size()) + " deleted=" +
                           std::to_string(input_context.deleted.size()) + " candidates=" +
                           std::to_string(candidate_list == nullptr ? 0 : candidate_list->size()) +
                           " preedit=" + input_context.inputPanel().preedit().toString());
                      return false;
                    }
                    signal_loop.exit();
                    return false;
                  });
              final_check->setOneShot();
              return false;
            });
        partial_check->setOneShot();
        return false;
      });
  delayed_start->setOneShot();
  const bool loop_result = signal_loop.exec();
  if (!loop_result || !failure.empty() || !start_attempted || !stop_attempted ||
      start != AppliedOutcome::Preedit || stop != AppliedOutcome::None || !partial_seen ||
      !post_stop_timer_fired) {
    std::cerr << "native addon async flow failed: loop=" << loop_result
              << " failure=" << failure << " start-attempted=" << start_attempted
              << " start=" << static_cast<int>(start) << " stop-attempted=" << stop_attempted
              << " stop=" << static_cast<int>(stop) << " partial=" << partial_seen
              << " post-stop-timer=" << post_stop_timer_fired
              << " preedit=" << partial_text << '\n';
    return 1;
  }
  delayed_start.reset();
  partial_check.reset();
  post_stop_probe.reset();
  final_check.reset();

  if (command_mode) {
    auto candidate_list = input_context.inputPanel().candidateList();
    const auto selected_size = static_cast<unsigned int>(selected_text.size());
    const auto expected_deletion = std::vector<std::pair<int, unsigned int>>{
        {-static_cast<int>(selected_size), selected_size}};
    if (candidate_list != nullptr) {
      if (candidate_list->size() != 2 ||
          candidate_list->candidate(0).text().toString() != selected_text ||
          candidate_list->candidate(1).text().toString() != expected_text ||
          !input_context.committed.empty() || !input_context.deleted.empty()) {
        std::cerr << "native command menu did not expose selected and ASR candidates cleanly\n";
        return 1;
      }
      candidate_list->candidate(1).select(&input_context);
      if (input_context.deleted != expected_deletion ||
          input_context.committed != std::vector<std::string>{expected_text}) {
        std::cerr << "native command selection did not replace the selected text\n";
        return 1;
      }
    } else if (expect_candidate_menu) {
      std::cerr << "native command fixture required a candidate menu but none was shown\n";
      return 1;
    } else if (input_context.deleted != expected_deletion ||
               input_context.committed != std::vector<std::string>{expected_text}) {
      std::cerr << "native command direct result did not replace the selected text\n";
      return 1;
    }
  } else if (!input_context.deleted.empty() ||
             input_context.committed != std::vector<std::string>{expected_text}) {
    std::cerr << "native normal result did not commit through InputContext\n";
    return 1;
  }

  if (input_context.inputPanel().candidateList() != nullptr ||
      !input_context.inputPanel().preedit().empty()) {
    std::cerr << "native addon left stale InputContext UI state\n";
    return 1;
  }

  std::cout << (command_mode ? "native addon command replacement: "
                             : "native addon InputContext commit: ")
            << expected_text << " (partial: " << partial_text << ")\n";
  return 0;
}
