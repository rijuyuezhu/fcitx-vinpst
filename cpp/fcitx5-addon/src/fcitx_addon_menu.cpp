#include "vinpst_fcitx_bridge/fcitx_addon.h"

#include "vinpst_fcitx_bridge/dbus_contract.h"

#include "vinpst_fcitx_bridge/fcitx_i18n.h"

#include "vinpst_fcitx_bridge/fcitx_menu_paging.h"
#include "vinpst_fcitx_bridge/fcitx_menu_projection.h"

#include "vinpst_fcitx_bridge/fcitx_selection.h"

#include <dbus_public.h>

#ifdef VINPST_FCITX_HAVE_CLIPBOARD
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

#include <algorithm>
#include <functional>
#include <limits>
#include <string_view>
#include <thread>
#include <utility>

namespace vinpst_fcitx_bridge {
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

template <typename RebuildMenu, typename HideMenu, typename SelectControl>
bool HandleProjectedMenuKeyEvent(fcitx::KeyEvent &event, bool trigger_key,
                                 FcitxProjectedMenuState &menu,
                                 const FrontendSettings &settings,
                                 std::string_view base_title,
                                 RebuildMenu &&rebuild_menu, HideMenu &&hide_menu,
                                 SelectControl &&select_control) {
  auto *input_context = menu.input_context;
  auto &session = menu.session;
  const auto projection = menu.projection;
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

template <typename HideMenu, typename SelectControl>
bool PublishProjectedMenu(FcitxProjectedMenuState &menu,
                          const std::shared_ptr<MenuProjection> &projection, int page,
                          std::string_view base_title, std::string_view menu_name,
                          HideMenu hide_menu, SelectControl select_control) {
  auto *input_context = menu.input_context;
  auto &session = menu.session;
  if (input_context == nullptr || !projection) {
    hide_menu();
    return false;
  }
  const auto item_count = projection->size();
  const auto current_label = projection->summary();
  if (!item_count.has_value() || !current_label.has_value()) {
    FCITX_ERROR() << "fcitx-vinpst failed to read " << menu_name << " menu projection";
    hide_menu();
    return false;
  }

  auto candidates = std::make_unique<fcitx::CommonCandidateList>();
  candidates->setPageSize(kMenuPageSize);
  candidates->setLayoutHint(fcitx::CandidateLayoutHint::Vertical);
  candidates->setCursorPositionAfterPaging(
      fcitx::CursorPositionAfterPaging::ResetToFirst);
  menu.projection = projection;
  for (std::size_t index = 0; index < *item_count; ++index) {
    const auto item = projection->item(index);
    if (!item.has_value()) {
      FCITX_ERROR() << "fcitx-vinpst failed to read " << menu_name << " menu item "
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

template <typename ProjectMenu, typename HideMenu, typename SelectControl>
void RebuildProjectedMenu(FcitxProjectedMenuState &menu, int page,
                          std::string_view base_title, std::string_view menu_name,
                          ProjectMenu project_menu, HideMenu hide_menu,
                          SelectControl select_control) {
  const auto open = menu.session.is_open();
  if (!open.has_value() || !*open || menu.input_context == nullptr) {
    return;
  }

  auto projection = project_menu(menu.session);
  if (!projection) {
    FCITX_ERROR() << "fcitx-vinpst failed to finalize " << menu_name
                  << " menu projection";
    hide_menu();
    return;
  }
  static_cast<void>(PublishProjectedMenu(menu, projection, page, base_title, menu_name,
                                         hide_menu, select_control));
}

void ClearProjectedMenu(FcitxProjectedMenuState &menu) {
  auto *previous_context = menu.input_context;
  menu.input_context = nullptr;
  menu.session.Close();
  menu.projection.reset();
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

void FcitxVinpstAddon::HideResultMenu() {
  auto *input_context = result_menu_ic_.get();
  result_menu_ic_.unwatch();
  ClearResultCandidateMenu(input_context);
}

bool FcitxVinpstAddon::HandleResultMenuKeyEvent(fcitx::KeyEvent &event) {
  auto *input_context = result_menu_ic_.get();
  if (input_context == nullptr) {
    return false;
  }

  auto candidate_list = input_context->inputPanel().candidateList();
  if (candidate_list == nullptr) {
    HideResultMenu();
    return false;
  }
  auto *bulk = candidate_list->toBulk();
  auto *common = dynamic_cast<fcitx::CommonCandidateList *>(candidate_list.get());
  auto *cursor = candidate_list->toCursorMovable();
  auto *pageable = candidate_list->toPageable();
  const int item_count = bulk != nullptr ? bulk->totalSize() : candidate_list->size();
  const int current_page =
      pageable != nullptr && pageable->currentPage() >= 0 ? pageable->currentPage() : 0;
  const int current_selection = common != nullptr ? common->globalCursorIndex() : -1;
  const auto semantic_key =
      ClassifyMenuKey(event.key(), false, false, frontend_settings_.page_prev_keys,
                      frontend_settings_.page_next_keys);
  const auto decision = PlanResultMenuKey(
      event.isRelease(), semantic_key, cursor != nullptr, current_selection,
      current_page, static_cast<std::size_t>(std::max(item_count, 0)));
  if (!decision.has_value()) {
    HideResultMenu();
    return false;
  }

  auto consume = [&event]() {
    event.filterAndAccept();
    return true;
  };
  switch (decision->action) {
  case MenuKeyAction::Pass:
    return false;
  case MenuKeyAction::Consume:
    return consume();
  case MenuKeyAction::CloseAndPass:
    HideResultMenu();
    return false;
  case MenuKeyAction::CloseAndConsume:
    HideResultMenu();
    return consume();
  case MenuKeyAction::Rebuild:
    if (pageable != nullptr) {
      if (decision->value < current_page && pageable->hasPrev()) {
        pageable->prev();
      } else if (decision->value > current_page && pageable->hasNext()) {
        pageable->next();
      }
      input_context->inputPanel().setAuxUp(fcitx::Text(DecoratePagedMenuTitle(
          ResultCandidateMenuTitle(static_cast<std::size_t>(std::max(item_count, 0))),
          candidate_list.get())));
      input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    }
    return consume();
  case MenuKeyAction::MovePrevious:
    cursor->prevCandidate();
    input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    return consume();
  case MenuKeyAction::MoveNext:
    cursor->nextCandidate();
    input_context->updateUserInterface(fcitx::UserInterfaceComponent::InputPanel);
    return consume();
  case MenuKeyAction::Select:
    if (bulk == nullptr || decision->value < 0 || decision->value >= item_count) {
      HideResultMenu();
      return consume();
    }
    bulk->candidateFromAll(static_cast<int>(decision->value)).select(input_context);
    return consume();
  }
  return false;
}

void FcitxVinpstAddon::ShowSceneMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideAsrMenu();
  scene_menu_.input_context = ic;
  scene_menu_.session.Open();
  RequestSceneMenuStateRefresh(ic);
}

void FcitxVinpstAddon::RebuildSceneMenu(int page) {
  RebuildProjectedMenu(
      scene_menu_, page, FrontendText("Scenes /filter"), "scene",
      [this](const MenuSessionState &session) {
        return scene_menu_controller_.Project(session);
      },
      [this]() { HideSceneMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      });
}

bool FcitxVinpstAddon::RefreshSceneState(std::string *error) {
  auto *client = EnsureDaemonClient(error);
  if (client == nullptr) {
    return false;
  }
  if (!client->RefreshSceneMenuController(&scene_menu_controller_, error)) {
    NoteDaemonSyncFailure();
    daemon_client_.reset();
    return false;
  }
  ClearDaemonSyncFailure();
  return true;
}

void FcitxVinpstAddon::RequestSceneMenuStateRefresh(fcitx::InputContext *ic) {
  if (ic == nullptr || menu_refresh_dispatcher_ == nullptr) {
    return;
  }
  const auto seq = ++scene_menu_refresh_seq_;
  auto ic_ref = ic->watch();
  std::weak_ptr<bool> lifetime = menu_refresh_lifetime_;
  auto dispatcher = menu_refresh_dispatcher_;
  std::thread([this, seq, ic_ref = std::move(ic_ref), lifetime, dispatcher]() mutable {
    auto refreshed = std::make_shared<SceneMenuController>();
    std::string error;
    auto client = SdBusDaemonClient::ConnectSession(&error);
    const bool ok = client != nullptr &&
                    client->RefreshSceneMenuController(refreshed.get(), &error);
    if (lifetime.expired()) {
      return;
    }
    dispatcher->schedule([this, seq, ic_ref = std::move(ic_ref), lifetime,
                          refreshed = std::move(refreshed), error = std::move(error),
                          ok]() mutable {
      if (lifetime.expired() || scene_menu_refresh_seq_.load() != seq ||
          !ic_ref.isValid()) {
        return;
      }
      if (!ok) {
        NoteDaemonSyncFailure();
        if (scene_menu_.input_context == ic_ref.get()) {
          HideSceneMenu();
          ApplyDaemonUnavailable(ic_ref.get(), std::move(error));
        }
        return;
      }
      ClearDaemonSyncFailure();
      scene_menu_controller_ = std::move(*refreshed);
      if (scene_menu_.input_context == ic_ref.get()) {
        RebuildSceneMenu();
      }
    });
  }).detach();
}

void FcitxVinpstAddon::HideSceneMenu() {
  ++scene_menu_refresh_seq_;
  ClearProjectedMenu(scene_menu_);
}

bool FcitxVinpstAddon::HandleSceneMenuKeyEvent(fcitx::KeyEvent &event) {
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsSceneMenuTrigger(event), scene_menu_, frontend_settings_,
      FrontendText("Scenes /filter"), [this](int page) { RebuildSceneMenu(page); },
      [this]() { HideSceneMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      });
}

void FcitxVinpstAddon::ShowAsrMenu(fcitx::InputContext *ic) {
  if (ic == nullptr || bridge_.recording()) {
    return;
  }
  HideSceneMenu();
  asr_menu_.input_context = ic;
  asr_menu_.session.Open();
  RequestAsrMenuStateRefresh(ic);
}

void FcitxVinpstAddon::RebuildAsrMenu(int page) {
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
  RebuildProjectedMenu(
      asr_menu_, page, FrontendText("Models /filter"), "ASR",
      [this, &localization](const MenuSessionState &session) {
        return asr_menu_controller_.Project(session, localization);
      },
      [this]() { HideAsrMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      });
}

void FcitxVinpstAddon::RequestAsrMenuStateRefresh(fcitx::InputContext *ic) {
  if (ic == nullptr || menu_refresh_dispatcher_ == nullptr) {
    return;
  }
  const auto seq = ++asr_menu_refresh_seq_;
  auto ic_ref = ic->watch();
  std::weak_ptr<bool> lifetime = menu_refresh_lifetime_;
  auto dispatcher = menu_refresh_dispatcher_;
  std::thread([this, seq, ic_ref = std::move(ic_ref), lifetime, dispatcher]() mutable {
    auto refreshed = std::make_shared<AsrMenuController>();
    std::string error;
    auto client = SdBusDaemonClient::ConnectSession(&error);
    const bool ok =
        client != nullptr && client->RefreshAsrMenuController(refreshed.get(), &error);
    if (lifetime.expired()) {
      return;
    }
    dispatcher->schedule([this, seq, ic_ref = std::move(ic_ref), lifetime,
                          refreshed = std::move(refreshed), error = std::move(error),
                          ok]() mutable {
      if (lifetime.expired() || asr_menu_refresh_seq_.load() != seq ||
          !ic_ref.isValid()) {
        return;
      }
      if (!ok) {
        NoteDaemonSyncFailure();
        if (asr_menu_.input_context == ic_ref.get()) {
          HideAsrMenu();
          ApplyDaemonUnavailable(ic_ref.get(), std::move(error));
        }
        return;
      }
      ClearDaemonSyncFailure();
      asr_menu_controller_ = std::move(*refreshed);
      if (asr_menu_.input_context == ic_ref.get()) {
        RebuildAsrMenu();
      }
    });
  }).detach();
}

void FcitxVinpstAddon::HideAsrMenu() {
  ++asr_menu_refresh_seq_;
  ClearProjectedMenu(asr_menu_);
}

bool FcitxVinpstAddon::HandleAsrMenuKeyEvent(fcitx::KeyEvent &event) {
  return HandleProjectedMenuKeyEvent(
      event, trigger_policy_.IsAsrMenuTrigger(event), asr_menu_, frontend_settings_,
      FrontendText("Models /filter"), [this](int page) { RebuildAsrMenu(page); },
      [this]() { HideAsrMenu(); },
      [this](const ProjectedMenuControl &control, fcitx::InputContext *input_context) {
        ExecuteMenuControl(control, input_context);
      });
}

void FcitxVinpstAddon::ExecuteMenuControl(const ProjectedMenuControl &control,
                                          fcitx::InputContext *ic) {
  std::string error;
  bool persisted = false;
  auto *client = EnsureDaemonClient(&error);

  switch (control.kind) {
  case ProjectedMenuControlKind::SetActiveScene:
    if (client == nullptr ||
        !client->SetActiveScene(&scene_menu_controller_, control.first, &persisted,
                                &error)) {
      if (client != nullptr) {
        NoteDaemonSyncFailure();
        daemon_client_.reset();
      }
      HideSceneMenu();
      ApplyDaemonUnavailable(ic, std::move(error));
      return;
    }
    ClearDaemonSyncFailure();
    HideSceneMenu();
    Notify(FrontendNotificationKind::Info,
           FrontendValueText("Switched scene to '%s'.", control.display_label));
    FCITX_INFO() << "fcitx-vinpst switched active scene to " << control.first
                 << " persisted=" << persisted;
    return;
  case ProjectedMenuControlKind::SetActiveAsrTarget:
    if (client == nullptr || !client->SetActiveAsrTarget(control.first, control.second,
                                                         &persisted, &error)) {
      if (client != nullptr) {
        NoteDaemonSyncFailure();
        daemon_client_.reset();
      }
      HideAsrMenu();
      ApplyDaemonUnavailable(ic, std::move(error));
      return;
    }
    ClearDaemonSyncFailure();
    HideAsrMenu();
    Notify(FrontendNotificationKind::Info,
           FrontendValueText("ASR switch requested for '%s'.", control.display_label));
    FCITX_INFO() << "fcitx-vinpst requested ASR target switch to " << control.first
                 << '/' << control.second << " persisted=" << persisted;
    return;
  case ProjectedMenuControlKind::None:
    HideSceneMenu();
    HideAsrMenu();
    return;
  }
}

} // namespace vinpst_fcitx_bridge
