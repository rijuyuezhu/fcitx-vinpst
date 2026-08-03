#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VinputFcitxAsrMenuController VinputFcitxAsrMenuController;
typedef struct VinputFcitxDaemonClient VinputFcitxDaemonClient;
typedef struct VinputFcitxDaemonLiveState VinputFcitxDaemonLiveState;
typedef struct VinputFcitxFrontendController VinputFcitxFrontendController;
typedef struct VinputFcitxFrontendOutcome VinputFcitxFrontendOutcome;
typedef struct VinputFcitxFrontendPresentation VinputFcitxFrontendPresentation;
typedef struct VinputFcitxMenuSession VinputFcitxMenuSession;
typedef struct VinputFcitxMenuProjection VinputFcitxMenuProjection;
typedef struct VinputFcitxOwnedString VinputFcitxOwnedString;
typedef struct VinputFcitxSceneMenuController VinputFcitxSceneMenuController;
typedef struct VinputFcitxTriggerState VinputFcitxTriggerState;

typedef struct VinputFcitxStringView {
  const uint8_t *data;
  size_t len;
} VinputFcitxStringView;

typedef struct VinputFcitxFrontendPresentationView {
  uint8_t kind;
  uint8_t replace_selection;
  VinputFcitxStringView text;
  size_t candidate_count;
  size_t cursor_index;
} VinputFcitxFrontendPresentationView;

typedef struct VinputFcitxPresentedCandidateView {
  VinputFcitxStringView text;
  VinputFcitxStringView comment;
  uint8_t commit;
} VinputFcitxPresentedCandidateView;

typedef struct VinputFcitxFrontendPresentationTextView {
  VinputFcitxStringView original;
  VinputFcitxStringView voice_command;
  VinputFcitxStringView cancel;
} VinputFcitxFrontendPresentationTextView;

typedef struct VinputFcitxMenuKeyDecisionView {
  uint8_t action;
  int64_t value;
} VinputFcitxMenuKeyDecisionView;

typedef struct VinputFcitxMenuKeyInputView {
  uint8_t release;
  uint8_t key_kind;
  int64_t key_value;
  VinputFcitxStringView text;
  uint8_t cursor_available;
  int64_t current_selection;
  size_t visible_item_count;
} VinputFcitxMenuKeyInputView;

typedef struct VinputFcitxMenuProjectionView {
  VinputFcitxStringView summary;
  size_t item_count;
} VinputFcitxMenuProjectionView;

typedef struct VinputFcitxAsrMenuTextView {
  VinputFcitxStringView local;
  VinputFcitxStringView remote;
  VinputFcitxStringView command;
  VinputFcitxStringView loading_suffix;
  VinputFcitxStringView unavailable;
  VinputFcitxStringView loading_prefix;
  VinputFcitxStringView error_prefix;
} VinputFcitxAsrMenuTextView;

typedef struct VinputFcitxProjectedMenuItemView {
  VinputFcitxStringView label;
  uint8_t control_kind;
  VinputFcitxStringView control_first;
  VinputFcitxStringView control_second;
  VinputFcitxStringView control_label;
} VinputFcitxProjectedMenuItemView;

enum {
  VINPUT_FCITX_MENU_CONTROL_NONE = 0,
  VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_SCENE = 1,
  VINPUT_FCITX_MENU_CONTROL_SET_ACTIVE_ASR_TARGET = 2,
};

typedef struct VinputFcitxTriggerEventView {
  uint8_t kind;
  uint8_t value;
  uint8_t flag;
  int64_t now_ns;
} VinputFcitxTriggerEventView;


typedef struct VinputFcitxDaemonSignalPlanView {
  uint8_t kind;
  uint8_t translate;
  VinputFcitxStringView text;
} VinputFcitxDaemonSignalPlanView;

typedef struct VinputFcitxDaemonNotificationView {
  VinputFcitxStringView code;
  VinputFcitxStringView subject;
  VinputFcitxStringView detail;
  VinputFcitxStringView raw;
} VinputFcitxDaemonNotificationView;

typedef struct VinputFcitxDaemonStatusView {
  VinputFcitxStringView status;
  uint8_t command_mode;
  VinputFcitxStringView partial;
} VinputFcitxDaemonStatusView;

typedef struct VinputFcitxDaemonControlView {
  uint8_t event;
  VinputFcitxStringView status;
  uint8_t flag;
  uint8_t recording;
  uint8_t remote_status_active;
} VinputFcitxDaemonControlView;

typedef struct VinputFcitxAsrTargetView {
  VinputFcitxStringView provider;
  VinputFcitxStringView model;
} VinputFcitxAsrTargetView;

enum {
  VINPUT_FCITX_DAEMON_CONTROL_EVENT_AVAILABILITY_CHANGED = 0,
  VINPUT_FCITX_DAEMON_CONTROL_EVENT_STATUS_CHANGED = 1,
  VINPUT_FCITX_DAEMON_CONTROL_EVENT_RECONCILE_BEFORE_START = 2,
};

enum {
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_NONE = 0,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_RESET_UNAVAILABLE = 1,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_CLEAR_REMOTE_STATUS = 2,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_RESET_LOCAL_RECORDING = 3,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_UPDATE_LOCAL_PREEDIT = 4,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_PRESENT_REMOTE_STATUS = 5,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_ADOPT_AND_STOP_NORMAL = 6,
  VINPUT_FCITX_DAEMON_CONTROL_PLAN_CLEAR_DAEMON_ERROR = 7,
};

enum {
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_CLEAR = 0,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_PARTIAL = 1,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_RECORDING = 2,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_COMMANDING = 3,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_RECOGNIZING = 4,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_POSTPROCESSING = 5,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_INFO = 6,
  VINPUT_FCITX_DAEMON_SIGNAL_PLAN_NOTIFICATION_ERROR = 7,
};

enum {
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_NONE = 0,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_START_NORMAL = 1,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_STOP_NORMAL = 2,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_START_COMMAND = 3,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_STOP_COMMAND = 4,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_SHOW_SCENE_MENU = 5,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_SCENE_MENU_RELEASE = 6,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_SHOW_ASR_MENU = 7,
  VINPUT_FCITX_FRONTEND_TRIGGER_REQUEST_CONSUME_ASR_MENU_RELEASE = 8,
};

enum {
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_NONE = 0,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_START_NORMAL = 1,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_STOP_NORMAL = 2,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_START_COMMAND = 3,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_STOP_COMMAND = 4,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_SCENE_MENU = 5,
  VINPUT_FCITX_FRONTEND_TRIGGER_INTENT_SHOW_ASR_MENU = 6,
};

enum {
  VINPUT_FCITX_FRONTEND_OUTCOME_NONE = 0,
  VINPUT_FCITX_FRONTEND_OUTCOME_PREEDIT = 1,
  VINPUT_FCITX_FRONTEND_OUTCOME_CLEAR = 2,
  VINPUT_FCITX_FRONTEND_OUTCOME_COMMIT = 3,
  VINPUT_FCITX_FRONTEND_OUTCOME_CANDIDATE_MENU = 4,
  VINPUT_FCITX_FRONTEND_OUTCOME_ERROR = 5,
};

enum {
  VINPUT_FCITX_TRIGGER_MODE_TAP = 0,
  VINPUT_FCITX_TRIGGER_MODE_HOLD = 1,
  VINPUT_FCITX_TRIGGER_MODE_BOTH = 2,
};

enum {
  VINPUT_FCITX_TRIGGER_KIND_NORMAL = 0,
  VINPUT_FCITX_TRIGGER_KIND_COMMAND = 1,
};

enum {
  VINPUT_FCITX_TRIGGER_ACTION_NONE = 0,
  VINPUT_FCITX_TRIGGER_ACTION_CONSUME = 1,
  VINPUT_FCITX_TRIGGER_ACTION_START_NORMAL = 2,
  VINPUT_FCITX_TRIGGER_ACTION_START_COMMAND = 3,
  VINPUT_FCITX_TRIGGER_ACTION_STOP_ACTIVE = 4,
  VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_NORMAL_START = 5,
  VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_COMMAND_START = 6,
  VINPUT_FCITX_TRIGGER_ACTION_CANCEL_PENDING_START = 7,
  VINPUT_FCITX_TRIGGER_ACTION_SCHEDULE_STOP = 8,
};

enum {
  VINPUT_FCITX_TRIGGER_EVENT_SET_MODE = 0,
  VINPUT_FCITX_TRIGGER_EVENT_PRESS = 1,
  VINPUT_FCITX_TRIGGER_EVENT_RELEASE = 2,
  VINPUT_FCITX_TRIGGER_EVENT_FIRE_PENDING_START = 3,
  VINPUT_FCITX_TRIGGER_EVENT_FIRE_PENDING_STOP = 4,
  VINPUT_FCITX_TRIGGER_EVENT_CONFIRM_START = 5,
  VINPUT_FCITX_TRIGGER_EVENT_RECORDING_STOPPED = 6,
};

enum {
  VINPUT_FCITX_MENU_KEY_OTHER = 0,
  VINPUT_FCITX_MENU_KEY_PASSIVE = 1,
  VINPUT_FCITX_MENU_KEY_ESCAPE = 2,
  VINPUT_FCITX_MENU_KEY_SLASH = 3,
  VINPUT_FCITX_MENU_KEY_BACKSPACE = 4,
  VINPUT_FCITX_MENU_KEY_DELETE_WORD = 5,
  VINPUT_FCITX_MENU_KEY_CLEAR_FILTER = 6,
  VINPUT_FCITX_MENU_KEY_TEXT = 7,
  VINPUT_FCITX_MENU_KEY_PAGE = 8,
  VINPUT_FCITX_MENU_KEY_DIGIT = 9,
  VINPUT_FCITX_MENU_KEY_MOVE_PREVIOUS = 10,
  VINPUT_FCITX_MENU_KEY_MOVE_NEXT = 11,
  VINPUT_FCITX_MENU_KEY_ENTER = 12,
};

enum {
  VINPUT_FCITX_MENU_ACTION_PASS = 0,
  VINPUT_FCITX_MENU_ACTION_CONSUME = 1,
  VINPUT_FCITX_MENU_ACTION_CLOSE_AND_PASS = 2,
  VINPUT_FCITX_MENU_ACTION_CLOSE_AND_CONSUME = 3,
  VINPUT_FCITX_MENU_ACTION_REBUILD = 4,
  VINPUT_FCITX_MENU_ACTION_MOVE_PREVIOUS = 5,
  VINPUT_FCITX_MENU_ACTION_MOVE_NEXT = 6,
  VINPUT_FCITX_MENU_ACTION_SELECT = 7,
};

enum { VINPUT_FCITX_MENU_PAGE_SIZE = 10 };

VinputFcitxMenuSession *vinput_fcitx_menu_session_new(void);
void vinput_fcitx_menu_session_free(VinputFcitxMenuSession *session);
uint8_t vinput_fcitx_menu_session_open(VinputFcitxMenuSession *session);
uint8_t vinput_fcitx_menu_session_close(VinputFcitxMenuSession *session);
uint8_t vinput_fcitx_menu_session_is_open(
    const VinputFcitxMenuSession *session, uint8_t *open_out);
uint8_t vinput_fcitx_menu_session_set_page(
    VinputFcitxMenuSession *session, int32_t page);
uint8_t vinput_fcitx_menu_session_filter_active(
    const VinputFcitxMenuSession *session, uint8_t *active_out);
uint8_t vinput_fcitx_menu_session_decorate_title(
    VinputFcitxMenuSession *session, const uint8_t *base_data,
    size_t base_len, VinputFcitxStringView *title_out);
uint8_t vinput_fcitx_menu_session_handle_key(
    VinputFcitxMenuSession *session,
    const VinputFcitxMenuKeyInputView *input,
    VinputFcitxMenuKeyDecisionView *decision_out);
int32_t vinput_fcitx_clamp_menu_page(int32_t total_pages,
                                     int32_t requested_page);

VinputFcitxSceneMenuController *vinput_fcitx_scene_menu_controller_new(void);
void vinput_fcitx_scene_menu_controller_free(
    VinputFcitxSceneMenuController *controller);
VinputFcitxMenuProjection *vinput_fcitx_scene_menu_controller_projection_new(
    const VinputFcitxSceneMenuController *controller,
    const VinputFcitxMenuSession *session);
VinputFcitxAsrMenuController *vinput_fcitx_asr_menu_controller_new(void);
void vinput_fcitx_asr_menu_controller_free(
    VinputFcitxAsrMenuController *controller);
VinputFcitxMenuProjection *vinput_fcitx_asr_menu_controller_projection_new(
    const VinputFcitxAsrMenuController *controller,
    const VinputFcitxMenuSession *session,
    const VinputFcitxAsrMenuTextView *text);

void vinput_fcitx_menu_projection_free(VinputFcitxMenuProjection *projection);
uint8_t vinput_fcitx_menu_projection_view(
    const VinputFcitxMenuProjection *projection,
    VinputFcitxMenuProjectionView *view_out);
uint8_t vinput_fcitx_menu_projection_item_view(
    const VinputFcitxMenuProjection *projection, size_t index,
    VinputFcitxProjectedMenuItemView *view_out);

uint8_t vinput_fcitx_daemon_control_plan(
    const VinputFcitxDaemonControlView *control);
VinputFcitxDaemonLiveState *vinput_fcitx_daemon_live_state_new(void);
void vinput_fcitx_daemon_live_state_free(VinputFcitxDaemonLiveState *state);
uint8_t vinput_fcitx_daemon_live_state_reset(VinputFcitxDaemonLiveState *state);
uint8_t vinput_fcitx_daemon_live_state_begin_status(
    VinputFcitxDaemonLiveState *state, const uint8_t *status_data,
    size_t status_len, uint8_t command_mode);
uint8_t vinput_fcitx_daemon_live_state_update_status(
    VinputFcitxDaemonLiveState *state, const uint8_t *status_data,
    size_t status_len);
uint8_t vinput_fcitx_daemon_live_state_update_partial(
    VinputFcitxDaemonLiveState *state, const uint8_t *partial_data,
    size_t partial_len, uint8_t recording);
uint8_t vinput_fcitx_daemon_live_state_preedit_plan(
    const VinputFcitxDaemonLiveState *state,
    VinputFcitxDaemonSignalPlanView *view_out);
uint8_t vinput_fcitx_daemon_live_state_command_mode(
    const VinputFcitxDaemonLiveState *state);
uint8_t vinput_fcitx_daemon_status_preedit_plan(
    const VinputFcitxDaemonStatusView *status,
    VinputFcitxDaemonSignalPlanView *view_out);
uint8_t vinput_fcitx_daemon_notification_plan(
    const VinputFcitxDaemonNotificationView *notification,
    VinputFcitxDaemonSignalPlanView *view_out);

VinputFcitxDaemonClient *vinput_fcitx_daemon_client_connect(
    VinputFcitxOwnedString **error_out);
void vinput_fcitx_daemon_client_free(VinputFcitxDaemonClient *client);
VinputFcitxOwnedString *vinput_fcitx_daemon_client_get_status(
    const VinputFcitxDaemonClient *client,
    VinputFcitxOwnedString **error_out);
uint8_t vinput_fcitx_daemon_client_refresh_scene_menu_controller(
    const VinputFcitxDaemonClient *client,
    VinputFcitxSceneMenuController *controller,
    VinputFcitxOwnedString **error_out);
uint8_t vinput_fcitx_daemon_client_set_active_scene(
    const VinputFcitxDaemonClient *client,
    VinputFcitxSceneMenuController *controller,
    const uint8_t *scene_data, size_t scene_len, uint8_t *persisted_out,
    VinputFcitxOwnedString **error_out);
uint8_t vinput_fcitx_daemon_client_refresh_asr_menu_controller(
    const VinputFcitxDaemonClient *client,
    VinputFcitxAsrMenuController *controller,
    VinputFcitxOwnedString **error_out);
uint8_t vinput_fcitx_daemon_client_set_active_asr_target(
    const VinputFcitxDaemonClient *client,
    const VinputFcitxAsrTargetView *target, uint8_t *persisted_out,
    VinputFcitxOwnedString **error_out);
void vinput_fcitx_owned_string_free(VinputFcitxOwnedString *value);
uint8_t vinput_fcitx_owned_string_view(
    const VinputFcitxOwnedString *value, VinputFcitxStringView *view_out);

VinputFcitxTriggerState *vinput_fcitx_trigger_state_new(uint8_t mode);
void vinput_fcitx_trigger_state_free(VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_dispatch(
    VinputFcitxTriggerState *state, const VinputFcitxTriggerEventView *event,
    uint8_t *action_out);

VinputFcitxFrontendController *vinput_fcitx_frontend_controller_new(void);
void vinput_fcitx_frontend_controller_free(
    VinputFcitxFrontendController *controller);
uint8_t vinput_fcitx_frontend_controller_recording(
    const VinputFcitxFrontendController *controller);
uint8_t vinput_fcitx_frontend_controller_command_mode(
    const VinputFcitxFrontendController *controller);
uint8_t vinput_fcitx_frontend_controller_plan_trigger(
    const VinputFcitxFrontendController *controller, uint8_t request,
    uint8_t *intent_out);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_start_normal_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon,
    const VinputFcitxSceneMenuController *scene_controller);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_start_command_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon, const uint8_t *selected_data,
    size_t selected_len, const uint8_t *scene_data, size_t scene_len);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_stop_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon,
    const VinputFcitxSceneMenuController *scene_controller);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon, uint8_t command_mode,
    const VinputFcitxSceneMenuController *scene_controller);
uint8_t vinput_fcitx_frontend_controller_reset(
    VinputFcitxFrontendController *controller);

void vinput_fcitx_frontend_outcome_free(
    VinputFcitxFrontendOutcome *outcome);
VinputFcitxFrontendPresentation *vinput_fcitx_frontend_presentation_new(
    const VinputFcitxFrontendOutcome *outcome,
    const VinputFcitxFrontendPresentationTextView *text);
void vinput_fcitx_frontend_presentation_free(
    VinputFcitxFrontendPresentation *presentation);
uint8_t vinput_fcitx_frontend_presentation_view(
    const VinputFcitxFrontendPresentation *presentation,
    VinputFcitxFrontendPresentationView *view_out);
uint8_t vinput_fcitx_frontend_presentation_candidate(
    const VinputFcitxFrontendPresentation *presentation, size_t index,
    VinputFcitxPresentedCandidateView *view_out);

#ifdef __cplusplus
}
#endif
