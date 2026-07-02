#include "vinput_fcitx_bridge/fcitx_outcome.h"

#include <cassert>
#include <string>
#include <string_view>
#include <utility>
#include <vector>

using vinput_fcitx_bridge::AppliedOutcome;
using vinput_fcitx_bridge::ApplyBridgeOutcomeToSink;
using vinput_fcitx_bridge::BridgeOutcome;
using vinput_fcitx_bridge::Candidate;
using vinput_fcitx_bridge::CandidateSource;
using vinput_fcitx_bridge::OutcomeSink;
using vinput_fcitx_bridge::RecognitionPayload;

class FakeOutcomeSink final : public OutcomeSink {
public:
  void SetPreedit(std::string_view text) override {
    preedit = std::string(text);
    events.push_back("preedit:" + preedit);
  }

  void ClearPreedit() override {
    preedit.clear();
    events.emplace_back("clear-preedit");
  }

  void ClearCandidateMenu() override {
    ++clear_candidate_menu_calls;
    events.emplace_back("clear-candidates");
  }

  void DeleteSelectedTextIfAny() override {
    ++delete_selected_text_calls;
    events.emplace_back("delete-selected");
  }

  void CommitString(std::string_view text) override {
    committed_texts.emplace_back(text);
    events.push_back("commit:" + std::string(text));
  }

  bool ShowCandidateMenu(const RecognitionPayload &payload,
                         bool command_mode) override {
    ++show_candidate_menu_calls;
    last_candidate_count = payload.candidates.size();
    last_candidate_commit_text = payload.commit_text;
    last_candidate_command_mode = command_mode;
    events.emplace_back("show-candidates");
    return show_candidate_menu_result;
  }

  bool show_candidate_menu_result = false;
  std::string preedit;
  std::vector<std::string> committed_texts;
  std::vector<std::string> events;
  int clear_candidate_menu_calls = 0;
  int delete_selected_text_calls = 0;
  int show_candidate_menu_calls = 0;
  std::size_t last_candidate_count = 0;
  std::string last_candidate_commit_text;
  bool last_candidate_command_mode = false;
};

BridgeOutcome Outcome(BridgeOutcome::Kind kind, std::string text = {}) {
  BridgeOutcome outcome;
  outcome.kind = kind;
  outcome.text = std::move(text);
  return outcome;
}

int main() {
  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(Outcome(BridgeOutcome::Kind::None), sink) ==
           AppliedOutcome::None);
    assert(sink.events.empty());
  }

  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(
               Outcome(BridgeOutcome::Kind::Preedit, "... Recording ..."), sink) ==
           AppliedOutcome::Preedit);
    assert(sink.preedit == "... Recording ...");
    assert((sink.events == std::vector<std::string>{"preedit:... Recording ..."}));
  }

  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(
               Outcome(BridgeOutcome::Kind::Error, "daemon unavailable"), sink) ==
           AppliedOutcome::Preedit);
    assert(sink.preedit == "daemon unavailable");
  }

  {
    FakeOutcomeSink sink;
    sink.preedit = "old preedit";
    assert(ApplyBridgeOutcomeToSink(Outcome(BridgeOutcome::Kind::Clear), sink) ==
           AppliedOutcome::Clear);
    assert(sink.preedit.empty());
    assert((sink.events == std::vector<std::string>{"clear-preedit"}));
  }

  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(Outcome(BridgeOutcome::Kind::Commit, "final"),
                                    sink) == AppliedOutcome::Commit);
    assert(sink.delete_selected_text_calls == 0);
    assert(sink.clear_candidate_menu_calls == 1);
    assert((sink.committed_texts == std::vector<std::string>{"final"}));
    assert((sink.events == std::vector<std::string>{"clear-candidates", "clear-preedit",
                                                    "commit:final"}));
  }

  {
    FakeOutcomeSink sink;
    auto outcome = Outcome(BridgeOutcome::Kind::Commit, "command final");
    outcome.command_mode = true;
    assert(ApplyBridgeOutcomeToSink(outcome, sink) == AppliedOutcome::Commit);
    assert(sink.delete_selected_text_calls == 1);
    assert((sink.events == std::vector<std::string>{"delete-selected",
                                                    "clear-candidates", "clear-preedit",
                                                    "commit:command final"}));
  }

  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(Outcome(BridgeOutcome::Kind::Commit), sink) ==
           AppliedOutcome::None);
    assert(sink.events.empty());
  }

  {
    FakeOutcomeSink sink;
    sink.show_candidate_menu_result = true;
    BridgeOutcome outcome = Outcome(BridgeOutcome::Kind::CandidateMenu);
    outcome.payload.commit_text = "polished";
    outcome.payload.candidates = {
        Candidate{"raw transcript", CandidateSource::Raw},
        Candidate{"polished", CandidateSource::Llm},
    };
    outcome.command_mode = true;

    assert(ApplyBridgeOutcomeToSink(outcome, sink) == AppliedOutcome::CandidateMenu);
    assert(sink.show_candidate_menu_calls == 1);
    assert(sink.last_candidate_count == 2);
    assert(sink.last_candidate_commit_text == "polished");
    assert(sink.last_candidate_command_mode);
    assert(sink.committed_texts.empty());
    assert((sink.events == std::vector<std::string>{"show-candidates"}));
  }

  {
    FakeOutcomeSink sink;
    BridgeOutcome outcome = Outcome(BridgeOutcome::Kind::CandidateMenu);
    outcome.payload.commit_text = "fallback commit";

    assert(ApplyBridgeOutcomeToSink(outcome, sink) == AppliedOutcome::Commit);
    assert(sink.show_candidate_menu_calls == 1);
    assert(sink.delete_selected_text_calls == 0);
    assert((sink.committed_texts == std::vector<std::string>{"fallback commit"}));
    assert((sink.events == std::vector<std::string>{"show-candidates",
                                                    "clear-candidates", "clear-preedit",
                                                    "commit:fallback commit"}));
  }

  {
    FakeOutcomeSink sink;
    BridgeOutcome outcome = Outcome(BridgeOutcome::Kind::CandidateMenu, "explicit");
    outcome.payload.commit_text = "payload fallback";
    outcome.command_mode = true;

    assert(ApplyBridgeOutcomeToSink(outcome, sink) == AppliedOutcome::Commit);
    assert(sink.delete_selected_text_calls == 1);
    assert((sink.committed_texts == std::vector<std::string>{"explicit"}));
    assert((sink.events == std::vector<std::string>{
                               "show-candidates", "delete-selected", "clear-candidates",
                               "clear-preedit", "commit:explicit"}));
  }

  {
    FakeOutcomeSink sink;
    assert(ApplyBridgeOutcomeToSink(Outcome(BridgeOutcome::Kind::CandidateMenu),
                                    sink) == AppliedOutcome::None);
    assert(sink.show_candidate_menu_calls == 1);
    assert((sink.events == std::vector<std::string>{"show-candidates"}));
  }

  return 0;
}
