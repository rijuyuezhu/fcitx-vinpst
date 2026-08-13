#include "vinpst_fcitx_bridge/fcitx_outcome.h"

#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>

#include <cassert>
#include <memory>
#include <string>
#include <utility>
#include <vector>

namespace {

class TestInputContext final : public fcitx::InputContext {
public:
  explicit TestInputContext(fcitx::InputContextManager &manager)
      : fcitx::InputContext(manager, "vinpst-outcome-smoke") {
    created();
  }

  ~TestInputContext() override {
    destroy();
  }

  const char *frontend() const override {
    return "vinpst-outcome-smoke";
  }

  std::vector<std::string> committed;
  std::vector<std::pair<int, unsigned int>> deleted;
  int preedit_updates = 0;

protected:
  void commitStringImpl(const std::string &text) override {
    committed.push_back(text);
  }

  void deleteSurroundingTextImpl(int offset, unsigned int size) override {
    deleted.emplace_back(offset, size);
  }

  void forwardKeyImpl(const fcitx::ForwardKeyEvent &) override {}

  void updatePreeditImpl() override {
    ++preedit_updates;
  }
};

vinpst_fcitx_bridge::BridgeOutcome CommandCandidateOutcome(bool cancel = false) {
  using vinpst_fcitx_bridge::BridgeOutcome;
  using vinpst_fcitx_bridge::PresentedCandidate;

  auto rows = std::make_shared<const std::vector<PresentedCandidate>>(
      std::vector<PresentedCandidate>{
          PresentedCandidate{"replace this text", "Original", true},
          cancel ? PresentedCandidate{"", "Cancel", false}
                 : PresentedCandidate{"native voice command", "Voice Command", true},
      });
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::CandidateMenu;
  outcome.replace_selection = true;
  outcome.text = "replace this text";
  outcome.candidate_menu = {
      .candidate_count = rows->size(),
      .cursor_index = 0,
      .candidate_at = [rows](std::size_t index) -> std::optional<PresentedCandidate> {
        if (index >= rows->size()) {
          return std::nullopt;
        }
        return (*rows)[index];
      },
  };
  return outcome;
}

} // namespace

int main() {
  using vinpst_fcitx_bridge::AppliedOutcome;
  using vinpst_fcitx_bridge::ApplyBridgeOutcomeToInputContext;
  using vinpst_fcitx_bridge::BridgeOutcome;

  fcitx::InputContextManager manager;

  {
    TestInputContext input_context(manager);
    input_context.surroundingText().setText("replace this text", 17, 0);

    const auto applied =
        ApplyBridgeOutcomeToInputContext(CommandCandidateOutcome(), &input_context);
    assert(applied == AppliedOutcome::CandidateMenu);
    assert(input_context.committed.empty());
    assert(input_context.deleted.empty());
    assert(input_context.inputPanel().preedit().empty());
    assert(!input_context.inputPanel().auxUp().empty());

    auto candidate_list = input_context.inputPanel().candidateList();
    assert(candidate_list != nullptr);
    assert(candidate_list->size() == 2);
    candidate_list->candidate(1).select(&input_context);

    assert((input_context.deleted ==
            std::vector<std::pair<int, unsigned int>>{{-17, 17}}));
    assert(
        (input_context.committed == std::vector<std::string>{"native voice command"}));
    assert(input_context.inputPanel().candidateList() == nullptr);
    assert(input_context.inputPanel().preedit().empty());
  }

  {
    TestInputContext input_context(manager);
    assert(
        ApplyBridgeOutcomeToInputContext(CommandCandidateOutcome(), &input_context) ==
        AppliedOutcome::CandidateMenu);
    const auto aux_up = input_context.inputPanel().auxUp().toString();
    BridgeOutcome outcome;
    outcome.kind = BridgeOutcome::Kind::Preedit;
    outcome.text = "... Commanding ...";

    assert(ApplyBridgeOutcomeToInputContext(outcome, &input_context) ==
           AppliedOutcome::Preedit);
    assert(input_context.inputPanel().preedit().toString() == "... Commanding ...");
    assert(input_context.inputPanel().auxUp().toString() == aux_up);
    auto candidate_list = input_context.inputPanel().candidateList();
    assert(candidate_list != nullptr);
    assert(candidate_list->size() == 2);
  }

  {
    TestInputContext input_context(manager);
    input_context.surroundingText().setText("head selected", 5, 13);

    BridgeOutcome outcome;
    outcome.kind = BridgeOutcome::Kind::Commit;
    outcome.replace_selection = true;
    outcome.text = "rewritten selection";

    const auto applied = ApplyBridgeOutcomeToInputContext(outcome, &input_context);
    assert(applied == AppliedOutcome::Commit);
    assert(
        (input_context.deleted == std::vector<std::pair<int, unsigned int>>{{0, 8}}));
    assert(
        (input_context.committed == std::vector<std::string>{"rewritten selection"}));
  }

  {
    TestInputContext input_context(manager);
    input_context.surroundingText().setText("keep selection", 14, 0);

    auto outcome = CommandCandidateOutcome(true);

    const auto applied = ApplyBridgeOutcomeToInputContext(outcome, &input_context);
    assert(applied == AppliedOutcome::CandidateMenu);
    auto candidate_list = input_context.inputPanel().candidateList();
    assert(candidate_list != nullptr);
    candidate_list->candidate(1).select(&input_context);
    assert(input_context.deleted.empty());
    assert(input_context.committed.empty());
  }

  assert(ApplyBridgeOutcomeToInputContext(BridgeOutcome{}, nullptr) ==
         AppliedOutcome::None);
  return 0;
}
