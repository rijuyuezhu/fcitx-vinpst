#include "vinpst_fcitx_bridge/frontend_bridge.h"

#include "vinpst_fcitx_bridge/fcitx_menu_projection.h"
#include "vinpst_fcitx_bridge/rust_handle.h"
#include "vinpst_fcitx_bridge/rust_string.h"

#include "vinpst_fcitx_ffi.h"

#include <memory>
#include <utility>

namespace vinpst_fcitx_bridge {
namespace {

static_assert(static_cast<std::uint8_t>(FrontendTriggerRequest::None) ==
              VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_NONE);
static_assert(
    static_cast<std::uint8_t>(FrontendTriggerRequest::ConsumeAsrMenuRelease) ==
    VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE);
static_assert(static_cast<std::uint8_t>(FrontendTriggerIntent::None) ==
              VINPST_FCITX_FRONTEND_TRIGGER_INTENT_NONE);
static_assert(static_cast<std::uint8_t>(FrontendTriggerIntent::ShowAsrMenu) ==
              VINPST_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU);

using FrontendOutcomeHandle =
    RustOwnedHandle<VinpstFcitxFrontendOutcome, vinpst_fcitx_frontend_outcome_free>;
using FrontendPresentationHandle =
    RustOwnedHandle<VinpstFcitxFrontendPresentation,
                    vinpst_fcitx_frontend_presentation_free>;

BridgeOutcome FfiFailure() {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Error;
  outcome.text = "Voice input daemon is unavailable.";
  return outcome;
}

BridgeOutcome TakeOutcome(VinpstFcitxFrontendOutcome *raw_outcome,
                          std::string_view original, std::string_view voice_command,
                          std::string_view cancel) {
  auto outcome = FrontendOutcomeHandle::Adopt(raw_outcome);
  const VinpstFcitxFrontendPresentationTextView text{
      .original = ToRustStringView(original),
      .voice_command = ToRustStringView(voice_command),
      .cancel = ToRustStringView(cancel),
  };
  auto presentation =
      std::make_shared<FrontendPresentationHandle>(FrontendPresentationHandle::Adopt(
          vinpst_fcitx_frontend_presentation_new(outcome.raw_handle(), &text)));
  VinpstFcitxFrontendPresentationView view{};
  if (vinpst_fcitx_frontend_presentation_view(presentation->raw_handle(), &view) == 0 ||
      view.kind > VINPST_FCITX_FRONTEND_OUTCOME_ERROR) {
    return FfiFailure();
  }

  CandidatePresentation candidate_menu{
      .candidate_count = view.candidate_count,
      .cursor_index = view.cursor_index,
      .candidate_at =
          [presentation](std::size_t index) -> std::optional<PresentedCandidate> {
        VinpstFcitxPresentedCandidateView candidate{};
        if (vinpst_fcitx_frontend_presentation_candidate(presentation->raw_handle(),
                                                         index, &candidate) == 0) {
          return std::nullopt;
        }
        return PresentedCandidate{
            CopyRustString(candidate.text), CopyRustString(candidate.comment),
            candidate.commit != 0, CopyRustString(candidate.context_source),
            candidate.suppress_commit_context != 0};
      },
  };
  std::vector<ContextEntryPresentation> context_entries;
  context_entries.reserve(view.context_entry_count);
  for (std::size_t index = 0; index < view.context_entry_count; ++index) {
    VinpstFcitxContextEntryView entry{};
    if (vinpst_fcitx_frontend_presentation_context_entry(presentation->raw_handle(),
                                                         index, &entry) == 0) {
      return FfiFailure();
    }
    context_entries.push_back(ContextEntryPresentation{CopyRustString(entry.text),
                                                       CopyRustString(entry.source)});
  }
  return BridgeOutcome{static_cast<BridgeOutcome::Kind>(view.kind),
                       CopyRustString(view.text),
                       std::move(candidate_menu),
                       view.replace_selection != 0,
                       std::move(context_entries),
                       view.suppress_commit_context != 0};
}

} // namespace

FrontendBridge::FrontendBridge()
    : controller_(ControllerHandle::Adopt(vinpst_fcitx_frontend_controller_new())) {}

FrontendTriggerIntent
FrontendBridge::PlanTrigger(FrontendTriggerRequest request) const {
  std::uint8_t intent = VINPST_FCITX_FRONTEND_TRIGGER_INTENT_NONE;
  if (vinpst_fcitx_frontend_controller_plan_trigger(
          controller_.raw_handle(), static_cast<std::uint8_t>(request), &intent) == 0 ||
      intent > VINPST_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU) {
    return FrontendTriggerIntent::None;
  }
  return static_cast<FrontendTriggerIntent>(intent);
}

bool FrontendBridge::recording() const {
  return vinpst_fcitx_frontend_controller_recording(controller_.raw_handle()) != 0;
}

bool FrontendBridge::command_mode() const {
  return vinpst_fcitx_frontend_controller_command_mode(controller_.raw_handle()) != 0;
}

BridgeOutcome FrontendBridge::StartNormal(const VinpstFcitxDaemonClient *client,
                                          const SceneMenuController &scene_controller) {
  return TakeOutcome(
      vinpst_fcitx_frontend_controller_start_normal_with_daemon(
          controller_.mutable_raw_handle(), client, scene_controller.raw_handle()),
      original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome FrontendBridge::StartCommand(const VinpstFcitxDaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return TakeOutcome(vinpst_fcitx_frontend_controller_start_command_with_daemon(
                         controller_.mutable_raw_handle(), client,
                         RustBytes(selected_text), selected_text.size(),
                         RustBytes(scene_id), scene_id.size()),
                     original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome FrontendBridge::Stop(const VinpstFcitxDaemonClient *client,
                                   const SceneMenuController &scene_controller) {
  return TakeOutcome(
      vinpst_fcitx_frontend_controller_stop_with_daemon(
          controller_.mutable_raw_handle(), client, scene_controller.raw_handle()),
      original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome
FrontendBridge::AdoptAndStop(const VinpstFcitxDaemonClient *client, bool command_mode,
                             const SceneMenuController &scene_controller) {
  return TakeOutcome(vinpst_fcitx_frontend_controller_adopt_and_stop_with_daemon(
                         controller_.mutable_raw_handle(), client,
                         command_mode ? 1U : 0U, scene_controller.raw_handle()),
                     original_text_, voice_command_text_, cancel_text_);
}

bool FrontendBridge::PrepareStartNormal(const SceneMenuController &scene_controller) {
  return vinpst_fcitx_frontend_controller_prepare_start_normal(
             controller_.mutable_raw_handle(), scene_controller.raw_handle()) != 0;
}

bool FrontendBridge::PrepareStartCommand(std::string_view selected_text,
                                         std::string_view scene_id) {
  return vinpst_fcitx_frontend_controller_prepare_start_command(
             controller_.mutable_raw_handle(), RustBytes(selected_text),
             selected_text.size(), RustBytes(scene_id), scene_id.size()) != 0;
}

bool FrontendBridge::PrepareStop(const SceneMenuController &scene_controller) {
  return vinpst_fcitx_frontend_controller_prepare_stop(
             controller_.mutable_raw_handle(), scene_controller.raw_handle()) != 0;
}

bool FrontendBridge::PrepareAdoptAndStop(bool command_mode,
                                         const SceneMenuController &scene_controller) {
  return vinpst_fcitx_frontend_controller_prepare_adopt_and_stop(
             controller_.mutable_raw_handle(), command_mode ? 1U : 0U,
             scene_controller.raw_handle()) != 0;
}

bool FrontendBridge::AdoptExternalRecording(
    bool command_mode, const SceneMenuController &scene_controller) {
  return vinpst_fcitx_frontend_controller_adopt_external_recording(
             controller_.mutable_raw_handle(), command_mode ? 1U : 0U,
             scene_controller.raw_handle()) != 0;
}

bool FrontendBridge::PendingArgument(std::string *argument) const {
  if (argument == nullptr) {
    return false;
  }
  VinpstFcitxStringView view{};
  if (vinpst_fcitx_frontend_controller_pending_argument(controller_.raw_handle(),
                                                        &view) == 0) {
    return false;
  }
  *argument = CopyRustString(view);
  return true;
}

BridgeOutcome FrontendBridge::Complete(bool success, std::string_view response) {
  return TakeOutcome(vinpst_fcitx_frontend_controller_complete(
                         controller_.mutable_raw_handle(), success ? 1U : 0U,
                         RustBytes(response), response.size()),
                     original_text_, voice_command_text_, cancel_text_);
}

BridgeOutcome FrontendBridge::CompleteRecognitionResult(std::string_view response) {
  return TakeOutcome(
      vinpst_fcitx_frontend_controller_complete_recognition_result(
          controller_.mutable_raw_handle(), RustBytes(response), response.size()),
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
      vinpst_fcitx_frontend_controller_reset(controller_.mutable_raw_handle()));
}

} // namespace vinpst_fcitx_bridge
