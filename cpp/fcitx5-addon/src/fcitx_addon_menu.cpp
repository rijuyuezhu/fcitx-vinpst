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
#include <optional>
#include <string_view>
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
                          MenuSessionState &session,
                          fcitx::CandidateList *candidate_list) {
  if (ic == nullptr) {
    return;
  }
  ic->inputPanel().setAuxUp(fcitx::Text(
      DecoratePagedMenuTitle(session.DecorateTitle(base_title), candidate_list)));
}

template <typename Projection, typename RebuildMenu, typename HideMenu,
          typename SelectControl>
bool HandleProjectedMenuKeyEvent(
    fcitx::KeyEvent &event, bool trigger_key, fcitx::InputContext *input_context,
    MenuSessionState &session, const std::shared_ptr<Projection> &projection,
    const FrontendSettings &settings, std::string_view base_title,
    RebuildMenu &&rebuild_menu, HideMenu &&hide_menu, SelectControl &&select_control) {
  const auto open = session.is_open();
  if (!open.has_value() || !*open || input_context == nullptr || !projection) {
    return false;
  }
  const auto item_count = projection->size();
  if (!item_count.has_value()) {
    hide_menu();
    return false;
  }
  auto candidate_list = input_context->inputPanel().candidateList();
  auto *cursor =
      candidate_list != nullptr ? candidate_list->toCursorMovable() : nullptr;
  const auto filter_active = session.active();
  if (!filter_active.has_value()) {
    hide_menu();
    return false;
  }
  const auto semantic_key =
      ClassifyMenuKey(event.key(), trigger_key, *filter_active, settings.page_prev_keys,
                      settings.page_next_keys);
  const auto decision =
      session.HandleKey(event.isRelease(), semantic_key, cursor != nullptr,
                        CurrentMenuSelectionIndex(candidate_list.get()), *item_count);
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
        static_cast<std::uint64_t>(decision->value) >= *item_count) {
      hide_menu();
      event.filterAndAccept();
      return true;
    }
    const auto item = projection->item(static_cast<std::size_t>(decision->value));
    if (!item.has_value()) {
      hide_menu();
      event.filterAndAccept();
      return true;
    }
    select_control(item->control, input_context);
    event.filterAndAccept();
    return true;
  }

  SetFilteredMenuTitle(input_context, base_title, session, candidate_list.get());
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  event.filterAndAccept();
  return true;
}

template <typename Projection, typename HideMenu, typename SelectControl>
bool PublishProjectedMenu(fcitx::InputContext *input_context, MenuSessionState &session,
                          std::shared_ptr<Projection> &stored_projection,
                          const std::shared_ptr<Projection> &projection,
                          std::optional<std::string> current_label, int page,
                          std::string_view base_title, std::string_view menu_name,
                          HideMenu hide_menu, SelectControl select_control) {
  if (input_context == nullptr || !projection) {
    hide_menu();
    return false;
  }
  const auto item_count = projection->size();
  if (!item_count.has_value() || !current_label.has_value()) {
    FCITX_ERROR() << "fcitx-vinput failed to read " << menu_name << " menu projection";
    hide_menu();
    return false;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  stored_projection = projection;
  for (std::size_t index = 0; index < *item_count; ++index) {
    const auto item = projection->item(index);
    if (!item.has_value()) {
      FCITX_ERROR() << "fcitx-vinput failed to read " << menu_name << " menu item "
                    << index;
      hide_menu();
      return false;
    }
    candidates->append<MenuCandidateWord>(
        item->label, [projection, index, hide_menu,
                      select_control](fcitx::InputContext *selected_context) {
          const auto selected = projection->item(index);
          if (!selected.has_value()) {
            hide_menu();
            return;
          }
          select_control(selected->control, selected_context);
        });
  }

  if (candidates->totalSize() > 0) {
    candidates->setGlobalCursorIndex(0);
    SetMenuCandidatePage(*candidates, page);
  }
  const int actual_page = candidates->totalSize() > 0 ? candidates->currentPage() : 0;
  if (!session.SetPage(actual_page)) {
    hide_menu();
    return false;
  }

  SetFilteredMenuTitle(input_context, base_title, session, candidates.get());
  input_context->inputPanel().setAuxDown(
      fcitx::Text(FrontendText("Current: ") + *current_label));
  PublishMenuCandidateList(input_context, std::move(candidates));
  input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
  return true;
}

template <typename Projection>
void ClearProjectedMenu(fcitx::InputContext *&input_context, MenuSessionState &session,
                        std::shared_ptr<Projection> &projection) {
  auto *previous_context = input_context;
  input_context = nullptr;
  session.Close();
  projection.reset();
  if (previous_context == nullptr) {
    return;
  }
  fcitx::Text empty;
  previous_context->inputPanel().setAuxUp(empty);
  previous_context->inputPanel().setAuxDown(empty);
  previous_context->inputPanel().setCandidateList({});
  previous_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
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
  scene_menu_session_.Open();
  RebuildSceneMenu();
}

void FcitxVinputAddon::RebuildSceneMenu(int page) {
  const auto open = scene_menu_session_.is_open();
  if (!open.has_value() || !*open || scene_menu_ic_ == nullptr) {
    return;
  }

  auto projection = scene_menu_controller_.Project(scene_menu_session_);
  if (!projection) {
    FCITX_ERROR() << "fcitx-vinput failed to finalize scene menu projection";
    HideSceneMenu();
    return;
  }
  static_cast<void>(PublishProjectedMenu(
      scene_menu_ic_, scene_menu_session_, scene_menu_projection_, projection,
      projection->summary(), page, FrontendText("Scenes /filter"), "scene",
      [this]() { HideSceneMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      }));
}

bool FcitxVinputAddon::RefreshSceneState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  return client != nullptr &&
         client->RefreshSceneMenuController(&scene_menu_controller_, error);
}

void FcitxVinputAddon::HideSceneMenu() {
  ClearProjectedMenu(scene_menu_ic_, scene_menu_session_, scene_menu_projection_);
}

bool FcitxVinputAddon::HandleSceneMenuKeyEvent(fcitx::KeyEvent &event) {
  const auto projection = scene_menu_projection_;
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsSceneMenuTrigger(event), scene_menu_ic_,
      scene_menu_session_, projection, frontend_settings_,
      FrontendText("Scenes /filter"), [this](int page) { RebuildSceneMenu(page); },
      [this]() { HideSceneMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
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
  asr_menu_session_.Open();
  RebuildAsrMenu();
}

void FcitxVinputAddon::RebuildAsrMenu(int page) {
  const auto open = asr_menu_session_.is_open();
  if (!open.has_value() || !*open || asr_menu_ic_ == nullptr) {
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
  auto projection = asr_menu_controller_.Project(asr_menu_session_, localization);
  if (!projection) {
    FCITX_ERROR() << "fcitx-vinput failed to finalize ASR menu projection";
    HideAsrMenu();
    return;
  }
  static_cast<void>(PublishProjectedMenu(
      asr_menu_ic_, asr_menu_session_, asr_menu_projection_, projection,
      projection->summary(), page, FrontendText("Models /filter"), "ASR",
      [this]() { HideAsrMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      }));
}

bool FcitxVinputAddon::RefreshAsrMenuState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  return client != nullptr &&
         client->RefreshAsrMenuController(&asr_menu_controller_, error);
}

void FcitxVinputAddon::HideAsrMenu() {
  ClearProjectedMenu(asr_menu_ic_, asr_menu_session_, asr_menu_projection_);
}

bool FcitxVinputAddon::HandleAsrMenuKeyEvent(fcitx::KeyEvent &event) {
  const auto projection = asr_menu_projection_;
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsAsrMenuTrigger(event), asr_menu_ic_, asr_menu_session_,
      projection, frontend_settings_, FrontendText("Models /filter"),
      [this](int page) { RebuildAsrMenu(page); }, [this]() { HideAsrMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
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
        !client->SetActiveScene(&scene_menu_controller_, control.first, &persisted,
                                &error)) {
      HideSceneMenu();
      ApplyDaemonUnavailable(ic, std::move(error));
      return;
    }
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
