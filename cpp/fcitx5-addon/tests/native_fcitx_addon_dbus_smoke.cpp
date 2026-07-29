#include "vinput_fcitx_bridge/fcitx_addon.h"

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
      : fcitx::InputContext(manager, "vinput-native-addon-smoke") {
    created();
  }

  ~TestInputContext() override {
    destroy();
  }

  const char *frontend() const override {
    return "vinput-native-addon-smoke";
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
  using vinput_fcitx_bridge::AppliedOutcome;
  using vinput_fcitx_bridge::FcitxTriggerAction;
  using vinput_fcitx_bridge::FcitxVinputAddon;

  const auto expected_text =
      RequiredEnvironment("VINPUT_NATIVE_FRONTEND_EXPECTED_TEXT");
  const auto selected_text = OptionalEnvironment("VINPUT_NATIVE_ADDON_SELECTED_TEXT");
  const bool command_mode = !selected_text.empty();

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

  FcitxVinputAddon addon(nullptr, &signal_bus);
  AppliedOutcome start = AppliedOutcome::None;
  bool start_attempted = false;
  bool partial_seen = false;
  std::string partial_text;
  std::unique_ptr<fcitx::EventSourceTime> partial_check;
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
              signal_loop.exit();
              return false;
            });
        partial_check->setOneShot();
        return false;
      });
  delayed_start->setOneShot();
  const bool loop_result = signal_loop.exec();
  if (!loop_result || !start_attempted || start != AppliedOutcome::Preedit ||
      !addon.bridge().recording() || addon.bridge().command_mode() != command_mode ||
      !partial_seen) {
    std::cerr << "native addon did not render a live partial preedit: loop="
              << loop_result << " start-attempted=" << start_attempted
              << " start=" << static_cast<int>(start)
              << " recording=" << addon.bridge().recording()
              << " command=" << addon.bridge().command_mode()
              << " partial=" << partial_seen << " preedit=" << partial_text << '\n';
    return 1;
  }
  delayed_start.reset();
  partial_check.reset();

  const auto stop = addon.ApplyTriggerAction(
      &input_context,
      command_mode ? FcitxTriggerAction::StopCommand : FcitxTriggerAction::StopNormal);
  const auto expected_outcome =
      command_mode ? AppliedOutcome::CandidateMenu : AppliedOutcome::Commit;
  if (stop != expected_outcome || addon.bridge().recording() ||
      addon.bridge().command_mode()) {
    std::cerr << "native addon stop did not apply the expected outcome: applied="
              << static_cast<int>(stop) << " recording=" << addon.bridge().recording()
              << " command=" << addon.bridge().command_mode() << '\n';
    return 1;
  }

  if (command_mode) {
    if (!input_context.committed.empty() || !input_context.deleted.empty()) {
      std::cerr << "native command menu mutated the selection before user choice\n";
      return 1;
    }

    auto candidate_list = input_context.inputPanel().candidateList();
    if (candidate_list == nullptr || candidate_list->size() != 2 ||
        candidate_list->candidate(0).text().toString() != selected_text ||
        candidate_list->candidate(1).text().toString() != expected_text) {
      std::cerr << "native command menu did not expose selected and ASR candidates\n";
      return 1;
    }

    candidate_list->candidate(1).select(&input_context);
    const auto selected_size = static_cast<unsigned int>(selected_text.size());
    if (input_context.deleted !=
            std::vector<std::pair<int, unsigned int>>{
                {-static_cast<int>(selected_size), selected_size}} ||
        input_context.committed != std::vector<std::string>{expected_text}) {
      std::cerr << "native command selection did not replace the selected text\n";
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
