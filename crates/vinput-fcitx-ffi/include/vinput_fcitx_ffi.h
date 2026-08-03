#pragma once

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct VinputFcitxCommitPlan VinputFcitxCommitPlan;
typedef struct VinputFcitxFrontendState VinputFcitxFrontendState;
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
