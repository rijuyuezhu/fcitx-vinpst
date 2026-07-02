#pragma once

#include "vinput_fcitx_bridge/frontend_bridge.h"

#include <cstdint>
#include <string_view>

namespace fcitx {
class InputContext;
}

namespace vinput_fcitx_bridge {

enum class AppliedOutcome : std::uint8_t {
  None,
  Preedit,
  Clear,
  Commit,
  CandidateMenu
};

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

AppliedOutcome ApplyBridgeOutcomeToSink(const BridgeOutcome &outcome,
                                        OutcomeSink &sink);

AppliedOutcome ApplyBridgeOutcomeToInputContext(const BridgeOutcome &outcome,
                                                fcitx::InputContext *ic);

} // namespace vinput_fcitx_bridge
