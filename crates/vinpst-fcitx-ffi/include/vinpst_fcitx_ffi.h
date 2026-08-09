#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VinpstFcitxAsrMenuController VinpstFcitxAsrMenuController;
typedef struct VinpstFcitxDaemonClient VinpstFcitxDaemonClient;
typedef struct VinpstFcitxDaemonLiveState VinpstFcitxDaemonLiveState;
typedef struct VinpstFcitxFrontendController VinpstFcitxFrontendController;
typedef struct VinpstFcitxFrontendOutcome VinpstFcitxFrontendOutcome;
typedef struct VinpstFcitxFrontendPresentation VinpstFcitxFrontendPresentation;
typedef struct VinpstFcitxMenuSession VinpstFcitxMenuSession;
typedef struct VinpstFcitxMenuProjection VinpstFcitxMenuProjection;
typedef struct VinpstFcitxOwnedString VinpstFcitxOwnedString;
typedef struct VinpstFcitxSceneMenuController VinpstFcitxSceneMenuController;
typedef struct VinpstFcitxTriggerState VinpstFcitxTriggerState;

typedef struct VinpstFcitxStringView {
  const uint8_t *data;
  size_t len;
} VinpstFcitxStringView;

typedef struct VinpstFcitxFrontendPresentationView {
  uint8_t kind;
  uint8_t replace_selection;
  VinpstFcitxStringView text;
  size_t candidate_count;
  size_t cursor_index;
  size_t context_entry_count;
  uint8_t suppress_commit_context;
} VinpstFcitxFrontendPresentationView;

typedef struct VinpstFcitxPresentedCandidateView {
  VinpstFcitxStringView text;
  VinpstFcitxStringView comment;
  uint8_t commit;
  VinpstFcitxStringView context_source;
  uint8_t suppress_commit_context;
} VinpstFcitxPresentedCandidateView;

typedef struct VinpstFcitxContextEntryView {
  VinpstFcitxStringView text;
  VinpstFcitxStringView source;
} VinpstFcitxContextEntryView;

typedef struct VinpstFcitxFrontendPresentationTextView {
  VinpstFcitxStringView original;
  VinpstFcitxStringView voice_command;
  VinpstFcitxStringView cancel;
} VinpstFcitxFrontendPresentationTextView;

typedef struct VinpstFcitxMenuKeyDecisionView {
  uint8_t action;
  int64_t value;
} VinpstFcitxMenuKeyDecisionView;

typedef struct VinpstFcitxMenuKeyInputView {
  uint8_t release;
  uint8_t key_kind;
  int64_t key_value;
  VinpstFcitxStringView text;
  uint8_t cursor_available;
  int64_t current_selection;
  size_t visible_item_count;
} VinpstFcitxMenuKeyInputView;

typedef struct VinpstFcitxMenuProjectionView {
  VinpstFcitxStringView summary;
  size_t item_count;
} VinpstFcitxMenuProjectionView;

typedef struct VinpstFcitxAsrMenuTextView {
  VinpstFcitxStringView local;
  VinpstFcitxStringView remote;
  VinpstFcitxStringView command;
  VinpstFcitxStringView loading_suffix;
  VinpstFcitxStringView unavailable;
  VinpstFcitxStringView loading_prefix;
  VinpstFcitxStringView error_prefix;
} VinpstFcitxAsrMenuTextView;

typedef struct VinpstFcitxProjectedMenuItemView {
  VinpstFcitxStringView label;
  uint8_t control_kind;
  VinpstFcitxStringView control_first;
  VinpstFcitxStringView control_second;
  VinpstFcitxStringView control_label;
} VinpstFcitxProjectedMenuItemView;

enum {
  VINPST_FCITX_MENU_CONTROL_NONE = 0,
  VINPST_FCITX_MENU_CONTROL_SET_ACTIVE_SCENE = 1,
  VINPST_FCITX_MENU_CONTROL_SET_ACTIVE_ASR_TARGET = 2,
};

typedef struct VinpstFcitxTriggerEventView {
  uint8_t kind;
  uint8_t value;
  uint8_t flag;
  int64_t now_ns;
} VinpstFcitxTriggerEventView;


typedef struct VinpstFcitxDaemonSignalPlanView {
  uint8_t kind;
  uint8_t translate;
  VinpstFcitxStringView text;
} VinpstFcitxDaemonSignalPlanView;

typedef struct VinpstFcitxDaemonNotificationView {
  VinpstFcitxStringView code;
  VinpstFcitxStringView subject;
  VinpstFcitxStringView detail;
  VinpstFcitxStringView raw;
} VinpstFcitxDaemonNotificationView;

typedef struct VinpstFcitxDaemonStatusView {
  VinpstFcitxStringView status;
  uint8_t command_mode;
  VinpstFcitxStringView partial;
} VinpstFcitxDaemonStatusView;

typedef struct VinpstFcitxDaemonControlView {
  uint8_t event;
  VinpstFcitxStringView status;
  uint8_t flag;
  uint8_t recording;
  uint8_t remote_status_active;
} VinpstFcitxDaemonControlView;

typedef struct VinpstFcitxAsrTargetView {
  VinpstFcitxStringView provider;
  VinpstFcitxStringView model;
} VinpstFcitxAsrTargetView;

enum {
  VINPST_FCITX_DAEMON_CONTROL_EVENT_AVAILABILITY_CHANGED = 0,
  VINPST_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED = 1,
  VINPST_FCITX_DAEMON_CONTROL_EVENT_RECONCILE_BEFORE_START = 2,
};

enum {
  VINPST_FCITX_DAEMON_CONTROL_PLAN_NONE = 0,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_UNAVAILABLE = 1,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_REMOTE_STATUS = 2,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_RESET_LOCAL_RECORDING = 3,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_UPDATE_LOCAL_PREEDIT = 4,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_PRESENT_REMOTE_STATUS = 5,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_AND_STOP_NORMAL = 6,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_CLEAR_DAEMON_ERROR = 7,
  VINPST_FCITX_DAEMON_CONTROL_PLAN_ADOPT_EXTERNAL_STATUS = 8,
};

enum {
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_CLEAR = 0,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_PARTIAL = 1,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_RECORDING = 2,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_COMMANDING = 3,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_RECOGNIZING = 4,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_POSTPROCESSING = 5,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_INFO = 6,
  VINPST_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_ERROR = 7,
};

enum {
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_NONE = 0,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_START_NORMAL = 1,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_STOP_NORMAL = 2,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_START_COMMAND = 3,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_STOP_COMMAND = 4,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU = 5,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_SCENE_MENU_RELEASE = 6,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU = 7,
  VINPST_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE = 8,
};

enum {
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_NONE = 0,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_START_NORMAL = 1,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_STOP_NORMAL = 2,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_START_COMMAND = 3,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_STOP_COMMAND = 4,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU = 5,
  VINPST_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU = 6,
};

enum {
  VINPST_FCITX_FRONTEND_OUTCOME_NONE = 0,
  VINPST_FCITX_FRONTEND_OUTCOME_PREEDIT = 1,
  VINPST_FCITX_FRONTEND_OUTCOME_CLEAR = 2,
  VINPST_FCITX_FRONTEND_OUTCOME_COMMIT = 3,
  VINPST_FCITX_FRONTEND_OUTCOME_CANDIDATE_MENU = 4,
  VINPST_FCITX_FRONTEND_OUTCOME_ERROR = 5,
};

enum {
  VINPST_FCITX_TRIGGER_MODE_TAP = 0,
  VINPST_FCITX_TRIGGER_MODE_HOLD = 1,
  VINPST_FCITX_TRIGGER_MODE_BOTH = 2,
};

enum {
  VINPST_FCITX_TRIGGER_KIND_NORMAL = 0,
  VINPST_FCITX_TRIGGER_KIND_COMMAND = 1,
};

enum {
  VINPST_FCITX_TRIGGER_ACTION_NONE = 0,
  VINPST_FCITX_TRIGGER_ACTION_CONSUME = 1,
  VINPST_FCITX_TRIGGER_ACTION_START_NORMAL = 2,
  VINPST_FCITX_TRIGGER_ACTION_START_COMMAND = 3,
  VINPST_FCITX_TRIGGER_ACTION_STOP_ACTIVE = 4,
  VINPST_FCITX_TRIGGER_ACTION_SCHEDULE_NORMAL_START = 5,
  VINPST_FCITX_TRIGGER_ACTION_SCHEDULE_COMMAND_START = 6,
  VINPST_FCITX_TRIGGER_ACTION_CANCEL_PENDING_START = 7,
  VINPST_FCITX_TRIGGER_ACTION_SCHEDULE_STOP = 8,
};

enum {
  VINPST_FCITX_TRIGGER_EVENT_SET_MODE = 0,
  VINPST_FCITX_TRIGGER_EVENT_PRESS = 1,
  VINPST_FCITX_TRIGGER_EVENT_RELEASE = 2,
  VINPST_FCITX_TRIGGER_EVENT_FIRE_PENDING_START = 3,
  VINPST_FCITX_TRIGGER_EVENT_FIRE_PENDING_STOP = 4,
  VINPST_FCITX_TRIGGER_EVENT_CONFIRM_START = 5,
  VINPST_FCITX_TRIGGER_EVENT_RECORDING_STOPPED = 6,
};

enum {
  VINPST_FCITX_MENU_KEY_OTHER = 0,
  VINPST_FCITX_MENU_KEY_PASSIVE = 1,
  VINPST_FCITX_MENU_KEY_ESCAPE = 2,
  VINPST_FCITX_MENU_KEY_SLASH = 3,
  VINPST_FCITX_MENU_KEY_BACKSPACE = 4,
  VINPST_FCITX_MENU_KEY_DELETE_WORD = 5,
  VINPST_FCITX_MENU_KEY_CLEAR_FILTER = 6,
  VINPST_FCITX_MENU_KEY_TEXT = 7,
  VINPST_FCITX_MENU_KEY_PAGE = 8,
  VINPST_FCITX_MENU_KEY_DIGIT = 9,
  VINPST_FCITX_MENU_KEY_MOVE_PREVIOUS = 10,
  VINPST_FCITX_MENU_KEY_MOVE_NEXT = 11,
  VINPST_FCITX_MENU_KEY_ENTER = 12,
};

enum {
  VINPST_FCITX_MENU_ACTION_PASS = 0,
  VINPST_FCITX_MENU_ACTION_CONSUME = 1,
  VINPST_FCITX_MENU_ACTION_CLOSE_AND_PASS = 2,
  VINPST_FCITX_MENU_ACTION_CLOSE_AND_CONSUME = 3,
  VINPST_FCITX_MENU_ACTION_REBUILD = 4,
  VINPST_FCITX_MENU_ACTION_MOVE_PREVIOUS = 5,
  VINPST_FCITX_MENU_ACTION_MOVE_NEXT = 6,
  VINPST_FCITX_MENU_ACTION_SELECT = 7,
};

enum { VINPST_FCITX_MENU_PAGE_SIZE = 10 };

VinpstFcitxMenuSession *vinpst_fcitx_menu_session_new(void);
void vinpst_fcitx_menu_session_free(VinpstFcitxMenuSession *session);
uint8_t vinpst_fcitx_menu_session_open(VinpstFcitxMenuSession *session);
uint8_t vinpst_fcitx_menu_session_close(VinpstFcitxMenuSession *session);
uint8_t vinpst_fcitx_menu_session_is_open(
    const VinpstFcitxMenuSession *session, uint8_t *open_out);
uint8_t vinpst_fcitx_menu_session_set_page(
    VinpstFcitxMenuSession *session, int32_t page);
uint8_t vinpst_fcitx_menu_session_filter_active(
    const VinpstFcitxMenuSession *session, uint8_t *active_out);
uint8_t vinpst_fcitx_menu_session_decorate_title(
    VinpstFcitxMenuSession *session, const uint8_t *base_data,
    size_t base_len, VinpstFcitxStringView *title_out);
uint8_t vinpst_fcitx_menu_session_handle_key(
    VinpstFcitxMenuSession *session,
    const VinpstFcitxMenuKeyInputView *input,
    VinpstFcitxMenuKeyDecisionView *decision_out);
int32_t vinpst_fcitx_clamp_menu_page(int32_t total_pages,
                                     int32_t requested_page);

VinpstFcitxSceneMenuController *vinpst_fcitx_scene_menu_controller_new(void);
void vinpst_fcitx_scene_menu_controller_free(
    VinpstFcitxSceneMenuController *controller);
VinpstFcitxMenuProjection *vinpst_fcitx_scene_menu_controller_projection_new(
    const VinpstFcitxSceneMenuController *controller,
    const VinpstFcitxMenuSession *session);
VinpstFcitxAsrMenuController *vinpst_fcitx_asr_menu_controller_new(void);
void vinpst_fcitx_asr_menu_controller_free(
    VinpstFcitxAsrMenuController *controller);
VinpstFcitxMenuProjection *vinpst_fcitx_asr_menu_controller_projection_new(
    const VinpstFcitxAsrMenuController *controller,
    const VinpstFcitxMenuSession *session,
    const VinpstFcitxAsrMenuTextView *text);

void vinpst_fcitx_menu_projection_free(VinpstFcitxMenuProjection *projection);
uint8_t vinpst_fcitx_menu_projection_view(
    const VinpstFcitxMenuProjection *projection,
    VinpstFcitxMenuProjectionView *view_out);
uint8_t vinpst_fcitx_menu_projection_item_view(
    const VinpstFcitxMenuProjection *projection, size_t index,
    VinpstFcitxProjectedMenuItemView *view_out);

uint8_t vinpst_fcitx_daemon_control_plan(
    const VinpstFcitxDaemonControlView *control);
VinpstFcitxDaemonLiveState *vinpst_fcitx_daemon_live_state_new(void);
void vinpst_fcitx_daemon_live_state_free(VinpstFcitxDaemonLiveState *state);
uint8_t vinpst_fcitx_daemon_live_state_reset(VinpstFcitxDaemonLiveState *state);
uint8_t vinpst_fcitx_daemon_live_state_begin_status(
    VinpstFcitxDaemonLiveState *state, const uint8_t *status_data,
    size_t status_len, uint8_t command_mode);
uint8_t vinpst_fcitx_daemon_live_state_update_status(
    VinpstFcitxDaemonLiveState *state, const uint8_t *status_data,
    size_t status_len);
uint8_t vinpst_fcitx_daemon_live_state_update_partial(
    VinpstFcitxDaemonLiveState *state, const uint8_t *partial_data,
    size_t partial_len, uint8_t recording);
uint8_t vinpst_fcitx_daemon_live_state_preedit_plan(
    const VinpstFcitxDaemonLiveState *state,
    VinpstFcitxDaemonSignalPlanView *view_out);
uint8_t vinpst_fcitx_daemon_live_state_command_mode(
    const VinpstFcitxDaemonLiveState *state);
uint8_t vinpst_fcitx_daemon_status_preedit_plan(
    const VinpstFcitxDaemonStatusView *status,
    VinpstFcitxDaemonSignalPlanView *view_out);
uint8_t vinpst_fcitx_daemon_notification_plan(
    const VinpstFcitxDaemonNotificationView *notification,
    VinpstFcitxDaemonSignalPlanView *view_out);

VinpstFcitxDaemonClient *vinpst_fcitx_daemon_client_connect(
    VinpstFcitxOwnedString **error_out);
void vinpst_fcitx_daemon_client_free(VinpstFcitxDaemonClient *client);
VinpstFcitxOwnedString *vinpst_fcitx_daemon_client_get_status(
    const VinpstFcitxDaemonClient *client,
    VinpstFcitxOwnedString **error_out);
uint8_t vinpst_fcitx_daemon_client_refresh_scene_menu_controller(
    const VinpstFcitxDaemonClient *client,
    VinpstFcitxSceneMenuController *controller,
    VinpstFcitxOwnedString **error_out);
uint8_t vinpst_fcitx_daemon_client_set_active_scene(
    const VinpstFcitxDaemonClient *client,
    VinpstFcitxSceneMenuController *controller,
    const uint8_t *scene_data, size_t scene_len, uint8_t *persisted_out,
    VinpstFcitxOwnedString **error_out);
uint8_t vinpst_fcitx_daemon_client_refresh_asr_menu_controller(
    const VinpstFcitxDaemonClient *client,
    VinpstFcitxAsrMenuController *controller,
    VinpstFcitxOwnedString **error_out);
uint8_t vinpst_fcitx_daemon_client_set_active_asr_target(
    const VinpstFcitxDaemonClient *client,
    const VinpstFcitxAsrTargetView *target, uint8_t *persisted_out,
    VinpstFcitxOwnedString **error_out);
void vinpst_fcitx_owned_string_free(VinpstFcitxOwnedString *value);
uint8_t vinpst_fcitx_owned_string_view(
    const VinpstFcitxOwnedString *value, VinpstFcitxStringView *view_out);

VinpstFcitxTriggerState *vinpst_fcitx_trigger_state_new(uint8_t mode);
void vinpst_fcitx_trigger_state_free(VinpstFcitxTriggerState *state);
uint8_t vinpst_fcitx_trigger_state_dispatch(
    VinpstFcitxTriggerState *state, const VinpstFcitxTriggerEventView *event,
    uint8_t *action_out);

VinpstFcitxFrontendController *vinpst_fcitx_frontend_controller_new(void);
void vinpst_fcitx_frontend_controller_free(
    VinpstFcitxFrontendController *controller);
uint8_t vinpst_fcitx_frontend_controller_recording(
    const VinpstFcitxFrontendController *controller);
uint8_t vinpst_fcitx_frontend_controller_command_mode(
    const VinpstFcitxFrontendController *controller);
uint8_t vinpst_fcitx_frontend_controller_plan_trigger(
    const VinpstFcitxFrontendController *controller, uint8_t request,
    uint8_t *intent_out);
uint8_t vinpst_fcitx_frontend_controller_prepare_start_normal(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxSceneMenuController *scene_controller);
uint8_t vinpst_fcitx_frontend_controller_prepare_start_command(
    VinpstFcitxFrontendController *controller, const uint8_t *selected_data,
    size_t selected_len, const uint8_t *scene_data, size_t scene_len);
uint8_t vinpst_fcitx_frontend_controller_prepare_stop(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxSceneMenuController *scene_controller);
uint8_t vinpst_fcitx_frontend_controller_prepare_adopt_and_stop(
    VinpstFcitxFrontendController *controller, uint8_t command_mode,
    const VinpstFcitxSceneMenuController *scene_controller);
uint8_t vinpst_fcitx_frontend_controller_adopt_external_recording(
    VinpstFcitxFrontendController *controller, uint8_t command_mode,
    const VinpstFcitxSceneMenuController *scene_controller);
uint8_t vinpst_fcitx_frontend_controller_pending_argument(
    const VinpstFcitxFrontendController *controller,
    VinpstFcitxStringView *argument_out);
VinpstFcitxFrontendOutcome *vinpst_fcitx_frontend_controller_complete(
    VinpstFcitxFrontendController *controller, uint8_t success,
    const uint8_t *response_data, size_t response_len);
VinpstFcitxFrontendOutcome *vinpst_fcitx_frontend_controller_complete_recognition_result(
    VinpstFcitxFrontendController *controller, const uint8_t *response_data,
    size_t response_len);
VinpstFcitxFrontendOutcome *
vinpst_fcitx_frontend_controller_start_normal_with_daemon(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxDaemonClient *daemon,
    const VinpstFcitxSceneMenuController *scene_controller);
VinpstFcitxFrontendOutcome *
vinpst_fcitx_frontend_controller_start_command_with_daemon(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxDaemonClient *daemon, const uint8_t *selected_data,
    size_t selected_len, const uint8_t *scene_data, size_t scene_len);
VinpstFcitxFrontendOutcome *
vinpst_fcitx_frontend_controller_stop_with_daemon(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxDaemonClient *daemon,
    const VinpstFcitxSceneMenuController *scene_controller);
VinpstFcitxFrontendOutcome *
vinpst_fcitx_frontend_controller_adopt_and_stop_with_daemon(
    VinpstFcitxFrontendController *controller,
    const VinpstFcitxDaemonClient *daemon, uint8_t command_mode,
    const VinpstFcitxSceneMenuController *scene_controller);
uint8_t vinpst_fcitx_frontend_controller_reset(
    VinpstFcitxFrontendController *controller);

void vinpst_fcitx_frontend_outcome_free(
    VinpstFcitxFrontendOutcome *outcome);
VinpstFcitxFrontendPresentation *vinpst_fcitx_frontend_presentation_new(
    const VinpstFcitxFrontendOutcome *outcome,
    const VinpstFcitxFrontendPresentationTextView *text);
void vinpst_fcitx_frontend_presentation_free(
    VinpstFcitxFrontendPresentation *presentation);
uint8_t vinpst_fcitx_frontend_presentation_view(
    const VinpstFcitxFrontendPresentation *presentation,
    VinpstFcitxFrontendPresentationView *view_out);
uint8_t vinpst_fcitx_frontend_presentation_candidate(
    const VinpstFcitxFrontendPresentation *presentation, size_t index,
    VinpstFcitxPresentedCandidateView *view_out);
uint8_t vinpst_fcitx_frontend_presentation_context_entry(
    const VinpstFcitxFrontendPresentation *presentation, size_t index,
    VinpstFcitxContextEntryView *view_out);

typedef struct VinpstFcitxContextHistory VinpstFcitxContextHistory;
VinpstFcitxContextHistory *vinpst_fcitx_context_history_new(void);
void vinpst_fcitx_context_history_free(VinpstFcitxContextHistory *history);
void vinpst_fcitx_context_history_reload(VinpstFcitxContextHistory *history);
uint8_t vinpst_fcitx_context_history_user_commit(VinpstFcitxContextHistory *history,
                                                 size_t context,
                                                 const uint8_t *text_data,
                                                 size_t text_len);
void vinpst_fcitx_context_history_append_entry(VinpstFcitxContextHistory *history,
                                               const uint8_t *text_data,
                                               size_t text_len,
                                               const uint8_t *source_data,
                                               size_t source_len);
void vinpst_fcitx_context_history_suppress_next(VinpstFcitxContextHistory *history,
                                                const uint8_t *text_data,
                                                size_t text_len);
void vinpst_fcitx_context_history_context_destroyed(VinpstFcitxContextHistory *history,
                                                    size_t context);
void vinpst_fcitx_context_history_flush(VinpstFcitxContextHistory *history);

#ifdef __cplusplus
}
#endif
