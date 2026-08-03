#include "vinput_fcitx_bridge/frontend_bridge.h"

#include "vinput_fcitx_ffi.h"

#include <cstddef>
#include <cstdint>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr std::string_view kRecordingPreedit = "... Recording ...";
constexpr std::string_view kCommandingPreedit = "... Commanding ...";
constexpr std::string_view kNoSelectionError = "Please select text first.";
constexpr std::string_view kDaemonUnavailableError =
    "Voice input daemon is unavailable.";

BridgeOutcome Preedit(std::string_view text) {
  return BridgeOutcome{BridgeOutcome::Kind::Preedit, std::string(text), {}};
}

BridgeOutcome Error(std::string_view text) {
  return BridgeOutcome{BridgeOutcome::Kind::Error, std::string(text), {}};
}

BridgeOutcome Clear(bool command_mode) {
  return BridgeOutcome{BridgeOutcome::Kind::Clear, {}, {}, command_mode};
}

BridgeOutcome Commit(std::string text, RecognitionPayload payload, bool command_mode) {
  return BridgeOutcome{BridgeOutcome::Kind::Commit, std::move(text), std::move(payload),
                       command_mode};
}

BridgeOutcome CandidateMenu(RecognitionPayload payload, bool command_mode) {
  return BridgeOutcome{
      BridgeOutcome::Kind::CandidateMenu, {}, std::move(payload), command_mode};
}

std::string FallbackError(const std::string &error) {
  return error.empty() ? std::string(kDaemonUnavailableError) : error;
}

const std::uint8_t *ByteData(std::string_view text) {
  if (text.empty()) {
    return nullptr;
  }
  return reinterpret_cast<const std::uint8_t *>(text.data());
}

} // namespace

FrontendBridge::FrontendBridge() : state_(vinput_fcitx_frontend_state_new()) {}

FrontendBridge::~FrontendBridge() {
  vinput_fcitx_frontend_state_free(state_);
}

bool FrontendBridge::recording() const {
  return state_ != nullptr && vinput_fcitx_frontend_state_recording(state_) != 0;
}

bool FrontendBridge::command_mode() const {
  return state_ != nullptr && vinput_fcitx_frontend_state_command_mode(state_) != 0;
}

std::optional<std::string> FrontendBridge::ActiveSceneId() const {
  if (state_ == nullptr || vinput_fcitx_frontend_state_has_active_scene(state_) == 0) {
    return std::nullopt;
  }

  const auto size = vinput_fcitx_frontend_state_active_scene_len(state_);
  if (size == 0) {
    return std::string{};
  }

  const auto *data = vinput_fcitx_frontend_state_active_scene_data(state_);
  if (data == nullptr) {
    return std::nullopt;
  }
  return std::string(reinterpret_cast<const char *>(data), size);
}

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client) {
  return StartNormalWithScene(client, std::nullopt);
}

BridgeOutcome FrontendBridge::StartNormal(DaemonClient *client,
                                          std::string_view scene_id) {
  return StartNormalWithScene(client, std::optional<std::string_view>(scene_id));
}

BridgeOutcome
FrontendBridge::StartNormalWithScene(DaemonClient *client,
                                     std::optional<std::string_view> scene_id) {
  if (client == nullptr || state_ == nullptr) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  const auto scene = scene_id.value_or(std::string_view{});
  if (vinput_fcitx_frontend_state_start_normal(state_, ByteData(scene), scene.size(),
                                               scene_id.has_value() ? 1U : 0U) == 0) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  std::string error;
  if (!client->StartRecording(&error)) {
    Reset();
    return Error(FallbackError(error));
  }

  return Preedit(kRecordingPreedit);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text) {
  return StartCommandWithScene(client, selected_text, std::nullopt);
}

BridgeOutcome FrontendBridge::StartCommand(DaemonClient *client,
                                           std::string_view selected_text,
                                           std::string_view scene_id) {
  return StartCommandWithScene(client, selected_text,
                               std::optional<std::string_view>(scene_id));
}

BridgeOutcome
FrontendBridge::StartCommandWithScene(DaemonClient *client,
                                      std::string_view selected_text,
                                      std::optional<std::string_view> scene_id) {
  if (selected_text.empty()) {
    Reset();
    return Error(kNoSelectionError);
  }
  if (client == nullptr || state_ == nullptr) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  const auto scene = scene_id.value_or(std::string_view{});
  if (vinput_fcitx_frontend_state_start_command(state_, ByteData(scene), scene.size(),
                                                scene_id.has_value() ? 1U : 0U) == 0) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  std::string error;
  if (!client->StartCommandRecording(selected_text, &error)) {
    Reset();
    return Error(FallbackError(error));
  }

  return Preedit(kCommandingPreedit);
}

BridgeOutcome FrontendBridge::Stop(DaemonClient *client, std::string_view scene_id) {
  if (!recording()) {
    return BridgeOutcome{};
  }
  if (client == nullptr) {
    Reset();
    return Error(kDaemonUnavailableError);
  }

  const bool was_command_mode = command_mode();
  const std::string stop_scene_id = ActiveSceneId().value_or(std::string(scene_id));

  std::string payload_json;
  std::string error;
  if (!client->StopRecording(stop_scene_id, &payload_json, &error)) {
    Reset();
    return Error(FallbackError(error));
  }

  auto plan = MakeCommitPlan(payload_json, was_command_mode);
  Reset();
  if (plan.payload.commit_text.empty()) {
    return Clear(was_command_mode);
  }
  if (plan.show_candidate_menu) {
    return CandidateMenu(std::move(plan.payload), was_command_mode);
  }
  auto commit_text = plan.payload.commit_text;
  return Commit(std::move(commit_text), std::move(plan.payload), was_command_mode);
}

void FrontendBridge::AdoptRecording(bool command_mode, std::string_view scene_id) {
  if (state_ == nullptr ||
      vinput_fcitx_frontend_state_adopt(state_, command_mode ? 1U : 0U,
                                        ByteData(scene_id), scene_id.size()) == 0) {
    Reset();
  }
}

void FrontendBridge::Reset() {
  if (state_ != nullptr) {
    static_cast<void>(vinput_fcitx_frontend_state_reset(state_));
  }
}

} // namespace vinput_fcitx_bridge
