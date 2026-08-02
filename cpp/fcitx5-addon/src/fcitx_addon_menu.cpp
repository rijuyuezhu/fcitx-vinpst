#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include "vinput_fcitx_bridge/fcitx_menu_paging.h"

#include "vinput_fcitx_bridge/fcitx_selection.h"

#include <dbus_public.h>

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
  const auto normalized = key.normalize();
  return normalized.sym() == symbol && normalized.states() == fcitx::KeyStates();
}

int CurrentMenuSelectionIndex(fcitx::CandidateList *candidate_list) {
  if (candidate_list == nullptr || candidate_list->cursorIndex() < 0) {
    return -1;
  }
  auto *pageable = candidate_list->toPageable();
  const int page = pageable != nullptr ? pageable->currentPage() : 0;
  return page * kMenuPageSize + candidate_list->cursorIndex();
}

std::string DecoratePagedMenuTitle(std::string title,
                                   fcitx::CandidateList *candidate_list) {
  auto *pageable = candidate_list != nullptr ? candidate_list->toPageable() : nullptr;
  if (pageable == nullptr || pageable->totalPages() <= 1 ||
      pageable->currentPage() < 0) {
    return title;
  }
  title += FrontendPageText(pageable->currentPage() + 1, pageable->totalPages());
  return title;
}

void SetFilteredMenuTitle(fcitx::InputContext *ic, std::string_view base_title,
                          const MenuFilterState &filter,
                          fcitx::CandidateList *candidate_list) {
  if (ic == nullptr) {
    return;
  }
  ic->inputPanel().setAuxUp(fcitx::Text(
      DecoratePagedMenuTitle(filter.DecorateTitle(base_title), candidate_list)));
}

bool IsMenuEnterKey(const fcitx::Key &key) {
  return IsKey(key, FcitxKey_Return) || IsKey(key, FcitxKey_KP_Enter);
}

bool IsHandledMenuKey(const fcitx::Key &key, bool trigger_key,
                      const MenuFilterState &filter, const FrontendSettings &settings) {
  return trigger_key || key.checkKeyList(settings.page_prev_keys) ||
         key.checkKeyList(settings.page_next_keys) || key.digitSelection() >= 0 ||
         IsMenuSlashKey(key) || IsMenuBackspaceKey(key) ||
         IsMenuCtrlShortcut(key, FcitxKey_w) || IsMenuCtrlShortcut(key, FcitxKey_u) ||
         IsKey(key, FcitxKey_Up) || IsKey(key, FcitxKey_Down) || IsMenuEnterKey(key) ||
         IsKey(key, FcitxKey_Escape) || IsMenuPureModifierKey(key) ||
         IsPrintableMenuInput(key, filter.active(), settings.page_prev_keys,
                              settings.page_next_keys);
}

bool IsEffectiveAsrTarget(const AsrDisplayMenuItem &target,
                          const AsrDisplayMenuStateSnapshot &state) {
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

std::string TranslatedProviderKind(std::string_view kind) {
  if (kind == "local") {
    return FrontendText("Local");
  }
  if (kind == "remote") {
    return FrontendText("Remote");
  }
  if (kind == "command") {
    return FrontendText("Command");
  }
  return std::string(kind);
}

std::string AsrTargetLabel(const AsrDisplayMenuItem &target,
                           const AsrDisplayMenuStateSnapshot &state) {
  std::string label =
      target.display_title.empty()
          ? (target.item_id.empty() ? target.provider_id : target.item_id)
          : target.display_title;
  label += " [" + TranslatedProviderKind(target.kind) + "]";
  if (state.reload_in_progress && target.provider_id == state.target_provider_id &&
      target.model_value == state.target_model_id) {
    label += FrontendText(" (loading)");
  }
  return label;
}

std::string AsrDisplayTitleFor(std::string_view provider_id,
                               std::string_view model_value,
                               const AsrDisplayMenuStateSnapshot &state) {
  for (const auto &target : state.targets) {
    if (target.provider_id == provider_id && target.model_value == model_value) {
      return target.display_title.empty() ? target.item_id : target.display_title;
    }
  }
  return std::string(model_value);
}

std::string EffectiveAsrLabel(const AsrDisplayMenuStateSnapshot &state) {
  std::string label = state.effective_model_id.empty()
                          ? state.effective_provider_id
                          : AsrDisplayTitleFor(state.effective_provider_id,
                                               state.effective_model_id, state);
  if (label.empty()) {
    label = FrontendText("unavailable");
  }
  if (state.reload_in_progress && !state.target_provider_id.empty()) {
    label += " | ";
    label += FrontendText("Loading: ");
    label += state.target_provider_id;
    if (!state.target_model_id.empty()) {
      label += "/";
      label += AsrDisplayTitleFor(state.target_provider_id, state.target_model_id, state);
    }
  }
  if (!state.last_error.empty()) {
    label += " | ";
    label += FrontendText("Error: ");
    label += state.last_error;
  }
  return label;
}

} // namespace

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

  scene_menu_ic_ = ic;
  scene_menu_visible_ = true;
  scene_menu_filter_.Reset();
  RebuildSceneMenu();
}

void FcitxVinputAddon::RebuildSceneMenu(int page) {
  if (!scene_menu_visible_ || scene_menu_ic_ == nullptr) {
    return;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  scene_menu_indices_.clear();
  std::string active_label = active_scene_id_;
  for (std::size_t index = 0; index < scene_state_.scenes.size(); ++index) {
    const auto &scene = scene_state_.scenes[index];
    if (scene.id == active_scene_id_) {
      active_label = scene.label;
      continue;
    }
    if (!scene_menu_filter_.Matches(scene.label + " " + scene.id)) {
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
    SetMenuCandidatePage(*candidates, page);
    scene_menu_page_ = candidates->currentPage();
  } else {
    scene_menu_page_ = 0;
  }

  SetFilteredMenuTitle(scene_menu_ic_, FrontendText("Scenes /filter"),
                       scene_menu_filter_, candidates.get());
  scene_menu_ic_->inputPanel().setAuxDown(
      fcitx::Text(FrontendText("Current: ") + active_label));
  PublishMenuCandidateList(scene_menu_ic_, std::move(candidates));
  scene_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
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
  scene_menu_filter_.Reset();
  scene_menu_indices_.clear();
  scene_menu_page_ = 0;
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
  const bool trigger_key = trigger_policy_.IsSceneMenuTrigger(event);
  const bool printable_filter_input = IsPrintableMenuInput(
      key, scene_menu_filter_.active(), frontend_settings_.page_prev_keys,
      frontend_settings_.page_next_keys);
  const bool handled =
      IsHandledMenuKey(key, trigger_key, scene_menu_filter_, frontend_settings_);

  if (event.isRelease()) {
    if (!handled) {
      return false;
    }
    event.filterAndAccept();
    return true;
  }
  if (!handled) {
    HideSceneMenu();
    return false;
  }
  if (trigger_key || IsMenuPureModifierKey(key)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    if (scene_menu_filter_.active() || !scene_menu_filter_.query().empty()) {
      scene_menu_filter_.ClearAndDeactivate();
      RebuildSceneMenu();
    } else {
      HideSceneMenu();
    }
    event.filterAndAccept();
    return true;
  }
  if (IsMenuSlashKey(key)) {
    scene_menu_filter_.Activate();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (IsMenuBackspaceKey(key) && scene_menu_filter_.active()) {
    scene_menu_filter_.Backspace();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (scene_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_w)) {
    scene_menu_filter_.DeleteLastWord();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (scene_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_u)) {
    scene_menu_filter_.ClearAndDeactivate();
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }
  if (printable_filter_input) {
    scene_menu_filter_.AppendText(MenuKeyToUtf8(key));
    RebuildSceneMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = scene_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  if (key.checkKeyList(frontend_settings_.page_prev_keys)) {
    RebuildSceneMenu(scene_menu_page_ - 1);
    event.filterAndAccept();
    return true;
  } else if (key.checkKeyList(frontend_settings_.page_next_keys)) {
    RebuildSceneMenu(scene_menu_page_ + 1);
    event.filterAndAccept();
    return true;
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int index = scene_menu_page_ * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
        SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)],
                    scene_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
      cursor->prevCandidate();
    } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
      cursor->nextCandidate();
    } else if (IsMenuEnterKey(key)) {
      int index = CurrentMenuSelectionIndex(candidate_list.get());
      if (index < 0 && !scene_menu_indices_.empty()) {
        index = scene_menu_page_ * kMenuPageSize;
      }
      if (index >= 0 && index < static_cast<int>(scene_menu_indices_.size())) {
        SelectScene(scene_menu_indices_[static_cast<std::size_t>(index)],
                    scene_menu_ic_);
      } else {
        HideSceneMenu();
      }
      event.filterAndAccept();
      return true;
    } else {
      HideSceneMenu();
      return false;
    }
  }

  SetFilteredMenuTitle(scene_menu_ic_, FrontendText("Scenes /filter"),
                       scene_menu_filter_, candidate_list.get());
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
  const auto message = FrontendValueText("Switched scene to '%s'.", scene.label);
  Notify(FrontendNotificationKind::Info, message);
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

  asr_menu_ic_ = ic;
  asr_menu_visible_ = true;
  asr_menu_filter_.Reset();
  RebuildAsrMenu();
}

void FcitxVinputAddon::RebuildAsrMenu(int page) {
  if (!asr_menu_visible_ || asr_menu_ic_ == nullptr) {
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
    const auto label = AsrTargetLabel(target, asr_menu_state_);
    const auto search_text = label + " " + target.provider_id + " " + target.kind +
                             " " + target.item_id + " " + target.display_title + " " +
                             target.model_value;
    if (!asr_menu_filter_.Matches(search_text)) {
      continue;
    }
    asr_menu_indices_.push_back(index);
    candidates->append<MenuCandidateWord>(
        label, [this, index](fcitx::InputContext *input_context) {
          SelectAsrTarget(index, input_context);
        });
  }
  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
    SetMenuCandidatePage(*candidates, page);
    asr_menu_page_ = candidates->currentPage();
  } else {
    asr_menu_page_ = 0;
  }

  SetFilteredMenuTitle(asr_menu_ic_, FrontendText("Models /filter"), asr_menu_filter_,
                       candidates.get());
  asr_menu_ic_->inputPanel().setAuxDown(
      fcitx::Text(FrontendText("Current: ") + EffectiveAsrLabel(asr_menu_state_)));
  PublishMenuCandidateList(asr_menu_ic_, std::move(candidates));
  asr_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshAsrMenuState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  return client != nullptr && client->GetAsrDisplayMenuState(&asr_menu_state_, error);
}

void FcitxVinputAddon::HideAsrMenu() {
  auto *ic = asr_menu_ic_;
  asr_menu_visible_ = false;
  asr_menu_ic_ = nullptr;
  asr_menu_filter_.Reset();
  asr_menu_indices_.clear();
  asr_menu_page_ = 0;
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
  const bool trigger_key = trigger_policy_.IsAsrMenuTrigger(event);
  const bool printable_filter_input = IsPrintableMenuInput(
      key, asr_menu_filter_.active(), frontend_settings_.page_prev_keys,
      frontend_settings_.page_next_keys);
  const bool handled =
      IsHandledMenuKey(key, trigger_key, asr_menu_filter_, frontend_settings_);

  if (event.isRelease()) {
    if (!handled) {
      return false;
    }
    event.filterAndAccept();
    return true;
  }
  if (!handled) {
    HideAsrMenu();
    return false;
  }
  if (trigger_key || IsMenuPureModifierKey(key)) {
    event.filterAndAccept();
    return true;
  }
  if (IsKey(key, FcitxKey_Escape)) {
    if (asr_menu_filter_.active() || !asr_menu_filter_.query().empty()) {
      asr_menu_filter_.ClearAndDeactivate();
      RebuildAsrMenu();
    } else {
      HideAsrMenu();
    }
    event.filterAndAccept();
    return true;
  }
  if (IsMenuSlashKey(key)) {
    asr_menu_filter_.Activate();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (IsMenuBackspaceKey(key) && asr_menu_filter_.active()) {
    asr_menu_filter_.Backspace();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (asr_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_w)) {
    asr_menu_filter_.DeleteLastWord();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (asr_menu_filter_.active() && IsMenuCtrlShortcut(key, FcitxKey_u)) {
    asr_menu_filter_.ClearAndDeactivate();
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }
  if (printable_filter_input) {
    asr_menu_filter_.AppendText(MenuKeyToUtf8(key));
    RebuildAsrMenu();
    event.filterAndAccept();
    return true;
  }

  auto candidate_list = asr_menu_ic_->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  if (key.checkKeyList(frontend_settings_.page_prev_keys)) {
    RebuildAsrMenu(asr_menu_page_ - 1);
    event.filterAndAccept();
    return true;
  } else if (key.checkKeyList(frontend_settings_.page_next_keys)) {
    RebuildAsrMenu(asr_menu_page_ + 1);
    event.filterAndAccept();
    return true;
  } else {
    const int digit = key.digitSelection();
    if (digit >= 0) {
      const int index = asr_menu_page_ * kMenuPageSize + digit;
      if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
        SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)],
                        asr_menu_ic_);
      }
      event.filterAndAccept();
      return true;
    }
    if (cursor != nullptr && IsKey(key, FcitxKey_Up)) {
      cursor->prevCandidate();
    } else if (cursor != nullptr && IsKey(key, FcitxKey_Down)) {
      cursor->nextCandidate();
    } else if (IsMenuEnterKey(key)) {
      int index = CurrentMenuSelectionIndex(candidate_list.get());
      if (index < 0 && !asr_menu_indices_.empty()) {
        index = asr_menu_page_ * kMenuPageSize;
      }
      if (index >= 0 && index < static_cast<int>(asr_menu_indices_.size())) {
        SelectAsrTarget(asr_menu_indices_[static_cast<std::size_t>(index)],
                        asr_menu_ic_);
      } else {
        HideAsrMenu();
      }
      event.filterAndAccept();
      return true;
    } else {
      HideAsrMenu();
      return false;
    }
  }

  SetFilteredMenuTitle(asr_menu_ic_, FrontendText("Models /filter"), asr_menu_filter_,
                       candidate_list.get());
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
  const auto display_title =
      target.display_title.empty() ? target.item_id : target.display_title;
  const auto message =
      FrontendValueText("ASR switch requested for '%s'.", display_title);
  Notify(FrontendNotificationKind::Info, message);
  FCITX_INFO() << "fcitx-vinput requested ASR target switch to " << target.provider_id
               << '/' << target.item_id << " persisted=" << persisted;
}

} // namespace vinput_fcitx_bridge
