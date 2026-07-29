#include "vinput_fcitx_bridge/fcitx_outcome.h"

#include <fcitx/candidatelist.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputcontextmanager.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>

#include <cassert>
#include <string>
#include <utility>
#include <vector>

namespace {

class TestInputContext final : public fcitx::InputContext {
public:
  explicit TestInputContext(fcitx::InputContextManager &manager)
      : fcitx::InputContext(manager, "vinput-outcome-smoke") {
    created();
  }

  ~TestInputContext() override {
    destroy();
  }

  const char *frontend() const override {
    return "vinput-outcome-smoke";
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

vinput_fcitx_bridge::BridgeOutcome CommandCandidateOutcome() {
  using vinput_fcitx_bridge::BridgeOutcome;
  using vinput_fcitx_bridge::Candidate;
  using vinput_fcitx_bridge::CandidateSource;

  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::CandidateMenu;
  outcome.command_mode = true;
  outcome.payload.commit_text = "replace this text";
  outcome.payload.candidates = {
      Candidate{"replace this text", CandidateSource::Raw},
      Candidate{"native voice command", CandidateSource::Asr},
  };
  return outcome;
}

} // namespace

int main() {
  using vinput_fcitx_bridge::AppliedOutcome;
  using vinput_fcitx_bridge::ApplyBridgeOutcomeToInputContext;
  using vinput_fcitx_bridge::BridgeOutcome;

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
    input_context.surroundingText().setText("head selected", 5, 13);

    BridgeOutcome outcome;
    outcome.kind = BridgeOutcome::Kind::Commit;
    outcome.command_mode = true;
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

    auto outcome = CommandCandidateOutcome();
    outcome.payload.candidates[1].source = vinput_fcitx_bridge::CandidateSource::Cancel;
    outcome.payload.candidates[1].text.clear();

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
