#pragma once

#include "vinpst_fcitx_bridge/fcitx_candidates.h"
#include "vinpst_fcitx_bridge/frontend_bridge.h"

#include <cstdint>

namespace fcitx {
class InputContext;
}

namespace vinpst_fcitx_bridge {

enum class AppliedOutcome : std::uint8_t {
  None,
  Preedit,
  Clear,
  Commit,
  CandidateMenu
};

AppliedOutcome ApplyBridgeOutcomeToInputContext(
    const BridgeOutcome &outcome, fcitx::InputContext *ic,
    ResultCandidateSelectCallback on_candidate_select = {});

} // namespace vinpst_fcitx_bridge
