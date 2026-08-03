#include "vinput_fcitx_bridge/fcitx_addon.h"

#include "vinput_fcitx_bridge/dbus_contract.h"

#include "vinput_fcitx_bridge/fcitx_i18n.h"

#include "vinput_fcitx_bridge/fcitx_menu_paging.h"
#include "vinput_fcitx_bridge/fcitx_menu_projection.h"

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
#include <limits>
#include <utility>

namespace vinput_fcitx_bridge {
namespace {

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

template <typename RebuildMenu, typename HideMenu, typename SelectItem>
bool HandleProjectedMenuKeyEvent(
    fcitx::KeyEvent &event, bool trigger_key, fcitx::InputContext *input_context,
    MenuFilterState &filter, const std::vector<ProjectedMenuControl> &visible_controls,
    int current_page, const FrontendSettings &settings, std::string_view base_title,
    RebuildMenu &&rebuild_menu, HideMenu &&hide_menu, SelectItem &&select_item) {
  auto candidate_list = input_context->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  const auto filter_view = filter.view();
  if (!filter_view.has_value()) {
    hide_menu();
    return false;
  }
  const auto semantic_key =
      ClassifyMenuKey(event.key(), trigger_key, filter_view->active,
                      settings.page_prev_keys, settings.page_next_keys);
  const auto decision =
      filter.HandleKey(event.isRelease(), semantic_key, cursor != nullptr,
                       CurrentMenuSelectionIndex(candidate_list.get()), current_page,
                       visible_controls.size());
  if (!decision.has_value()) {
    hide_menu();
    return false;
  }

  switch (decision->action) {
  case MenuKeyAction::Pass:
    return false;
  case MenuKeyAction::Consume:
    event.filterAndAccept();
    return true;
  case MenuKeyAction::CloseAndPass:
    hide_menu();
    return false;
  case MenuKeyAction::CloseAndConsume:
    hide_menu();
    event.filterAndAccept();
    return true;
  case MenuKeyAction::Rebuild:
    if (decision->value < std::numeric_limits<int>::min() ||
        decision->value > std::numeric_limits<int>::max()) {
      hide_menu();
      return false;
    }
    rebuild_menu(static_cast<int>(decision->value));
    event.filterAndAccept();
    return true;
  case MenuKeyAction::MovePrevious:
    if (cursor == nullptr) {
      hide_menu();
      return false;
    }
    cursor->prevCandidate();
    break;
  case MenuKeyAction::MoveNext:
    if (cursor == nullptr) {
      hide_menu();
      return false;
    }
    cursor->nextCandidate();
    break;
  case MenuKeyAction::Select:
    if (decision->value < 0 ||
        static_cast<std::uint64_t>(decision->value) >= visible_controls.size()) {
      hide_menu();
      event.filterAndAccept();
      return true;
    }
    select_item(visible_controls[static_cast<std::size_t>(decision->value)],
                input_context);
    event.filterAndAccept();
    return true;
  }

  SetFilteredMenuTitle(input_context, base_title, filter, candidate_list.get());
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
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
  const auto filter_view = scene_menu_filter_.view();
  if (!filter_view.has_value()) {
    HideSceneMenu();
    return;
  }
  SceneMenuProjectionBuilder projection_builder(scene_state_, filter_view->query);
  auto projection = projection_builder.Finish();
  if (!projection.has_value()) {
    FCITX_ERROR() << "fcitx-vinput failed to finalize scene menu projection";
    HideSceneMenu();
    return;
  }

  scene_menu_controls_.clear();
  for (const auto &item : projection->items) {
    scene_menu_controls_.push_back(item.control);
    candidates->append<MenuCandidateWord>(
        item.label, [this, control = item.control](fcitx::InputContext *input_context) {
          ExecuteMenuControl(control, input_context);
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
      fcitx::Text(FrontendText("Current: ") + projection->active_label));
  PublishMenuCandidateList(scene_menu_ic_, std::move(candidates));
  scene_menu_ic_->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
}

bool FcitxVinputAddon::RefreshSceneState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  if (client == nullptr || !client->GetSceneState(&scene_state_, error)) {
    return false;
  }
  const auto active_scene_id = scene_state_.active_scene_id();
  if (!active_scene_id.has_value()) {
    return false;
  }
  if (!active_scene_id->empty()) {
    active_scene_id_ = *active_scene_id;
  }
  return true;
}

void FcitxVinputAddon::HideSceneMenu() {
  auto *ic = scene_menu_ic_;
  scene_menu_visible_ = false;
  scene_menu_ic_ = nullptr;
  scene_menu_filter_.Reset();
  scene_menu_controls_.clear();
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
  const auto title = FrontendText("Scenes /filter");
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsSceneMenuTrigger(event), scene_menu_ic_,
      scene_menu_filter_, scene_menu_controls_, scene_menu_page_, frontend_settings_,
      title, [this](int page) { RebuildSceneMenu(page); },
      [this]() { HideSceneMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *ic) {
        ExecuteMenuControl(control, ic);
      });
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
  const auto filter_view = asr_menu_filter_.view();
  if (!filter_view.has_value()) {
    HideAsrMenu();
    return;
  }
  const auto local = FrontendText("Local");
  const auto remote = FrontendText("Remote");
  const auto command = FrontendText("Command");
  const auto loading_suffix = FrontendText(" (loading)");
  const auto unavailable = FrontendText("unavailable");
  const auto loading_prefix = FrontendText("Loading: ");
  const auto error_prefix = FrontendText("Error: ");
  const AsrMenuLocalization localization{local,          remote,      command,
                                         loading_suffix, unavailable, loading_prefix,
                                         error_prefix};
  AsrMenuProjectionBuilder projection_builder(asr_menu_state_, filter_view->query,
                                              localization);
  auto projection = projection_builder.Finish();
  if (!projection.has_value()) {
    FCITX_ERROR() << "fcitx-vinput failed to finalize ASR menu projection";
    HideAsrMenu();
    return;
  }

  asr_menu_controls_.clear();
  for (const auto &item : projection->items) {
    asr_menu_controls_.push_back(item.control);
    candidates->append<MenuCandidateWord>(
        item.label, [this, control = item.control](fcitx::InputContext *input_context) {
          ExecuteMenuControl(control, input_context);
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
      fcitx::Text(FrontendText("Current: ") + projection->effective_label));
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
  asr_menu_controls_.clear();
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
  const auto title = FrontendText("Models /filter");
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsAsrMenuTrigger(event), asr_menu_ic_, asr_menu_filter_,
      asr_menu_controls_, asr_menu_page_, frontend_settings_, title,
      [this](int page) { RebuildAsrMenu(page); }, [this]() { HideAsrMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *ic) {
        ExecuteMenuControl(control, ic);
      });
}

void FcitxVinputAddon::ExecuteMenuControl(const ProjectedMenuControl &control,
                                          fcitx::InputContext *ic) {
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);

  switch (control.kind) {
  case ProjectedMenuControlKind::SetActiveScene:
    if (client == nullptr ||
        !client->SetActiveScene(control.first, &persisted, &error)) {
      HideSceneMenu();
      ApplyDaemonUnavailable(ic, std::move(error));
      return;
    }
    active_scene_id_ = control.first;
    static_cast<void>(scene_state_.SetActive(control.first));
    HideSceneMenu();
    Notify(FrontendNotificationKind::Info,
           FrontendValueText("Switched scene to '%s'.", control.display_label));
    FCITX_INFO() << "fcitx-vinput switched active scene to " << control.first
                 << " persisted=" << persisted;
    return;
  case ProjectedMenuControlKind::SetActiveAsrTarget:
    if (client == nullptr || !client->SetActiveAsrTarget(control.first, control.second,
                                                         &persisted, &error)) {
      HideAsrMenu();
      ApplyDaemonUnavailable(ic, std::move(error));
      return;
    }
    HideAsrMenu();
    Notify(FrontendNotificationKind::Info,
           FrontendValueText("ASR switch requested for '%s'.", control.display_label));
    FCITX_INFO() << "fcitx-vinput requested ASR target switch to " << control.first
                 << '/' << control.second << " persisted=" << persisted;
    return;
  case ProjectedMenuControlKind::None:
    HideSceneMenu();
    HideAsrMenu();
    return;
  }
}

} // namespace vinput_fcitx_bridge
