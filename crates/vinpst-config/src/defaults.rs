use crate::RAW_SCENE_ID;

pub(crate) fn default_language() -> String {
    "zh".to_owned()
}

pub(crate) const fn default_scene_candidate_count() -> u8 {
    1
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
