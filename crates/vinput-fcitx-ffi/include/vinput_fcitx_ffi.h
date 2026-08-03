#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VinputFcitxAsrProjection VinputFcitxAsrProjection;
typedef struct VinputFcitxAsrDisplaySnapshot VinputFcitxAsrDisplaySnapshot;
typedef struct VinputFcitxDaemonClient VinputFcitxDaemonClient;
typedef struct VinputFcitxDaemonResponse VinputFcitxDaemonResponse;
typedef struct VinputFcitxFrontendController VinputFcitxFrontendController;
typedef struct VinputFcitxFrontendOutcome VinputFcitxFrontendOutcome;
typedef struct VinputFcitxMenuFilterState VinputFcitxMenuFilterState;
typedef struct VinputFcitxSceneProjection VinputFcitxSceneProjection;
typedef struct VinputFcitxSceneSnapshot VinputFcitxSceneSnapshot;
typedef struct VinputFcitxTriggerState VinputFcitxTriggerState;

enum {
  VINPUT_FCITX_CANDIDATE_SOURCE_RAW = 0,
  VINPUT_FCITX_CANDIDATE_SOURCE_LLM = 1,
  VINPUT_FCITX_CANDIDATE_SOURCE_ASR = 2,
  VINPUT_FCITX_CANDIDATE_SOURCE_CANCEL = 3,
};

typedef struct VinputFcitxStringView {
  const uint8_t *data;
  size_t len;
} VinputFcitxStringView;

typedef struct VinputFcitxFrontendOutcomeView {
  uint8_t kind;
  uint8_t command_mode;
  VinputFcitxStringView text;
  VinputFcitxStringView commit_text;
  size_t candidate_count;
} VinputFcitxFrontendOutcomeView;

typedef struct VinputFcitxCandidateView {
  VinputFcitxStringView text;
  uint8_t source;
} VinputFcitxCandidateView;

typedef struct VinputFcitxMenuFilterView {
  uint8_t active;
  VinputFcitxStringView query;
} VinputFcitxMenuFilterView;

typedef struct VinputFcitxMenuKeyDecisionView {
  uint8_t action;
  int64_t value;
} VinputFcitxMenuKeyDecisionView;

typedef struct VinputFcitxSceneSnapshotView {
  VinputFcitxStringView active_scene_id;
  VinputFcitxStringView active_label;
  size_t item_count;
} VinputFcitxSceneSnapshotView;

typedef struct VinputFcitxSceneSnapshotItemView {
  VinputFcitxStringView id;
  VinputFcitxStringView label;
} VinputFcitxSceneSnapshotItemView;

typedef struct VinputFcitxAsrDisplaySnapshotView {
  VinputFcitxStringView target_provider_id;
  VinputFcitxStringView target_model_id;
  VinputFcitxStringView effective_provider_id;
  VinputFcitxStringView effective_model_id;
  VinputFcitxStringView last_error;
  VinputFcitxStringView effective_base_label;
  VinputFcitxStringView target_base_label;
  uint8_t reload_in_progress;
  size_t item_count;
} VinputFcitxAsrDisplaySnapshotView;

typedef struct VinputFcitxAsrDisplaySnapshotItemView {
  VinputFcitxStringView provider_id;
  VinputFcitxStringView kind;
  VinputFcitxStringView item_id;
  VinputFcitxStringView display_title;
  VinputFcitxStringView model_value;
  VinputFcitxStringView base_label;
  uint8_t is_loading;
} VinputFcitxAsrDisplaySnapshotItemView;

typedef struct VinputFcitxProjectionView {
  VinputFcitxStringView effective_label;
  size_t item_count;
} VinputFcitxProjectionView;

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

typedef struct VinputFcitxSceneProjectionView {
  VinputFcitxStringView active_label;
  size_t item_count;
} VinputFcitxSceneProjectionView;

typedef struct VinputFcitxTriggerEventView {
  uint8_t kind;
  uint8_t value;
  uint8_t flag;
  int64_t now_ns;
} VinputFcitxTriggerEventView;

typedef struct VinputFcitxTriggerStateView {
  uint8_t mode;
  uint8_t has_pending_start;
  uint8_t has_active_trigger;
} VinputFcitxTriggerStateView;

typedef struct VinputFcitxDaemonResponseView {
  uint8_t kind;
  uint8_t bool_value;
  VinputFcitxStringView text;
} VinputFcitxDaemonResponseView;

typedef struct VinputFcitxDaemonSignalPlanView {
  uint8_t kind;
  uint8_t translate;
  VinputFcitxStringView text;
} VinputFcitxDaemonSignalPlanView;

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
  VINPUT_FCITX_DAEMON_RESPONSE_ERROR = 0,
  VINPUT_FCITX_DAEMON_RESPONSE_NONE = 1,
  VINPUT_FCITX_DAEMON_RESPONSE_TEXT = 2,
  VINPUT_FCITX_DAEMON_RESPONSE_BOOL = 3,
  VINPUT_FCITX_DAEMON_RESPONSE_SCENE_SNAPSHOT = 4,
  VINPUT_FCITX_DAEMON_RESPONSE_ASR_DISPLAY_SNAPSHOT = 5,
};

enum {
  VINPUT_FCITX_DAEMON_OPERATION_START_RECORDING = 0,
  VINPUT_FCITX_DAEMON_OPERATION_START_COMMAND_RECORDING = 1,
  VINPUT_FCITX_DAEMON_OPERATION_STOP_RECORDING = 2,
  VINPUT_FCITX_DAEMON_OPERATION_GET_STATUS = 3,
  VINPUT_FCITX_DAEMON_OPERATION_GET_SCENE_STATE = 4,
  VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_SCENE = 5,
  VINPUT_FCITX_DAEMON_OPERATION_GET_ASR_DISPLAY_MENU_STATE = 6,
  VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_ASR_PROVIDER = 7,
  VINPUT_FCITX_DAEMON_OPERATION_SET_ACTIVE_ASR_TARGET = 8,
  VINPUT_FCITX_DAEMON_OPERATION_GET_TEXT_ADAPTER_STATE = 9,
  VINPUT_FCITX_DAEMON_OPERATION_START_ADAPTER = 10,
  VINPUT_FCITX_DAEMON_OPERATION_STOP_ADAPTER = 11,
  VINPUT_FCITX_DAEMON_OPERATION_GET_RUNTIME_STATUS = 12,
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

VinputFcitxMenuFilterState *vinput_fcitx_menu_filter_state_new(void);
void vinput_fcitx_menu_filter_state_free(VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_reset(
    VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_view(
    const VinputFcitxMenuFilterState *state,
    VinputFcitxMenuFilterView *view_out);
uint8_t vinput_fcitx_menu_filter_state_decorate_title(
    VinputFcitxMenuFilterState *state, const uint8_t *base_data,
    size_t base_len, VinputFcitxStringView *title_out);
uint8_t vinput_fcitx_menu_filter_state_handle_key(
    VinputFcitxMenuFilterState *state, uint8_t release, uint8_t key_kind,
    int64_t key_value, const uint8_t *text_data, size_t text_len,
    uint8_t cursor_available, int64_t current_selection,
    int32_t current_page, size_t visible_item_count,
    VinputFcitxMenuKeyDecisionView *decision_out);
int32_t vinput_fcitx_clamp_menu_page(int32_t total_pages,
                                     int32_t requested_page);

VinputFcitxSceneSnapshot *vinput_fcitx_scene_snapshot_new(
    const uint8_t *active_scene_data, size_t active_scene_len);
void vinput_fcitx_scene_snapshot_free(VinputFcitxSceneSnapshot *snapshot);
uint8_t vinput_fcitx_scene_snapshot_add(
    VinputFcitxSceneSnapshot *snapshot, const uint8_t *id_data,
    size_t id_len, const uint8_t *label_data, size_t label_len);
uint8_t vinput_fcitx_scene_snapshot_set_active(
    VinputFcitxSceneSnapshot *snapshot, const uint8_t *active_scene_data,
    size_t active_scene_len);
uint8_t vinput_fcitx_scene_snapshot_view(
    const VinputFcitxSceneSnapshot *snapshot,
    VinputFcitxSceneSnapshotView *view_out);
uint8_t vinput_fcitx_scene_snapshot_item_view(
    const VinputFcitxSceneSnapshot *snapshot, size_t index,
    VinputFcitxSceneSnapshotItemView *view_out);

VinputFcitxAsrDisplaySnapshot *vinput_fcitx_asr_display_snapshot_new(
    const uint8_t *target_provider_data, size_t target_provider_len,
    const uint8_t *target_model_data, size_t target_model_len,
    const uint8_t *effective_provider_data, size_t effective_provider_len,
    const uint8_t *effective_model_data, size_t effective_model_len,
    uint8_t reload_in_progress, const uint8_t *last_error_data,
    size_t last_error_len);
void vinput_fcitx_asr_display_snapshot_free(
    VinputFcitxAsrDisplaySnapshot *snapshot);
uint8_t vinput_fcitx_asr_display_snapshot_add(
    VinputFcitxAsrDisplaySnapshot *snapshot, const uint8_t *provider_data,
    size_t provider_len, const uint8_t *kind_data, size_t kind_len,
    const uint8_t *item_id_data, size_t item_id_len,
    const uint8_t *display_title_data, size_t display_title_len,
    const uint8_t *model_value_data, size_t model_value_len);
uint8_t vinput_fcitx_asr_display_snapshot_view(
    const VinputFcitxAsrDisplaySnapshot *snapshot,
    VinputFcitxAsrDisplaySnapshotView *view_out);
uint8_t vinput_fcitx_asr_display_snapshot_item_view(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index,
    VinputFcitxAsrDisplaySnapshotItemView *view_out);

VinputFcitxAsrProjection *vinput_fcitx_asr_projection_new(
    const VinputFcitxAsrDisplaySnapshot *snapshot,
    const uint8_t *query_data, size_t query_len,
    const uint8_t *local_data, size_t local_len,
    const uint8_t *remote_data, size_t remote_len,
    const uint8_t *command_data, size_t command_len,
    const uint8_t *loading_suffix_data, size_t loading_suffix_len,
    const uint8_t *unavailable_data, size_t unavailable_len,
    const uint8_t *loading_prefix_data, size_t loading_prefix_len,
    const uint8_t *error_prefix_data, size_t error_prefix_len);
void vinput_fcitx_asr_projection_free(VinputFcitxAsrProjection *projection);
uint8_t vinput_fcitx_asr_projection_view(
    const VinputFcitxAsrProjection *projection,
    VinputFcitxProjectionView *view_out);
uint8_t vinput_fcitx_asr_projection_item_view(
    const VinputFcitxAsrProjection *projection, size_t index,
    VinputFcitxProjectedMenuItemView *view_out);

VinputFcitxSceneProjection *vinput_fcitx_scene_projection_new(
    const VinputFcitxSceneSnapshot *snapshot, const uint8_t *query_data,
    size_t query_len);
void vinput_fcitx_scene_projection_free(VinputFcitxSceneProjection *projection);
uint8_t vinput_fcitx_scene_projection_view(
    const VinputFcitxSceneProjection *projection,
    VinputFcitxSceneProjectionView *view_out);
uint8_t vinput_fcitx_scene_projection_item_view(
    const VinputFcitxSceneProjection *projection, size_t index,
    VinputFcitxProjectedMenuItemView *view_out);

uint8_t vinput_fcitx_daemon_control_plan(
    uint8_t event, const uint8_t *status_data, size_t status_len,
    uint8_t flag, uint8_t recording, uint8_t remote_status_active);
uint8_t vinput_fcitx_daemon_status_preedit_plan(
    const uint8_t *status_data, size_t status_len, uint8_t command_mode,
    const uint8_t *partial_data, size_t partial_len,
    VinputFcitxDaemonSignalPlanView *view_out);
uint8_t vinput_fcitx_daemon_notification_plan(
    const uint8_t *code_data, size_t code_len,
    const uint8_t *subject_data, size_t subject_len,
    const uint8_t *detail_data, size_t detail_len,
    const uint8_t *raw_data, size_t raw_len,
    VinputFcitxDaemonSignalPlanView *view_out);

VinputFcitxDaemonClient *vinput_fcitx_daemon_client_connect(
    VinputFcitxDaemonResponse **error_out);
void vinput_fcitx_daemon_client_free(VinputFcitxDaemonClient *client);
VinputFcitxDaemonResponse *vinput_fcitx_daemon_client_call(
    const VinputFcitxDaemonClient *client, uint8_t operation,
    const uint8_t *first_data, size_t first_len,
    const uint8_t *second_data, size_t second_len);
void vinput_fcitx_daemon_response_free(VinputFcitxDaemonResponse *response);
uint8_t vinput_fcitx_daemon_response_view(
    const VinputFcitxDaemonResponse *response,
    VinputFcitxDaemonResponseView *view_out);
VinputFcitxSceneSnapshot *vinput_fcitx_daemon_response_take_scene_snapshot(
    VinputFcitxDaemonResponse *response);
VinputFcitxAsrDisplaySnapshot *
vinput_fcitx_daemon_response_take_asr_display_snapshot(
    VinputFcitxDaemonResponse *response);

VinputFcitxTriggerState *vinput_fcitx_trigger_state_new(uint8_t mode);
void vinput_fcitx_trigger_state_free(VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_dispatch(
    VinputFcitxTriggerState *state, const VinputFcitxTriggerEventView *event,
    uint8_t *action_out);
uint8_t vinput_fcitx_trigger_state_view(
    const VinputFcitxTriggerState *state,
    VinputFcitxTriggerStateView *view_out);

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
    const VinputFcitxDaemonClient *daemon, const uint8_t *scene_data,
    size_t scene_len);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_start_command_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon, const uint8_t *selected_data,
    size_t selected_len, const uint8_t *scene_data, size_t scene_len);
VinputFcitxFrontendOutcome *vinput_fcitx_frontend_controller_stop_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon,
    const uint8_t *fallback_scene_data, size_t fallback_scene_len);
VinputFcitxFrontendOutcome *
vinput_fcitx_frontend_controller_adopt_and_stop_with_daemon(
    VinputFcitxFrontendController *controller,
    const VinputFcitxDaemonClient *daemon, uint8_t command_mode,
    const uint8_t *scene_data, size_t scene_len);
uint8_t vinput_fcitx_frontend_controller_reset(
    VinputFcitxFrontendController *controller);

VinputFcitxFrontendOutcome *vinput_fcitx_frontend_outcome_from_payload(
    const uint8_t *json_data, size_t json_len, uint8_t command_mode);
void vinput_fcitx_frontend_outcome_free(
    VinputFcitxFrontendOutcome *outcome);
uint8_t vinput_fcitx_frontend_outcome_view(
    const VinputFcitxFrontendOutcome *outcome,
    VinputFcitxFrontendOutcomeView *view_out);
uint8_t vinput_fcitx_frontend_outcome_candidate(
    const VinputFcitxFrontendOutcome *outcome, size_t index,
    VinputFcitxCandidateView *view_out);

#ifdef __cplusplus
}
#endif
