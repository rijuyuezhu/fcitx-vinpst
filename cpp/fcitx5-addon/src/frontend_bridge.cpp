#include "vinput_fcitx_bridge/frontend_bridge.h"

#include "vinput_fcitx_bridge/menu_snapshot.h"

#include "vinput_fcitx_ffi.h"

#include <memory>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(FrontendTriggerRequest::None) ==
              VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_NONE);
static_assert(
    static_cast<std::uint8_t>(FrontendTriggerRequest::ConsumeAsrMenuRelease) ==
    VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE);
static_assert(static_cast<std::uint8_t>(FrontendTriggerIntent::None) ==
              VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_NONE);
static_assert(static_cast<std::uint8_t>(FrontendTriggerIntent::ShowAsrMenu) ==
              VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU);

struct OutcomeDeleter {
  void operator()(VinputFcitxFrontendOutcome *outcome) const {
    vinput_fcitx_frontend_outcome_free(outcome);
  }
};

const std::uint8_t *Bytes(std::string_view text) {
  return text.empty() ? nullptr : reinterpret_cast<const std::uint8_t *>(text.data());
}

std::string_view StringView(VinputFcitxStringView view) {
  if (view.data == nullptr || view.len == 0) {
    return {};
  }
  return {reinterpret_cast<const char *>(view.data), view.len};
}

CandidateSource CandidateSourceFromValue(std::uint8_t source) {
  switch (source) {
  case VINPUT_FCITX_CANDIDATE_SOURCE_LLM:
    return CandidateSource::Llm;
  case VINPUT_FCITX_CANDIDATE_SOURCE_ASR:
    return CandidateSource::Asr;
  case VINPUT_FCITX_CANDIDATE_SOURCE_CANCEL:
    return CandidateSource::Cancel;
  default:
    return CandidateSource::Raw;
  }
}

BridgeOutcome FfiFailure() {
  return BridgeOutcome{
      BridgeOutcome::Kind::Error, "Voice input daemon is unavailable.", {}};
}

BridgeOutcome TakeOutcome(VinputFcitxFrontendOutcome *raw_outcome) {
  std::unique_ptr<VinputFcitxFrontendOutcome, OutcomeDeleter> outcome(raw_outcome);
  VinputFcitxFrontendOutcomeView view{};
  if (vinput_fcitx_frontend_outcome_view(outcome.get(), &view) == 0 ||
      view.kind > VINPUT_FCITX_FRONTEND_OUTCOME_ERROR) {
    return FfiFailure();
  }

  RecognitionPayload payload;
  payload.commit_text = StringView(view.commit_text);
  payload.candidates.reserve(view.candidate_count);
  for (std::size_t index = 0; index < view.candidate_count; ++index) {
    VinputFcitxCandidateView candidate{};
    if (vinput_fcitx_frontend_outcome_candidate(outcome.get(), index, &candidate) ==
        0) {
      return FfiFailure();
    }
    payload.candidates.push_back(Candidate{std::string(StringView(candidate.text)),
                                           CandidateSourceFromValue(candidate.source)});
  }
  return BridgeOutcome{static_cast<BridgeOutcome::Kind>(view.kind),
                       std::string(StringView(view.text)), std::move(payload),
                       view.command_mode != 0};
}

} // namespace

FrontendBridge::FrontendBridge()
    : controller_(vinput_fcitx_frontend_controller_new()) {}

FrontendBridge::~FrontendBridge() {
  vinput_fcitx_frontend_controller_free(controller_);
}

FrontendTriggerIntent
FrontendBridge::PlanTrigger(FrontendTriggerRequest request) const {
  std::uint8_t intent = VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_NONE;
  if (vinput_fcitx_frontend_controller_plan_trigger(
          controller_, static_cast<std::uint8_t>(request), &intent) == 0 ||
      intent > VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU) {
    return FrontendTriggerIntent::None;
  }
  return static_cast<FrontendTriggerIntent>(intent);
}

bool FrontendBridge::recording() const {
  return vinput_fcitx_frontend_controller_recording(controller_) != 0;
}

bool FrontendBridge::command_mode() const {
  return vinput_fcitx_frontend_controller_command_mode(controller_) != 0;
}

BridgeOutcome FrontendBridge::StartNormal(const VinputFcitxDaemonClient *client,
                                          const SceneStateSnapshot &scene_state) {
  return TakeOutcome(vinput_fcitx_frontend_controller_start_normal_with_daemon(
      controller_, client, scene_state.raw_handle()));
}

BridgeOutcome FrontendBridge::StartCommand(const VinputFcitxDaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return TakeOutcome(vinput_fcitx_frontend_controller_start_command_with_daemon(
      controller_, client, Bytes(selected_text), selected_text.size(), Bytes(scene_id),
      scene_id.size()));
}

BridgeOutcome FrontendBridge::Stop(const VinputFcitxDaemonClient *client,
                                   const SceneStateSnapshot &scene_state) {
  return TakeOutcome(vinput_fcitx_frontend_controller_stop_with_daemon(
      controller_, client, scene_state.raw_handle()));
}

BridgeOutcome FrontendBridge::AdoptAndStop(const VinputFcitxDaemonClient *client,
                                           bool command_mode,
                                           const SceneStateSnapshot &scene_state) {
  return TakeOutcome(vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
      controller_, client, command_mode ? 1U : 0U, scene_state.raw_handle()));
}

void FrontendBridge::Reset() {
  static_cast<void>(vinput_fcitx_frontend_controller_reset(controller_));
}

} // namespace vinput_fcitx_bridge
