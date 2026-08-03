#include "vinput_fcitx_bridge/fcitx_outcome.h"

#include "vinput_fcitx_bridge/fcitx_candidates.h"
#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <string>
#include <string_view>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

class OutcomeSink {
public:
  virtual ~OutcomeSink() = default;

  virtual void SetPreedit(std::string_view text) = 0;
  virtual void ClearPreedit() = 0;
  virtual void ClearCandidateMenu() = 0;
  virtual void DeleteSelectedTextIfAny() = 0;
  virtual void CommitString(std::string_view text) = 0;
  virtual bool ShowCandidateMenu(const RecognitionPayload &payload,
                                 bool command_mode) = 0;
};

class FcitxInputContextSink final : public OutcomeSink {
public:
  explicit FcitxInputContextSink(fcitx::InputContext *input_context)
      : input_context_(input_context) {}

  void SetPreedit(std::string_view text) override {
    ClearCandidateMenu();
    fcitx::Text preedit;
    if (!text.empty()) {
      preedit.append(std::string(text));
    }
    input_context_->inputPanel().setPreedit(preedit);
    input_context_->updatePreedit();
    input_context_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  }

  void ClearPreedit() override {
    SetPreedit("");
  }

  void ClearCandidateMenu() override {
    ClearResultCandidateMenu(input_context_);
  }

  void DeleteSelectedTextIfAny() override {
    auto range = SelectedTextDeletionRange(input_context_->surroundingText());
    if (!range.has_value()) {
      return;
    }
    input_context_->deleteSurroundingText(range->offset, range->size);
  }

  void CommitString(std::string_view text) override {
    input_context_->commitString(std::string(text));
  }

  bool ShowCandidateMenu(const RecognitionPayload &payload,
                         bool command_mode) override {
    auto candidate_list = BuildResultCandidateList(
        payload,
        [command_mode](fcitx::InputContext *input_context, const Candidate &candidate) {
          ApplyResultCandidateSelection(input_context, candidate, command_mode);
        });
    if (candidate_list == nullptr) {
      return false;
    }
    ClearPreedit();
    fcitx::Text aux_up;
    aux_up.append(ResultCandidateMenuTitle(payload.candidates.size()));
    input_context_->inputPanel().setAuxUp(aux_up);
    input_context_->inputPanel().setCandidateList(std::move(candidate_list));
    input_context_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    return true;
  }

private:
  fcitx::InputContext *input_context_;
};

std::string_view CommitText(const BridgeOutcome &outcome) {
  if (!outcome.text.empty()) {
    return outcome.text;
  }
  return outcome.payload.commit_text;
}

AppliedOutcome ApplyBridgeOutcomeToSink(const BridgeOutcome &outcome,
                                        OutcomeSink &sink) {

  switch (outcome.kind) {
  case BridgeOutcome::Kind::None:
    return AppliedOutcome::None;
  case BridgeOutcome::Kind::Preedit:
  case BridgeOutcome::Kind::Error:
    sink.SetPreedit(outcome.text);
    return AppliedOutcome::Preedit;
  case BridgeOutcome::Kind::Clear:
    sink.ClearPreedit();
    return AppliedOutcome::Clear;
  case BridgeOutcome::Kind::Commit: {
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    if (outcome.command_mode) {
      sink.DeleteSelectedTextIfAny();
    }
    sink.ClearCandidateMenu();
    sink.ClearPreedit();
    sink.CommitString(text);
    return AppliedOutcome::Commit;
  }
  case BridgeOutcome::Kind::CandidateMenu:
    if (sink.ShowCandidateMenu(outcome.payload, outcome.command_mode)) {
      return AppliedOutcome::CandidateMenu;
    }
    const auto text = CommitText(outcome);
    if (text.empty()) {
      return AppliedOutcome::None;
    }
    if (outcome.command_mode) {
      sink.DeleteSelectedTextIfAny();
    }
    sink.ClearCandidateMenu();
    sink.ClearPreedit();
    sink.CommitString(text);
    return AppliedOutcome::Commit;
  }

  return AppliedOutcome::None;
}

} // namespace

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *ic) {
  if (ic == nullptr) {
    return AppliedOutcome::None;
  }
  FcitxInputContextSink sink(ic);
  return ApplyBridgeOutcomeToSink(outcome, sink);
}

} // namespace vinput_fcitx_bridge
