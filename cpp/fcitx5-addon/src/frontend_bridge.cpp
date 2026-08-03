#include "vinput_fcitx_bridge/frontend_bridge.h"

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

BridgeOutcome FfiFailure() {
  return BridgeOutcome{
      BridgeOutcome::Kind::Error, "Voice input daemon is unavailable.", {}};
}

BridgeOutcome TakeOutcome(VinputFcitxFrontendOutcome *raw_outcome) {
  std::unique_ptr<VinputFcitxFrontendOutcome, OutcomeDeleter> outcome(raw_outcome);
  const auto copied = CopyFrontendOutcome(outcome.get());
  if (!copied.has_value() || copied->kind > VINPUT_FCITX_FRONTEND_OUTCOME_ERROR) {
    return FfiFailure();
  }
  return BridgeOutcome{static_cast<BridgeOutcome::Kind>(copied->kind),
                       std::move(copied->text), std::move(copied->payload),
                       copied->command_mode};
}

BridgeOutcome RunPreparedCall(DaemonClient *client,
                              VinputFcitxFrontendController *controller,
                              std::uint8_t step,
                              VinputFcitxFrontendOutcome *immediate) {
  if (step == VINPUT_FCITX_FRONTEND_STEP_OUTCOME_READY) {
    return TakeOutcome(immediate);
  }
  if (step != VINPUT_FCITX_FRONTEND_STEP_CALL_READY || controller == nullptr) {
    vinput_fcitx_frontend_outcome_free(immediate);
    if (controller != nullptr) {
      static_cast<void>(vinput_fcitx_frontend_controller_reset(controller));
    }
    return FfiFailure();
  }

  std::uint8_t call_kind = VINPUT_FCITX_FRONTEND_CALL_NONE;
  VinputFcitxStringView argument{};
  if (vinput_fcitx_frontend_controller_pending_call(controller, &call_kind,
                                                    &argument) == 0) {
    return FfiFailure();
  }

  const auto call_argument = StringView(argument);
  std::string payload;
  std::string error;
  bool success = false;
  if (client != nullptr) {
    switch (call_kind) {
    case VINPUT_FCITX_FRONTEND_CALL_START_NORMAL:
      success = client->StartRecording(&error);
      break;
    case VINPUT_FCITX_FRONTEND_CALL_START_COMMAND:
      success = client->StartCommandRecording(call_argument, &error);
      break;
    case VINPUT_FCITX_FRONTEND_CALL_STOP:
      success = client->StopRecording(call_argument, &payload, &error);
      break;
    default:
      break;
    }
  }

  const auto &response = success ? payload : error;
  return TakeOutcome(vinput_fcitx_frontend_controller_complete(
      controller, success ? 1U : 0U, Bytes(response), response.size()));
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

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client) {
  return StartNormalWithScene(client, std::nullopt);
}

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client,
                                          std::string_view scene_id) {
  return StartNormalWithScene(client, scene_id);
}

BridgeOutcome
FrontendBridge::StartNormalWithScene(DaemonClient *client,
                                     std::optional<std::string_view> scene_id) {
  VinputFcitxFrontendOutcome *outcome = nullptr;
  const auto scene = scene_id.value_or(std::string_view{});
  const auto step = vinput_fcitx_frontend_controller_start_normal(
      controller_, Bytes(scene), scene.size(), scene_id.has_value() ? 1U : 0U,
      &outcome);
  return RunPreparedCall(client, controller_, step, outcome);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text) {
  return StartCommandWithScene(client, selected_text, std::nullopt);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return StartCommandWithScene(client, selected_text, scene_id);
}

BridgeOutcome
FrontendBridge::StartCommandWithScene(DaemonClient *client,
                                      std::string_view selected_text,
                                      std::optional<std::string_view> scene_id) {
  VinputFcitxFrontendOutcome *outcome = nullptr;
  const auto scene = scene_id.value_or(std::string_view{});
  const auto step = vinput_fcitx_frontend_controller_start_command(
      controller_, Bytes(selected_text), selected_text.size(), Bytes(scene),
      scene.size(), scene_id.has_value() ? 1U : 0U, &outcome);
  return RunPreparedCall(client, controller_, step, outcome);
}

BridgeOutcome FrontendBridge::Stop(DaemonClient *client, std::string_view scene_id) {
  VinputFcitxFrontendOutcome *outcome = nullptr;
  const auto step = vinput_fcitx_frontend_controller_stop(controller_, Bytes(scene_id),
                                                          scene_id.size(), &outcome);
  return RunPreparedCall(client, controller_, step, outcome);
}

void FrontendBridge::AdoptRecording(bool command_mode, std::string_view scene_id) {
  if (vinput_fcitx_frontend_controller_adopt(controller_, command_mode ? 1U : 0U,
                                             Bytes(scene_id), scene_id.size()) == 0) {
    Reset();
  }
}

void FrontendBridge::Reset() {
  static_cast<void>(vinput_fcitx_frontend_controller_reset(controller_));
}

} // namespace vinput_fcitx_bridge
