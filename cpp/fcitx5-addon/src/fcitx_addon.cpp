#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/fcitx_selection.h"

#ifdef VINPUT_FCITX_HAVE_CLIPBOARD
#include "clipboard_public.h"
#include <fcitx-utils/utf8.h>
#endif

#include <fcitx-utils/log.h>
#include <fcitx/addonmanager.h>
#include <fcitx/candidatelist.h>
#include <fcitx/event.h>
#include <fcitx/inputcontext.h>
#include <fcitx/inputpanel.h>
#include <fcitx/surroundingtext.h>
#include <fcitx/text.h>
#include <fcitx/userinterface.h>

#include <chrono>
#include <functional>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

constexpr int kMenuPageSize = 10;

class MenuCandidateWord final : public fcitx::CandidateWord {
public:
  MenuCandidateWord(std::string label,
                    std::function<void(fcitx::InputContext *)> on_select)
      : fcitx::CandidateWord(fcitx::Text(std::move(label))),
        on_select_(std::move(on_select)) {}

  void select(fcitx::InputContext *input_context) const override {
    if (on_select_) {
      on_select_(input_context);
    }
  }

private:
  std::function<void(fcitx::InputContext *)> on_select_;
};

bool IsKey(const fcitx::Key &key, FcitxKeySym symbol) {
  return key.normalize().sym() == symbol;
}

int CurrentMenuSelectionIndex(fcitx::CandidateList *candidate_list) {
  if (candidate_list == nullptr || candidate_list->cursorIndex() < 0) {
    return -1;
  }
  auto *pageable = candidate_list->toPageable();
  const int page = pageable != nullptr ? pageable->currentPage() : 0;
  return page * kMenuPageSize + candidate_list->cursorIndex();
}

bool IsEffectiveAsrTarget(const AsrTargetMenuItem &target,
                          const AsrTargetMenuStateSnapshot &state) {
  if (target.provider_id != state.effective_provider_id) {
    return false;
  }
  if (target.model_value == state.effective_model_id) {
    return true;
  }
  return !state.reload_in_progress && state.last_error.empty() &&
         target.provider_id == state.target_provider_id &&
         target.model_value == state.target_model_id;
}

std::string AsrTargetLabel(const AsrTargetMenuItem &target,
                           const AsrTargetMenuStateSnapshot &state) {
  std::string label = target.item_id.empty() ? target.provider_id : target.item_id;
  label += " [" + target.kind + "]";
  if (state.reload_in_progress && target.provider_id == state.target_provider_id &&
      target.model_value == state.target_model_id) {
    label += " (loading)";
  }
  return label;
}

std::string EffectiveAsrLabel(const AsrTargetMenuStateSnapshot &state) {
  std::string label = state.effective_model_id.empty() ? state.effective_provider_id
                                                       : state.effective_model_id;
  if (label.empty()) {
    label = "unavailable";
  }
  if (state.reload_in_progress && !state.target_provider_id.empty()) {
    label += " | Loading: " + state.target_provider_id;
    if (!state.target_model_id.empty()) {
      label += "/" + state.target_model_id;
    }
  }
  if (!state.last_error.empty()) {
    label += " | Error: " + state.last_error;
  }
  return label;
}

std::string TriggerListDescription(const fcitx::KeyList &keys) {
  std::string description;
  for (const auto &key : keys) {
    if (!description.empty()) {
      description += ", ";
    }
    description += key.toString();
  }
  return description.empty() ? "<disabled>" : description;
}

#ifdef VINPUT_FCITX_HAVE_CLIPBOARD
std::string PrimarySelectionFromClipboard(fcitx::Instance *instance,
                                          fcitx::InputContext *ic) {
  if (instance == nullptr || ic == nullptr) {
    return {};
  }
  auto *clipboard = instance->addonManager().addon("clipboard");
  if (clipboard == nullptr) {
    return {};
  }
  auto primary = clipboard->call<fcitx::IClipboard::primary>(ic);
  if (!fcitx::utf8::validate(primary)) {
    return {};
  }
  return primary;
}
#else
std::string PrimarySelectionFromClipboard(fcitx::Instance *, fcitx::InputContext *) {
  return {};
}
#endif

std::string SelectedTextFromInputContext(fcitx::Instance *instance,
                                         fcitx::InputContext *ic) {
  if (ic == nullptr) {
    return {};
  }
  return SelectedTextWithPrimaryFallback(ic->surroundingText(),
                                         PrimarySelectionFromClipboard(instance, ic));
}
std::string_view TriggerActionName(FcitxTriggerAction action) {
  switch (action) {
  case FcitxTriggerAction::None:
    return "none";
  case FcitxTriggerAction::StartNormal:
    return "start-normal";
  case FcitxTriggerAction::StopNormal:
    return "stop-normal";
  case FcitxTriggerAction::StartCommand:
    return "start-command";
  case FcitxTriggerAction::StopCommand:
    return "stop-command";
  case FcitxTriggerAction::ShowSceneMenu:
    return "show-scene-menu";
  case FcitxTriggerAction::ConsumeSceneMenuRelease:
    return "consume-scene-menu-release";
  case FcitxTriggerAction::ShowAsrMenu:
    return "show-asr-menu";
  case FcitxTriggerAction::ConsumeAsrMenuRelease:
    return "consume-asr-menu-release";
  }
  return "unknown";
}

void RequestSurroundingText(fcitx::Event &event) {
  auto &ic_event = static_cast<fcitx::InputContextEvent &>(event);
  auto *ic = ic_event.inputContext();
  if (ic == nullptr) {
    return;
  }
  ic->setCapabilityFlags(ic->capabilityFlags() |
                         fcitx::CapabilityFlag::SurroundingText);
}

} // namespace

FcitxVinputAddon::FcitxVinputAddon(fcitx::Instance *instance)
    : instance_(instance), frontend_settings_(LoadFrontendSettings()),
      trigger_policy_(FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
          frontend_settings_.normal_triggers, frontend_settings_.command_triggers,
          frontend_settings_.scene_menu_triggers,
          frontend_settings_.asr_menu_triggers)),
      trigger_mode_controller_(frontend_settings_.trigger_mode) {
  FCITX_INFO() << "fcitx-vinput addon loaded with normal triggers "
               << TriggerListDescription(trigger_policy_.normal_triggers())
               << ", command triggers "
               << TriggerListDescription(trigger_policy_.command_triggers())
               << ", scene menu triggers "
               << TriggerListDescription(trigger_policy_.scene_menu_triggers())
               << ", and ASR menu triggers "
               << TriggerListDescription(trigger_policy_.asr_menu_triggers())
               << ", trigger mode "
               << TriggerModeToString(frontend_settings_.trigger_mode);
  if (instance_ != nullptr) {
    event_handlers_.emplace_back(
        instance_->watchEvent(fcitx::EventType::InputContextKeyEvent,
                              fcitx::EventWatcherPhase::PostInputMethod,
                              [this](fcitx::Event &event) { HandleKeyEvent(event); }));
    event_handlers_.emplace_back(instance_->watchEvent(
        fcitx::EventType::InputContextCreated, fcitx::EventWatcherPhase::PreInputMethod,
        RequestSurroundingText));
  }
}

void FcitxVinputAddon::reloadConfig() {
  frontend_settings_ = LoadFrontendSettings();
  ApplyFrontendSettings();
}

void FcitxVinputAddon::save() {
  if (!SaveFrontendSettings(frontend_settings_)) {
    FCITX_ERROR() << "fcitx-vinput failed to save frontend configuration";
  }
}

const fcitx::Configuration *FcitxVinputAddon::getConfig() const {
  frontend_config_ = BuildFrontendConfig(frontend_settings_);
  return frontend_config_.get();
}

void FcitxVinputAddon::setConfig(const fcitx::RawConfig &config) {
  auto frontend_config = BuildFrontendConfig(frontend_settings_);
  frontend_config->load(config, true);
  frontend_settings_ = frontend_config->settings();
  ApplyFrontendSettings();
  save();
}

void FcitxVinputAddon::ApplyFrontendSettings() {
  CancelTriggerStart();
  trigger_policy_ = FcitxKeyTriggerPolicy::WithEnvironmentOverrides(
      frontend_settings_.normal_triggers, frontend_settings_.command_triggers,
      frontend_settings_.scene_menu_triggers, frontend_settings_.asr_menu_triggers);
  trigger_mode_controller_.SetMode(frontend_settings_.trigger_mode);
  if (!bridge_.recording()) {
    CancelTriggerStop();
    trigger_mode_controller_.RecordingStopped();
    active_trigger_ic_.unwatch();
  }
}

SdBusDaemonClient *FcitxVinputAddon::EnsureDaemonClient(std::string *error) {
  if (daemon_client_ == nullptr) {
    daemon_client_ = SdBusDaemonClient::ConnectSession(error);
  }
  return daemon_client_.get();
}

AppliedOutcome FcitxVinputAddon::ApplyDaemonUnavailable(fcitx::InputContext *ic,
                                                        std::string error) {
  BridgeOutcome outcome;
  outcome.kind = BridgeOutcome::Kind::Error;
  outcome.text =
      error.empty() ? "Voice input daemon is unavailable." : std::move(error);
  FCITX_WARN() << "fcitx-vinput daemon unavailable: " << outcome.text;
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyBridgeOutcome(fcitx::InputContext *ic,
                                                    const BridgeOutcome &outcome) {
  if (outcome.kind == BridgeOutcome::Kind::Error) {
    daemon_client_.reset();
  }
  return ApplyBridgeOutcomeToInputContext(outcome, ic);
}

AppliedOutcome FcitxVinputAddon::TriggerNormal(fcitx::InputContext *ic,
                                               std::string_view scene_id) {
  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording() ? bridge_.Stop(client, scene_id)
                                     : bridge_.StartNormal(client, scene_id);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::TriggerCommand(fcitx::InputContext *ic,
                                                std::string_view selected_text,
                                                std::string_view scene_id) {
  if (!bridge_.recording() && selected_text.empty()) {
    return ApplyBridgeOutcome(ic,
                              bridge_.StartCommand(nullptr, selected_text, scene_id));
  }

  std::string error;
  auto *client = EnsureDaemonClient(&error);
  if (client == nullptr) {
    return ApplyDaemonUnavailable(ic, std::move(error));
  }

  auto outcome = bridge_.recording()
                     ? bridge_.Stop(client, scene_id)
                     : bridge_.StartCommand(client, selected_text, scene_id);
  return ApplyBridgeOutcome(ic, outcome);
}

AppliedOutcome FcitxVinputAddon::ApplyTriggerAction(fcitx::InputContext *ic,
                                                    FcitxTriggerAction action,
                                                    std::string_view selected_text) {
  switch (action) {
  case FcitxTriggerAction::None:
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartNormal:
    if (!bridge_.recording()) {
      std::string error;
      RefreshSceneState(&error);
      return TriggerNormal(ic, active_scene_id_);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopNormal:
    if (bridge_.recording() && !bridge_.command_mode()) {
      return TriggerNormal(ic, active_scene_id_);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StartCommand:
    if (!bridge_.recording()) {
      return TriggerCommand(ic, selected_text);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::StopCommand:
    if (bridge_.recording() && bridge_.command_mode()) {
      return TriggerCommand(ic, "");
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ShowSceneMenu:
    if (!bridge_.recording()) {
      ShowSceneMenu(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ConsumeSceneMenuRelease:
    return AppliedOutcome::None;
  case FcitxTriggerAction::ShowAsrMenu:
    if (!bridge_.recording()) {
      ShowAsrMenu(ic);
    }
    return AppliedOutcome::None;
  case FcitxTriggerAction::ConsumeAsrMenuRelease:
    return AppliedOutcome::None;
  }
  return AppliedOutcome::None;
}

void FcitxVinputAddon::HandleKeyEvent(fcitx::Event &event) {
  if (event.type() != fcitx::EventType::InputContextKeyEvent) {
    return;
  }

  auto &key_event = static_cast<fcitx::KeyEvent &>(event);
  if (asr_menu_visible_ && HandleAsrMenuKeyEvent(key_event)) {
    return;
  }
  if (scene_menu_visible_ && HandleSceneMenuKeyEvent(key_event)) {
    return;
  }
  const auto action = trigger_policy_.Classify(key_event);
  if (action == FcitxTriggerAction::None) {
    return;
  }

  FCITX_INFO() << "fcitx-vinput handling trigger " << TriggerActionName(action);
  if (action == FcitxTriggerAction::ShowSceneMenu ||
      action == FcitxTriggerAction::ConsumeSceneMenuRelease ||
      action == FcitxTriggerAction::ShowAsrMenu ||
      action == FcitxTriggerAction::ConsumeAsrMenuRelease) {
    ApplyTriggerAction(key_event.inputContext(), action);
    key_event.filterAndAccept();
    return;
  }

  const bool normal = action == FcitxTriggerAction::StartNormal ||
                      action == FcitxTriggerAction::StopNormal;
  const auto kind = normal ? TriggerKind::Normal : TriggerKind::Command;
  TriggerModeAction mode_action = TriggerModeAction::None;
  if (key_event.isRelease()) {
    mode_action = trigger_mode_controller_.OnRelease(
        kind, key_event.key(), TriggerModeController::Clock::now());
  } else {
    mode_action = trigger_mode_controller_.OnPress(kind, key_event.key(),
                                                   TriggerModeController::Clock::now(),
                                                   bridge_.recording());
    if (mode_action != TriggerModeAction::None &&
        mode_action != TriggerModeAction::Consume) {
      CancelTriggerStop();
    }
  }
  HandleTriggerModeAction(key_event.inputContext(), mode_action);
  key_event.filterAndAccept();
}

void FcitxVinputAddon::HandleTriggerModeAction(fcitx::InputContext *ic,
                                               TriggerModeAction action) {
  switch (action) {
  case TriggerModeAction::None:
  case TriggerModeAction::Consume:
    return;
  case TriggerModeAction::StartNormal:
    ApplyTriggerAction(ic, FcitxTriggerAction::StartNormal);
    break;
  case TriggerModeAction::StartCommand:
    ApplyTriggerAction(ic, FcitxTriggerAction::StartCommand,
                       SelectedTextFromInputContext(instance_, ic));
    break;
  case TriggerModeAction::StopActive:
    StopActiveRecording(ic);
    return;
  case TriggerModeAction::ScheduleNormalStart:
  case TriggerModeAction::ScheduleCommandStart:
    ScheduleTriggerStart(ic);
    return;
  case TriggerModeAction::CancelPendingStart:
    CancelTriggerStart();
    return;
  case TriggerModeAction::ScheduleStop:
    ScheduleTriggerStop(ic);
    return;
  }

  const bool started = bridge_.recording();
  trigger_mode_controller_.ConfirmStart(started);
  if (started && ic != nullptr) {
    active_trigger_ic_ = ic->watch();
  }
}

void FcitxVinputAddon::ScheduleTriggerStart(fcitx::InputContext *ic) {
  CancelTriggerStart();
  if (instance_ == nullptr || ic == nullptr) {
    trigger_mode_controller_.RecordingStopped();
    return;
  }
  pending_trigger_ic_ = ic->watch();
  const auto fire_at =
      fcitx::now(CLOCK_MONOTONIC) +
      static_cast<std::uint64_t>(
          std::chrono::duration_cast<std::chrono::microseconds>(kTriggerHoldThreshold)
              .count());
  pending_trigger_start_event_ = instance_->eventLoop().addTimeEvent(
      CLOCK_MONOTONIC, fire_at, 0, [this](fcitx::EventSourceTime *, std::uint64_t) {
        auto *input_context = pending_trigger_ic_.get();
        pending_trigger_start_event_.reset();
        pending_trigger_ic_.unwatch();
        const auto action = trigger_mode_controller_.FirePendingStart();
        if (input_context == nullptr) {
          trigger_mode_controller_.ConfirmStart(false);
          return false;
        }
        HandleTriggerModeAction(input_context, action);
        return false;
      });
  pending_trigger_start_event_->setOneShot();
}

void FcitxVinputAddon::CancelTriggerStart() {
  if (pending_trigger_start_event_ != nullptr) {
    pending_trigger_start_event_->setEnabled(false);
    pending_trigger_start_event_.reset();
  }
  pending_trigger_ic_.unwatch();
}

void FcitxVinputAddon::ScheduleTriggerStop(fcitx::InputContext *fallback_ic) {
  CancelTriggerStop();
  if (!active_trigger_ic_.isValid() && fallback_ic != nullptr) {
    active_trigger_ic_ = fallback_ic->watch();
  }
  if (instance_ == nullptr) {
    StopActiveRecording(fallback_ic);
    return;
  }
  const auto fire_at =
      fcitx::now(CLOCK_MONOTONIC) +
      static_cast<std::uint64_t>(
          std::chrono::duration_cast<std::chrono::microseconds>(kTriggerReleaseTail)
              .count());
  pending_trigger_stop_event_ = instance_->eventLoop().addTimeEvent(
      CLOCK_MONOTONIC, fire_at, 0, [this](fcitx::EventSourceTime *, std::uint64_t) {
        pending_trigger_stop_event_.reset();
        if (trigger_mode_controller_.FirePendingStop() ==
            TriggerModeAction::StopActive) {
          StopActiveRecording(active_trigger_ic_.get());
        }
        return false;
      });
  pending_trigger_stop_event_->setOneShot();
}

void FcitxVinputAddon::CancelTriggerStop() {
  if (pending_trigger_stop_event_ != nullptr) {
    pending_trigger_stop_event_->setEnabled(false);
    pending_trigger_stop_event_.reset();
  }
}

void FcitxVinputAddon::StopActiveRecording(fcitx::InputContext *fallback_ic) {
  auto *ic = active_trigger_ic_.get();
  if (ic == nullptr) {
    ic = fallback_ic;
  }
  if (bridge_.recording()) {
    if (bridge_.command_mode()) {
      ApplyTriggerAction(ic, FcitxTriggerAction::StopCommand);
    } else {
      ApplyTriggerAction(ic, FcitxTriggerAction::StopNormal);
    }
  }
  if (!bridge_.recording()) {
    CancelTriggerStart();
    CancelTriggerStop();
    trigger_mode_controller_.RecordingStopped();
    active_trigger_ic_.unwatch();
  }
}

void FcitxVinputAddon::ShowSceneMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideAsrMenu();
  std::string error;
  if (!RefreshSceneState(&error)) {
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  scene_menu_indices_.clear();
  for (std::size_t index = 0; index < scene_state_.scenes.size(); ++index) {
    const auto &scene = scene_state_.scenes[index];
    if (scene.id == active_scene_id_) {
      continue;
    }
    scene_menu_indices_.push_back(index);
    candidates->append<MenuCandidateWord>(
        scene.label, [this, index](fcitx::InputContext *input_context) {
          SelectScene(index, input_context);
        });
  }
  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
  }

  scene_menu_ic_ = ic;
  scene_menu_visible_ = true;
  ic->inputPanel().setAuxUp(fcitx::Text("Scenes"));
  ic->inputPanel().setAuxDown(fcitx::Text("Current: " + active_scene_id_));
  ic->inputPanel().setCandidateList(std::move(candidates));
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshSceneState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  if (client == nullptr || !client->GetSceneState(&scene_state_, error)) {
    return false;
  }
  if (!scene_state_.active_scene_id.empty()) {
    active_scene_id_ = scene_state_.active_scene_id;
  }
  return true;
}

void FcitxVinputAddon::HideSceneMenu() {
  auto *ic = scene_menu_ic_;
  scene_menu_visible_ = false;
  scene_menu_ic_ = nullptr;
  scene_menu_indices_.clear();
  if (ic == nullptr) {
    return;
  }
  fcitx::Text empty;
  ic->inputPanel().setAuxUp(empty);
  ic->inputPanel().setAuxDown(empty);
  ic->inputPanel().setCandidateList({});
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::HandleSceneMenuKeyEvent(fcitx::KeyEvent &event) {
  if (!scene_menu_visible_ || scene_menu_ic_ == nullptr) {
    return false;
  }
  const auto &key = event.key();
  if (event.isRelease()) {
    if (trigger_policy_.IsSceneMenuTrigger(event)) {
      event.filterAndAccept();
      return true;
    }
    return false;
  }
  if (trigger_policy_.IsSceneMenuTrigger(event)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    HideSceneMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = scene_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
    cursor->prevCandidate();
  } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
    cursor->nextCandidate();
  } else if (pageable != nullptr &&
             key.checkKeyList(frontend_settings_.page_prev_keys) &&
             pageable->hasPrev()) {
    pageable->prev();
  } else if (pageable != nullptr &&
             key.checkKeyList(frontend_settings_.page_next_keys) &&
             pageable->hasNext()) {
    pageable->next();
  } else if (IsKey(key, FcitxKey_Return) || IsKey(key, FcitxKey_KP_Enter)) {
    const int index = CurrentMenuSelectionIndex(candidate_list.get());
    if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
      SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)], scene_menu_ic_);
    } else {
      HideSceneMenu();
    }
    event.filterAndAccept();
    return true;
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int page = pageable != nullptr ? pageable->currentPage() : 0;
      const int index = page * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
        SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)],
                    scene_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    HideSceneMenu();
    return false;
  }
  scene_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
}

void FcitxVinputAddon::SelectScene(std::size_t index, fcitx::InputContext *ic) {
  if (index >= scene_state_.scenes.size()) {
    HideSceneMenu();
    return;
  }
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);
  const auto &scene = scene_state_.scenes[index];
  if (client == nullptr || !client->SetActiveScene(scene.id, &persisted, &error)) {
    HideSceneMenu();
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }
  active_scene_id_ = scene.id;
  scene_state_.active_scene_id = scene.id;
  HideSceneMenu();
  FCITX_INFO() << "fcitx-vinput switched active scene to " << scene.id
               << " persisted=" << persisted;
}

void FcitxVinputAddon::ShowAsrMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideSceneMenu();
  std::string error;
  if (!RefreshAsrMenuState(&error)) {
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  asr_menu_indices_.clear();
  for (std::size_t index = 0; index < asr_menu_state_.targets.size(); ++index) {
    const auto &target = asr_menu_state_.targets[index];
    if (IsEffectiveAsrTarget(target, asr_menu_state_)) {
      continue;
    }
    asr_menu_indices_.push_back(index);
    candidates->append<MenuCandidateWord>(
        AsrTargetLabel(target, asr_menu_state_),
        [this, index](fcitx::InputContext *input_context) {
          SelectAsrTarget(index, input_context);
        });
  }
  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
  }

  asr_menu_ic_ = ic;
  asr_menu_visible_ = true;
  ic->inputPanel().setAuxUp(fcitx::Text("ASR Models"));
  ic->inputPanel().setAuxDown(
      fcitx::Text("Current: " + EffectiveAsrLabel(asr_menu_state_)));
  ic->inputPanel().setCandidateList(std::move(candidates));
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshAsrMenuState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  return client != nullptr && client->GetAsrTargetMenuState(&asr_menu_state_, error);
}

void FcitxVinputAddon::HideAsrMenu() {
  auto *ic = asr_menu_ic_;
  asr_menu_visible_ = false;
  asr_menu_ic_ = nullptr;
  asr_menu_indices_.clear();
  if (ic == nullptr) {
    return;
  }
  fcitx::Text empty;
  ic->inputPanel().setAuxUp(empty);
  ic->inputPanel().setAuxDown(empty);
  ic->inputPanel().setCandidateList({});
  ic->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::HandleAsrMenuKeyEvent(fcitx::KeyEvent &event) {
  if (!asr_menu_visible_ || asr_menu_ic_ == nullptr) {
    return false;
  }
  const auto &key = event.key();
  if (event.isRelease()) {
    if (trigger_policy_.IsAsrMenuTrigger(event)) {
      event.filterAndAccept();
      return true;
    }
    return false;
  }
  if (trigger_policy_.IsAsrMenuTrigger(event)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    HideAsrMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = asr_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
    cursor->prevCandidate();
  } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
    cursor->nextCandidate();
  } else if (pageable != nullptr &&
             key.checkKeyList(frontend_settings_.page_prev_keys) &&
             pageable->hasPrev()) {
    pageable->prev();
  } else if (pageable != nullptr &&
             key.checkKeyList(frontend_settings_.page_next_keys) &&
             pageable->hasNext()) {
    pageable->next();
  } else if (IsKey(key, FcitxKey_Return) || IsKey(key, FcitxKey_KP_Enter)) {
    const int index = CurrentMenuSelectionIndex(candidate_list.get());
    if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
      SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)], asr_menu_ic_);
    } else {
      HideAsrMenu();
    }
    event.filterAndAccept();
    return true;
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int page = pageable != nullptr ? pageable->currentPage() : 0;
      const int index = page * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
        SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)],
                        asr_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    HideAsrMenu();
    return false;
  }
  asr_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
}

void FcitxVinputAddon::SelectAsrTarget(std::size_t index, fcitx::InputContext *ic) {
  if (index >= asr_menu_state_.targets.size()) {
    HideAsrMenu();
    return;
  }
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);
  const auto &target = asr_menu_state_.targets[index];
  if (client == nullptr ||
      !client->SetActiveAsrTarget(target.provider_id, target.model_value, &persisted,
                                  &error)) {
    HideAsrMenu();
    ApplyDaemonUnavailable(ic, std::move(error));
    return;
  }
  HideAsrMenu();
  FCITX_INFO() << "fcitx-vinput requested ASR target switch to " << target.provider_id
               << '/' << target.item_id << " persisted=" << persisted;
}

} // namespace vinput_fcitx_bridge
