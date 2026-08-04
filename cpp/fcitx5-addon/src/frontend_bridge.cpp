#include "vinput_fcitx_bridge/frontend_bridge.h"

#include "vinput_fcitx_bridge/fcitx_menu_projection.h"
#include "vinput_fcitx_bridge/rust_handle.h"
#include "vinput_fcitx_bridge/rust_string.h"

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

using FrontendOutcomeHandle =
    RustOwnedHandle<VinputFcitxFrontendOutcome, vinput_fcitx_frontend_outcome_free>;
using FrontendPresentationHandle =
    RustOwnedHandle<VinputFcitxFrontendPresentation,
                    vinput_fcitx_frontend_presentation_free>;

BridgeOutcome FfiFailure() {
  return BridgeOutcome{
      BridgeOutcome::Kind::Error, "Voice input daemon is unavailable.", {}};
}

BridgeOutcome TakeOutcome(VinputFcitxFrontendOutcome *raw_outcome,
                          std::string_view original, std::string_view voice_command,
                          std::string_view cancel) {
  auto outcome = FrontendOutcomeHandle::Adopt(raw_outcome);
  const VinputFcitxFrontendPresentationTextView text{
      .original = ToRustStringView(original),
      .voice_command = ToRustStringView(voice_command),
      .cancel = ToRustStringView(cancel),
  };
  auto presentation =
      std::make_shared<FrontendPresentationHandle>(FrontendPresentationHandle::Adopt(
          vinput_fcitx_frontend_presentation_new(outcome.raw_handle(), &text)));
  VinputFcitxFrontendPresentationView view{};
  if (vinput_fcitx_frontend_presentation_view(presentation->raw_handle(), &view) == 0 ||
      view.kind > VINPUT_FCITX_FRONTEND_OUTCOME_ERROR) {
    return FfiFailure();
  }

  CandidatePresentation candidate_menu{
      .candidate_count = view.candidate_count,
      .cursor_index = view.cursor_index,
      .candidate_at =
          [presentation](std::size_t index) -> std::optional<PresentedCandidate> {
        VinputFcitxPresentedCandidateView candidate{};
        if (vinput_fcitx_frontend_presentation_candidate(presentation->raw_handle(),
                                                         index, &candidate) == 0) {
          return std::nullopt;
        }
        return PresentedCandidate{CopyRustString(candidate.text),
                                  CopyRustString(candidate.comment),
                                  candidate.commit != 0};
      },
  };
  return BridgeOutcome{static_cast<BridgeOutcome::Kind>(view.kind),
                       CopyRustString(view.text), std::move(candidate_menu),
                       view.replace_selection != 0};
}

} // namespace

FrontendBridge::FrontendBridge()
    : controller_(ControllerHandle::Adopt(vinput_fcitx_frontend_controller_new())) {}

FrontendTriggerIntent
FrontendBridge::PlanTrigger(FrontendTriggerRequest request) const {
  std::uint8_t intent = VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_NONE;
  if (vinput_fcitx_frontend_controller_plan_trigger(
          controller_.raw_handle(), static_cast<std::uint8_t>(request), &intent) == 0 ||
      intent > VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU) {
    return FrontendTriggerIntent::None;
  }
  return static_cast<FrontendTriggerIntent>(intent);
}

bool FrontendBridge::recording() const {
  return vinput_fcitx_frontend_controller_recording(controller_.raw_handle()) != 0;
}

bool FrontendBridge::command_mode() const {
  return vinput_fcitx_frontend_controller_command_mode(controller_.raw_handle()) != 0;
}

BridgeOutcome FrontendBridge::StartNormal(const VinputFcitxDaemonClient *client,
                                          const SceneMenuController &scene_controller) {
  return TakeOutcome(
      vinput_fcitx_frontend_controller_start_normal_with_daemon(
          controller_.mutable_raw_handle(), client, scene_controller.raw_handle()),
      original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome FrontendBridge::StartCommand(const VinputFcitxDaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return TakeOutcome(vinput_fcitx_frontend_controller_start_command_with_daemon(
                         controller_.mutable_raw_handle(), client,
                         RustBytes(selected_text), selected_text.size(),
                         RustBytes(scene_id), scene_id.size()),
                     original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome FrontendBridge::Stop(const VinputFcitxDaemonClient *client,
                                   const SceneMenuController &scene_controller) {
  return TakeOutcome(
      vinput_fcitx_frontend_controller_stop_with_daemon(
          controller_.mutable_raw_handle(), client, scene_controller.raw_handle()),
      original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome
FrontendBridge::AdoptAndStop(const VinputFcitxDaemonClient *client, bool command_mode,
                             const SceneMenuController &scene_controller) {
  return TakeOutcome(vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
                         controller_.mutable_raw_handle(), client,
                         command_mode ? 1U : 0U, scene_controller.raw_handle()),
                     original_text_, voice_command_text_, cancel_text_);
}

void FrontendBridge::SetPresentationText(std::string original,
                                         std::string voice_command,
                                         std::string cancel) {
  original_text_ = std::move(original);
  voice_command_text_ = std::move(voice_command);
  cancel_text_ = std::move(cancel);
}

void FrontendBridge::Reset() {
  static_cast<void>(
      vinput_fcitx_frontend_controller_reset(controller_.mutable_raw_handle()));
}

} // namespace vinput_fcitx_bridge
