use crate::{COMMAND_SCENE_ID, RAW_SCENE_ID, SceneDefinition};

pub(crate) fn ensure_builtin_scenes(definitions: &mut Vec<SceneDefinition>) {
    if !definitions.iter().any(|scene| scene.id == RAW_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: RAW_SCENE_ID.to_owned(),
            label: "__label_raw__".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 0,
            timeout_ms: None,
            context_lines: 0,
        });
    }
    if !definitions.iter().any(|scene| scene.id == COMMAND_SCENE_ID) {
        definitions.push(SceneDefinition {
            id: COMMAND_SCENE_ID.to_owned(),
            label: "__label_command__".to_owned(),
            prompt: None,
            provider_id: None,
            model: None,
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    }
}

pub(crate) fn default_language() -> String {
    "zh".to_owned()
}

pub(crate) fn default_capture_device() -> String {
    "default".to_owned()
}

pub(crate) const fn default_duck_output_volume() -> f32 {
    0.25
}

pub(crate) fn default_asr_provider() -> String {
    "sherpa-onnx".to_owned()
}

pub(crate) fn default_active_scene() -> String {
    RAW_SCENE_ID.to_owned()
}

pub(crate) const fn default_true() -> bool {
    true
}

pub(crate) const fn default_input_gain() -> f32 {
    1.0
}

pub(crate) const fn default_vad_threshold() -> f32 {
    0.45
}

pub(crate) const fn default_vad_min_speech_duration() -> f32 {
    0.15
}

pub(crate) const fn default_vad_min_silence_duration() -> f32 {
    0.5
}

pub(crate) const fn default_vad_speech_pad_ms() -> u32 {
    300
}

pub(crate) fn default_json_object() -> serde_json::Value {
    serde_json::json!({})
}
