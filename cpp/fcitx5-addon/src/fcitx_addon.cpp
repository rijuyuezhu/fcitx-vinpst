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
    : instance_(instance), trigger_policy_(FcitxKeyTriggerPolicy::FromEnvironment()) {
  FCITX_INFO() << "fcitx-vinput addon loaded with normal trigger "
               << trigger_policy_.normal_trigger() << " and command trigger "
               << trigger_policy_.command_trigger() << " and scene menu trigger "
               << trigger_policy_.scene_menu_trigger() << " and ASR menu trigger "
               << trigger_policy_.asr_menu_trigger();
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
  ApplyTriggerAction(key_event.inputContext(), action,
                     SelectedTextFromInputContext(instance_, key_event.inputContext()));
  key_event.filterAndAccept();
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
  } else if (pageable != nullptr && IsKey(key, FcitxKey_Page_Up) &&
             pageable->hasPrev()) {
    pageable->prev();
  } else if (pageable != nullptr && IsKey(key, FcitxKey_Page_Down) &&
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
  } else if (pageable != nullptr && IsKey(key, FcitxKey_Page_Up) &&
             pageable->hasPrev()) {
    pageable->prev();
  } else if (pageable != nullptr && IsKey(key, FcitxKey_Page_Down) &&
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
