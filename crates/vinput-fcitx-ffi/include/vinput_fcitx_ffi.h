#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VinputFcitxAsrProjection VinputFcitxAsrProjection;
typedef struct VinputFcitxAsrDisplaySnapshot VinputFcitxAsrDisplaySnapshot;
typedef struct VinputFcitxCommitPlan VinputFcitxCommitPlan;
typedef struct VinputFcitxFrontendState VinputFcitxFrontendState;
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
uint8_t vinput_fcitx_menu_filter_state_activate(
    VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_clear_and_deactivate(
    VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_backspace(
    VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_delete_last_word(
    VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_append_text(
    VinputFcitxMenuFilterState *state, const uint8_t *text_data,
    size_t text_len);
uint8_t vinput_fcitx_menu_filter_state_handle_key(
    VinputFcitxMenuFilterState *state, uint8_t release, uint8_t key_kind,
    int64_t key_value, const uint8_t *text_data, size_t text_len,
    uint8_t cursor_available, int64_t current_selection,
    int32_t current_page, size_t visible_item_count, uint8_t *action_out,
    int64_t *value_out);
uint8_t vinput_fcitx_menu_filter_state_active(
    const VinputFcitxMenuFilterState *state);
const uint8_t *vinput_fcitx_menu_filter_state_query_data(
    const VinputFcitxMenuFilterState *state);
size_t vinput_fcitx_menu_filter_state_query_len(
    const VinputFcitxMenuFilterState *state);
uint8_t vinput_fcitx_menu_filter_state_matches(
    const VinputFcitxMenuFilterState *state, const uint8_t *search_data,
    size_t search_len);
uint8_t vinput_fcitx_menu_filter_state_decorate_title(
    VinputFcitxMenuFilterState *state, const uint8_t *base_data,
    size_t base_len);
const uint8_t *vinput_fcitx_menu_filter_state_decorated_title_data(
    const VinputFcitxMenuFilterState *state);
size_t vinput_fcitx_menu_filter_state_decorated_title_len(
    const VinputFcitxMenuFilterState *state);
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
const uint8_t *vinput_fcitx_scene_snapshot_active_id_data(
    const VinputFcitxSceneSnapshot *snapshot);
size_t vinput_fcitx_scene_snapshot_active_id_len(
    const VinputFcitxSceneSnapshot *snapshot);
const uint8_t *vinput_fcitx_scene_snapshot_active_label_data(
    const VinputFcitxSceneSnapshot *snapshot);
size_t vinput_fcitx_scene_snapshot_active_label_len(
    const VinputFcitxSceneSnapshot *snapshot);
size_t vinput_fcitx_scene_snapshot_item_count(
    const VinputFcitxSceneSnapshot *snapshot);
const uint8_t *vinput_fcitx_scene_snapshot_item_id_data(
    const VinputFcitxSceneSnapshot *snapshot, size_t index);
size_t vinput_fcitx_scene_snapshot_item_id_len(
    const VinputFcitxSceneSnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_scene_snapshot_item_label_data(
    const VinputFcitxSceneSnapshot *snapshot, size_t index);
size_t vinput_fcitx_scene_snapshot_item_label_len(
    const VinputFcitxSceneSnapshot *snapshot, size_t index);

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
const uint8_t *vinput_fcitx_asr_display_snapshot_target_provider_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_target_provider_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_target_model_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_target_model_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_effective_provider_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_effective_provider_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_effective_model_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_effective_model_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_last_error_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_last_error_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_effective_base_label_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_effective_base_label_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_target_base_label_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_target_base_label_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
uint8_t vinput_fcitx_asr_display_snapshot_reload_in_progress(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
size_t vinput_fcitx_asr_display_snapshot_item_count(
    const VinputFcitxAsrDisplaySnapshot *snapshot);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_provider_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_provider_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_kind_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_kind_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_id_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_id_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_display_title_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_display_title_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_model_value_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_model_value_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
const uint8_t *vinput_fcitx_asr_display_snapshot_item_base_label_data(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
size_t vinput_fcitx_asr_display_snapshot_item_base_label_len(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);
uint8_t vinput_fcitx_asr_display_snapshot_item_is_loading(
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t index);

VinputFcitxAsrProjection *vinput_fcitx_asr_projection_new(
    const uint8_t *target_provider_data, size_t target_provider_len,
    const uint8_t *target_model_data, size_t target_model_len,
    const uint8_t *effective_provider_data, size_t effective_provider_len,
    const uint8_t *effective_model_data, size_t effective_model_len,
    uint8_t reload_in_progress, const uint8_t *last_error_data,
    size_t last_error_len, const uint8_t *query_data, size_t query_len);
VinputFcitxAsrProjection *vinput_fcitx_asr_projection_new_from_snapshot(
    const VinputFcitxAsrDisplaySnapshot *snapshot,
    const uint8_t *query_data, size_t query_len);
void vinput_fcitx_asr_projection_free(VinputFcitxAsrProjection *projection);
uint8_t vinput_fcitx_asr_projection_add(
    VinputFcitxAsrProjection *projection, size_t source_index,
    const uint8_t *provider_data, size_t provider_len,
    const uint8_t *kind_data, size_t kind_len, const uint8_t *item_id_data,
    size_t item_id_len, const uint8_t *display_title_data,
    size_t display_title_len, const uint8_t *model_value_data,
    size_t model_value_len, const uint8_t *rendered_label_data,
    size_t rendered_label_len);
uint8_t vinput_fcitx_asr_projection_add_snapshot_item(
    VinputFcitxAsrProjection *projection,
    const VinputFcitxAsrDisplaySnapshot *snapshot, size_t source_index,
    const uint8_t *rendered_label_data, size_t rendered_label_len);
uint8_t vinput_fcitx_asr_projection_finish(
    VinputFcitxAsrProjection *projection);
size_t vinput_fcitx_asr_projection_item_count(
    const VinputFcitxAsrProjection *projection);
size_t vinput_fcitx_asr_projection_item_source_index(
    const VinputFcitxAsrProjection *projection, size_t index);
const uint8_t *vinput_fcitx_asr_projection_item_label_data(
    const VinputFcitxAsrProjection *projection, size_t index);
size_t vinput_fcitx_asr_projection_item_label_len(
    const VinputFcitxAsrProjection *projection, size_t index);

VinputFcitxSceneProjection *vinput_fcitx_scene_projection_new(
    const uint8_t *active_scene_data, size_t active_scene_len,
    const uint8_t *query_data, size_t query_len);
VinputFcitxSceneProjection *vinput_fcitx_scene_projection_from_snapshot(
    const VinputFcitxSceneSnapshot *snapshot, const uint8_t *query_data,
    size_t query_len);
void vinput_fcitx_scene_projection_free(VinputFcitxSceneProjection *projection);
uint8_t vinput_fcitx_scene_projection_add(
    VinputFcitxSceneProjection *projection, size_t source_index,
    const uint8_t *id_data, size_t id_len, const uint8_t *label_data,
    size_t label_len);
uint8_t vinput_fcitx_scene_projection_finish(
    VinputFcitxSceneProjection *projection);
const uint8_t *vinput_fcitx_scene_projection_active_label_data(
    const VinputFcitxSceneProjection *projection);
size_t vinput_fcitx_scene_projection_active_label_len(
    const VinputFcitxSceneProjection *projection);
size_t vinput_fcitx_scene_projection_item_count(
    const VinputFcitxSceneProjection *projection);
size_t vinput_fcitx_scene_projection_item_source_index(
    const VinputFcitxSceneProjection *projection, size_t index);
const uint8_t *vinput_fcitx_scene_projection_item_label_data(
    const VinputFcitxSceneProjection *projection, size_t index);
size_t vinput_fcitx_scene_projection_item_label_len(
    const VinputFcitxSceneProjection *projection, size_t index);

VinputFcitxTriggerState *vinput_fcitx_trigger_state_new(uint8_t mode);
void vinput_fcitx_trigger_state_free(VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_set_mode(VinputFcitxTriggerState *state,
                                             uint8_t mode);
uint8_t vinput_fcitx_trigger_state_mode(const VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_on_press(VinputFcitxTriggerState *state,
                                            uint8_t kind, int64_t now_ns,
                                            uint8_t recording);
uint8_t vinput_fcitx_trigger_state_on_release(VinputFcitxTriggerState *state,
                                              int64_t now_ns,
                                              uint8_t active_release);
uint8_t vinput_fcitx_trigger_state_fire_pending_start(
    VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_fire_pending_stop(
    VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_confirm_start(
    VinputFcitxTriggerState *state, uint8_t recording_started);
uint8_t vinput_fcitx_trigger_state_recording_stopped(
    VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_has_pending_start(
    const VinputFcitxTriggerState *state);
uint8_t vinput_fcitx_trigger_state_has_active_trigger(
    const VinputFcitxTriggerState *state);

VinputFcitxFrontendState *vinput_fcitx_frontend_state_new(void);
void vinput_fcitx_frontend_state_free(VinputFcitxFrontendState *state);
uint8_t vinput_fcitx_frontend_state_recording(
    const VinputFcitxFrontendState *state);
uint8_t vinput_fcitx_frontend_state_command_mode(
    const VinputFcitxFrontendState *state);
uint8_t vinput_fcitx_frontend_state_has_active_scene(
    const VinputFcitxFrontendState *state);
const uint8_t *vinput_fcitx_frontend_state_active_scene_data(
    const VinputFcitxFrontendState *state);
size_t vinput_fcitx_frontend_state_active_scene_len(
    const VinputFcitxFrontendState *state);
uint8_t vinput_fcitx_frontend_state_start_normal(
    VinputFcitxFrontendState *state, const uint8_t *scene_data,
    size_t scene_len, uint8_t has_scene);
uint8_t vinput_fcitx_frontend_state_start_command(
    VinputFcitxFrontendState *state, const uint8_t *scene_data,
    size_t scene_len, uint8_t has_scene);
uint8_t vinput_fcitx_frontend_state_adopt(
    VinputFcitxFrontendState *state, uint8_t command_mode,
    const uint8_t *scene_data, size_t scene_len);
uint8_t vinput_fcitx_frontend_state_reset(VinputFcitxFrontendState *state);

VinputFcitxCommitPlan *vinput_fcitx_commit_plan_new(
    const uint8_t *json_data, size_t json_len, uint8_t command_mode);
void vinput_fcitx_commit_plan_free(VinputFcitxCommitPlan *plan);

uint8_t vinput_fcitx_commit_plan_show_candidate_menu(
    const VinputFcitxCommitPlan *plan);
const uint8_t *vinput_fcitx_commit_plan_text_data(
    const VinputFcitxCommitPlan *plan);
size_t vinput_fcitx_commit_plan_text_len(const VinputFcitxCommitPlan *plan);

size_t vinput_fcitx_commit_plan_candidate_count(
    const VinputFcitxCommitPlan *plan);
const uint8_t *vinput_fcitx_commit_plan_candidate_text_data(
    const VinputFcitxCommitPlan *plan, size_t index);
size_t vinput_fcitx_commit_plan_candidate_text_len(
    const VinputFcitxCommitPlan *plan, size_t index);
uint8_t vinput_fcitx_commit_plan_candidate_source(
    const VinputFcitxCommitPlan *plan, size_t index);

#ifdef __cplusplus
}
#endif
