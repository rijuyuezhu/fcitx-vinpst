//! Frozen-upstream daemon error classification for user notifications.

/// Structured notification payload carried by the legacy `ssss` D-Bus signal.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct ErrorInfo {
    pub(crate) code: &'static str,
    pub(crate) subject: String,
    pub(crate) detail: String,
    pub(crate) raw_message: String,
}

pub(crate) const ERROR_CODE_UNKNOWN: &str = "unknown";
pub(crate) const ERROR_CODE_DAEMON_BUSY: &str = "daemon_busy";
pub(crate) const ERROR_CODE_ASR_BACKEND_LOADING: &str = "asr_backend_loading";
pub(crate) const ERROR_CODE_ASR_BACKEND_RELOAD_FAILED: &str = "asr_backend_reload_failed";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_CHECK_FAILED: &str = "local_asr_model_check_failed";
pub(crate) const ERROR_CODE_LOCAL_ASR_PROVIDER_INIT_FAILED: &str = "local_asr_provider_init_failed";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_CONFIG_MISSING: &str = "local_asr_model_config_missing";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_TYPE_MISSING: &str = "local_asr_model_type_missing";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_INVALID_PATH: &str = "local_asr_model_invalid_path";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_TOKENS_MISSING: &str = "local_asr_model_tokens_missing";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_FILES_MISSING: &str = "local_asr_model_files_missing";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_ROOT_RESOLVE_FAILED: &str =
    "local_asr_model_root_resolve_failed";
pub(crate) const ERROR_CODE_LOCAL_ASR_MODEL_PARSE_FAILED: &str = "local_asr_model_parse_failed";
pub(crate) const ERROR_CODE_LOCAL_ASR_UNSUPPORTED_MODEL_TYPE: &str =
    "local_asr_unsupported_model_type";
pub(crate) const ERROR_CODE_LOCAL_ASR_RECOGNIZER_CREATE_FAILED: &str =
    "local_asr_recognizer_create_failed";
pub(crate) const ERROR_CODE_VAD_CREATE_FAILED: &str = "vad_create_failed";
pub(crate) const ERROR_CODE_AUDIO_CAPTURE_LOOP_NOT_INITIALIZED: &str =
    "audio_capture_loop_not_initialized";
pub(crate) const ERROR_CODE_PIPEWIRE_THREAD_LOOP_CREATE_FAILED: &str =
    "pipewire_thread_loop_create_failed";
pub(crate) const ERROR_CODE_PIPEWIRE_THREAD_LOOP_START_FAILED: &str =
    "pipewire_thread_loop_start_failed";
pub(crate) const ERROR_CODE_PIPEWIRE_PROPERTIES_ALLOC_FAILED: &str =
    "pipewire_properties_alloc_failed";
pub(crate) const ERROR_CODE_PIPEWIRE_STREAM_CREATE_FAILED: &str = "pipewire_stream_create_failed";
pub(crate) const ERROR_CODE_PIPEWIRE_STREAM_CONNECT_FAILED: &str = "pipewire_stream_connect_failed";
pub(crate) const ERROR_CODE_DBUS_EVENTFD_CREATE_FAILED: &str = "dbus_eventfd_create_failed";
pub(crate) const ERROR_CODE_DBUS_USER_BUS_OPEN_FAILED: &str = "dbus_user_bus_open_failed";
pub(crate) const ERROR_CODE_DBUS_VTABLE_ADD_FAILED: &str = "dbus_vtable_add_failed";
pub(crate) const ERROR_CODE_DBUS_NAME_REQUEST_FAILED: &str = "dbus_name_request_failed";
pub(crate) const ERROR_CODE_START_RECORDING_FAILED: &str = "start_recording_failed";
pub(crate) const ERROR_CODE_START_COMMAND_RECORDING_FAILED: &str = "start_command_recording_failed";
pub(crate) const ERROR_CODE_ASR_PROVIDER_START_FAILED: &str = "asr_provider_start_failed";
pub(crate) const ERROR_CODE_ASR_PROVIDER_TIMEOUT: &str = "asr_provider_timeout";
pub(crate) const ERROR_CODE_ASR_PROVIDER_FAILED: &str = "asr_provider_failed";
pub(crate) const ERROR_CODE_ASR_PROVIDER_NO_TEXT: &str = "asr_provider_no_text";
pub(crate) const ERROR_CODE_LLM_REQUEST_FAILED: &str = "llm_request_failed";
pub(crate) const ERROR_CODE_LLM_HTTP_FAILED: &str = "llm_http_failed";
pub(crate) const ERROR_CODE_PROCESSING_UNKNOWN: &str = "processing_unknown";
pub(crate) const ERROR_CODE_PROMPT_FILE_LOAD_FAILED: &str = "prompt_file_load_failed";

#[must_use]
pub(crate) fn make_error_info(
    code: &'static str,
    subject: impl Into<String>,
    detail: impl Into<String>,
    raw_message: impl Into<String>,
) -> ErrorInfo {
    ErrorInfo {
        code,
        subject: subject.into(),
        detail: detail.into(),
        raw_message: raw_message.into(),
    }
}

#[must_use]
pub(crate) fn make_raw_error(raw_message: impl Into<String>) -> ErrorInfo {
    make_error_info(ERROR_CODE_UNKNOWN, "", "", raw_message)
}

#[must_use]
pub(crate) fn classify_error_text(message: &str) -> ErrorInfo {
    let normalized = trim_ascii_whitespace(message);
    if normalized.is_empty() {
        return ErrorInfo::default();
    }
    if let Some(text) = normalized
        .strip_prefix("vinput-daemon: ")
        .or_else(|| normalized.strip_prefix("vinput: "))
    {
        return classify_error_text(text);
    }
    classify_known_detail(normalized)
}

fn classify_known_detail(text: &str) -> ErrorInfo {
    let original = text.to_owned();
    if let Some(rest) = text.strip_prefix("ASR provider error: ") {
        return classify_error_text(rest);
    }
    if let Some(rest) = text.strip_prefix("worker exception: ") {
        return make_raw_error(rest);
    }
    if let Some(info) = classify_provider_or_start(text, &original) {
        return info;
    }
    if let Some(info) = classify_daemon_or_reload(text, &original) {
        return info;
    }
    let normalized = trim_ascii_whitespace(text);
    if let Some(info) = classify_model_error(normalized, &original) {
        return info;
    }
    if let Some(info) = classify_runtime_error(normalized, &original) {
        return info;
    }
    if let Some(info) = classify_processing_error(normalized, &original) {
        return info;
    }
    make_raw_error(original)
}

fn classify_provider_or_start(text: &str, original: &str) -> Option<ErrorInfo> {
    if let Some((provider, tail)) = parse_quoted_value(text, "ASR provider ")
        && let Some(detail) = tail.strip_prefix(": ")
        && let Some(info) = classify_provider_detail(provider, detail, original)
    {
        return Some(info);
    }
    for (prefix, code) in [
        (
            "Local ASR model check failed for provider ",
            ERROR_CODE_LOCAL_ASR_MODEL_CHECK_FAILED,
        ),
        (
            "Failed to initialize local ASR provider ",
            ERROR_CODE_LOCAL_ASR_PROVIDER_INIT_FAILED,
        ),
    ] {
        if let Some((provider, tail)) = parse_quoted_value(text, prefix) {
            return Some(if let Some(detail) = tail.strip_prefix(": ") {
                adopt_nested(classify_error_text(detail), text, provider)
            } else {
                make_error_info(code, provider, "", original)
            });
        }
    }
    classify_recording_start(text, original)
}

fn classify_recording_start(text: &str, original: &str) -> Option<ErrorInfo> {
    for (prefix, exact, code) in [
        (
            "Failed to start recording: ",
            "Failed to start recording.",
            ERROR_CODE_START_RECORDING_FAILED,
        ),
        (
            "Failed to start command recording: ",
            "Failed to start command recording.",
            ERROR_CODE_START_COMMAND_RECORDING_FAILED,
        ),
    ] {
        if let Some(detail) = text.strip_prefix(prefix) {
            let mut nested = classify_error_text(detail);
            if nested.code != ERROR_CODE_UNKNOWN {
                if nested.raw_message.is_empty() {
                    original.clone_into(&mut nested.raw_message);
                }
                return Some(nested);
            }
            return Some(make_error_info(
                code,
                "",
                trim_ascii_whitespace(detail),
                original,
            ));
        }
        if text == exact {
            return Some(make_error_info(code, "", "", original));
        }
    }
    None
}

fn classify_daemon_or_reload(text: &str, original: &str) -> Option<ErrorInfo> {
    if text == "Daemon is busy." {
        return Some(make_error_info(ERROR_CODE_DAEMON_BUSY, "", "", original));
    }
    if text == "ASR backend is still loading." {
        return Some(make_error_info(
            ERROR_CODE_ASR_BACKEND_LOADING,
            "",
            "",
            original,
        ));
    }
    for prefix in [
        "Failed to apply ASR backend reload.",
        "Failed to reload ASR backend.",
    ] {
        if let Some(detail) = text.strip_prefix(prefix) {
            return Some(make_error_info(
                ERROR_CODE_ASR_BACKEND_RELOAD_FAILED,
                "",
                trim_ascii_whitespace(detail),
                original,
            ));
        }
    }
    if let Some(detail) = text.strip_prefix("missing 'vinput-model.json' in ") {
        return Some(make_error_info(
            ERROR_CODE_LOCAL_ASR_MODEL_CONFIG_MISSING,
            "",
            trim_ascii_whitespace(detail),
            original,
        ));
    }
    None
}

fn classify_model_error(text: &str, original: &str) -> Option<ErrorInfo> {
    if text == "Local ASR model configuration is missing." {
        return Some(make_error_info(
            ERROR_CODE_LOCAL_ASR_MODEL_CONFIG_MISSING,
            "",
            "",
            original,
        ));
    }
    if let Some((provider, tail)) = parse_quoted_value(
        text,
        "Local ASR model configuration is missing for provider ",
    ) && tail == "."
    {
        return Some(make_error_info(
            ERROR_CODE_LOCAL_ASR_MODEL_CONFIG_MISSING,
            provider,
            "",
            original,
        ));
    }
    if let Some((subject, tail)) = parse_quoted_value(text, "")
        && let Some(model) = tail.strip_prefix(" is missing family for model '")
        && let Some(model) = model.strip_suffix('\'')
    {
        return Some(make_error_info(
            ERROR_CODE_LOCAL_ASR_MODEL_TYPE_MISSING,
            subject,
            model,
            original,
        ));
    }
    if let Some((subject, tail)) = parse_quoted_value(text, "")
        && let Some(detail) = tail.strip_prefix(" contains invalid path for '")
    {
        return Some(make_error_info(
            ERROR_CODE_LOCAL_ASR_MODEL_INVALID_PATH,
            subject,
            trim_ascii_whitespace(detail),
            original,
        ));
    }
    classify_model_asset_error(text, original)
}

fn classify_model_asset_error(text: &str, original: &str) -> Option<ErrorInfo> {
    for (prefix, code) in [
        (
            "tokens file not found for model ",
            ERROR_CODE_LOCAL_ASR_MODEL_TOKENS_MISSING,
        ),
        (
            "no model files found for model ",
            ERROR_CODE_LOCAL_ASR_MODEL_FILES_MISSING,
        ),
        (
            "unsupported model family ",
            ERROR_CODE_LOCAL_ASR_UNSUPPORTED_MODEL_TYPE,
        ),
        (
            "failed to create sherpa-onnx recognizer for family ",
            ERROR_CODE_LOCAL_ASR_RECOGNIZER_CREATE_FAILED,
        ),
        ("failed to create VAD from ", ERROR_CODE_VAD_CREATE_FAILED),
    ] {
        if let Some((subject, _)) = parse_quoted_value(text, prefix) {
            return Some(make_error_info(code, subject, "", original));
        }
    }
    for (prefix, code) in [
        (
            "failed to resolve model root ",
            ERROR_CODE_LOCAL_ASR_MODEL_ROOT_RESOLVE_FAILED,
        ),
        ("failed to parse ", ERROR_CODE_LOCAL_ASR_MODEL_PARSE_FAILED),
    ] {
        if let Some((subject, tail)) = parse_quoted_value(text, prefix)
            && let Some(detail) = tail.strip_prefix(": ")
        {
            return Some(make_error_info(
                code,
                subject,
                trim_ascii_whitespace(detail),
                original,
            ));
        }
    }
    None
}

fn classify_runtime_error(text: &str, original: &str) -> Option<ErrorInfo> {
    for (exact, code) in [
        (
            "audio capture loop is not initialized",
            ERROR_CODE_AUDIO_CAPTURE_LOOP_NOT_INITIALIZED,
        ),
        (
            "failed to create PipeWire thread loop",
            ERROR_CODE_PIPEWIRE_THREAD_LOOP_CREATE_FAILED,
        ),
        (
            "failed to allocate PipeWire properties",
            ERROR_CODE_PIPEWIRE_PROPERTIES_ALLOC_FAILED,
        ),
        (
            "failed to create PipeWire stream",
            ERROR_CODE_PIPEWIRE_STREAM_CREATE_FAILED,
        ),
    ] {
        if text == exact {
            return Some(make_error_info(code, "", "", original));
        }
    }
    for (prefix, code) in [
        (
            "failed to start PipeWire thread loop: ",
            ERROR_CODE_PIPEWIRE_THREAD_LOOP_START_FAILED,
        ),
        (
            "failed to connect PipeWire stream: ",
            ERROR_CODE_PIPEWIRE_STREAM_CONNECT_FAILED,
        ),
        (
            "failed to create eventfd: ",
            ERROR_CODE_DBUS_EVENTFD_CREATE_FAILED,
        ),
        (
            "failed to open user bus: ",
            ERROR_CODE_DBUS_USER_BUS_OPEN_FAILED,
        ),
        (
            "failed to add D-Bus vtable: ",
            ERROR_CODE_DBUS_VTABLE_ADD_FAILED,
        ),
        (
            "failed to request D-Bus name: ",
            ERROR_CODE_DBUS_NAME_REQUEST_FAILED,
        ),
    ] {
        if let Some(detail) = text.strip_prefix(prefix) {
            return Some(make_error_info(
                code,
                "",
                trim_ascii_whitespace(detail),
                original,
            ));
        }
    }
    None
}

fn classify_processing_error(text: &str, original: &str) -> Option<ErrorInfo> {
    if let Some(detail) = text.strip_prefix("LLM request failed: ") {
        return Some(make_error_info(
            ERROR_CODE_LLM_REQUEST_FAILED,
            "",
            trim_ascii_whitespace(detail),
            original,
        ));
    }
    if text.starts_with("HTTP ") {
        return Some(make_error_info(
            ERROR_CODE_LLM_HTTP_FAILED,
            "",
            trim_ascii_whitespace(text),
            original,
        ));
    }
    if let Some(body) = text.strip_prefix("Prompt file load failed: ") {
        let body = trim_ascii_whitespace(body);
        return Some(
            if let Some((subject, detail)) = parse_prompt_load_body(body) {
                make_error_info(
                    ERROR_CODE_PROMPT_FILE_LOAD_FAILED,
                    subject,
                    detail,
                    original,
                )
            } else {
                make_error_info(ERROR_CODE_PROMPT_FILE_LOAD_FAILED, "", body, original)
            },
        );
    }
    (text == "Unknown error during processing")
        .then(|| make_error_info(ERROR_CODE_PROCESSING_UNKNOWN, "", "", original))
}

fn classify_provider_detail(provider: &str, detail: &str, original: &str) -> Option<ErrorInfo> {
    for (exact, prefix, code) in [
        (
            "failed to start.",
            "failed to start. ",
            ERROR_CODE_ASR_PROVIDER_START_FAILED,
        ),
        ("timed out.", "timed out. ", ERROR_CODE_ASR_PROVIDER_TIMEOUT),
        ("failed.", "failed. ", ERROR_CODE_ASR_PROVIDER_FAILED),
    ] {
        if detail == exact {
            return Some(make_error_info(code, provider, "", original));
        }
        if let Some(rest) = detail.strip_prefix(prefix) {
            return Some(make_error_info(
                code,
                provider,
                trim_ascii_whitespace(rest),
                original,
            ));
        }
    }
    (detail == "returned no text.")
        .then(|| make_error_info(ERROR_CODE_ASR_PROVIDER_NO_TEXT, provider, "", original))
}

fn adopt_nested(mut nested: ErrorInfo, raw_message: &str, subject: &str) -> ErrorInfo {
    if nested.raw_message.is_empty() {
        raw_message.clone_into(&mut nested.raw_message);
    }
    if nested.subject.is_empty() && !subject.is_empty() {
        subject.clone_into(&mut nested.subject);
    }
    nested
}

fn parse_quoted_value<'a>(text: &'a str, prefix: &str) -> Option<(&'a str, &'a str)> {
    let text = text.strip_prefix(prefix)?;
    let text = text.strip_prefix('\'')?;
    let quote = text.find('\'')?;
    Some((&text[..quote], &text[quote + 1..]))
}

fn parse_prompt_load_body(body: &str) -> Option<(&str, &str)> {
    body.strip_prefix("file:///")?;
    let separator = body.find(char::is_whitespace)?;
    let uri = body[..separator].strip_suffix(':')?;
    let detail = trim_ascii_whitespace(&body[separator..]);
    (!detail.is_empty()).then_some((uri, detail))
}

fn trim_ascii_whitespace(text: &str) -> &str {
    text.trim_matches(|character: char| character.is_ascii_whitespace())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifier_handles_prefixes_provider_failures_and_unknown_errors() {
        let cases = [
            (
                " vinput-daemon: ASR provider 'cmd': failed to start. spawn denied ",
                ERROR_CODE_ASR_PROVIDER_START_FAILED,
                "cmd",
                "spawn denied",
            ),
            (
                "ASR provider 'cmd': timed out.",
                ERROR_CODE_ASR_PROVIDER_TIMEOUT,
                "cmd",
                "",
            ),
            (
                "ASR provider 'cmd': failed. stderr detail",
                ERROR_CODE_ASR_PROVIDER_FAILED,
                "cmd",
                "stderr detail",
            ),
            (
                "ASR provider 'cmd': returned no text.",
                ERROR_CODE_ASR_PROVIDER_NO_TEXT,
                "cmd",
                "",
            ),
            ("unmapped failure", ERROR_CODE_UNKNOWN, "", ""),
        ];
        for (message, code, subject, detail) in cases {
            let info = classify_error_text(message);
            assert_eq!(info.code, code, "message {message}");
            assert_eq!(info.subject, subject, "message {message}");
            assert_eq!(info.detail, detail, "message {message}");
        }
    }

    #[test]
    fn classifier_covers_frozen_model_audio_dbus_and_processing_codes() {
        let cases = [
            (
                "Local ASR model configuration is missing.",
                ERROR_CODE_LOCAL_ASR_MODEL_CONFIG_MISSING,
            ),
            (
                "tokens file not found for model 'm'",
                ERROR_CODE_LOCAL_ASR_MODEL_TOKENS_MISSING,
            ),
            (
                "no model files found for model 'm'",
                ERROR_CODE_LOCAL_ASR_MODEL_FILES_MISSING,
            ),
            (
                "unsupported model family 'whisper'",
                ERROR_CODE_LOCAL_ASR_UNSUPPORTED_MODEL_TYPE,
            ),
            (
                "failed to create sherpa-onnx recognizer for family 'whisper'",
                ERROR_CODE_LOCAL_ASR_RECOGNIZER_CREATE_FAILED,
            ),
            (
                "failed to create VAD from '/tmp/vad.onnx'",
                ERROR_CODE_VAD_CREATE_FAILED,
            ),
            (
                "audio capture loop is not initialized",
                ERROR_CODE_AUDIO_CAPTURE_LOOP_NOT_INITIALIZED,
            ),
            (
                "failed to create PipeWire thread loop",
                ERROR_CODE_PIPEWIRE_THREAD_LOOP_CREATE_FAILED,
            ),
            (
                "failed to start PipeWire thread loop: denied",
                ERROR_CODE_PIPEWIRE_THREAD_LOOP_START_FAILED,
            ),
            (
                "failed to allocate PipeWire properties",
                ERROR_CODE_PIPEWIRE_PROPERTIES_ALLOC_FAILED,
            ),
            (
                "failed to create PipeWire stream",
                ERROR_CODE_PIPEWIRE_STREAM_CREATE_FAILED,
            ),
            (
                "failed to connect PipeWire stream: -32",
                ERROR_CODE_PIPEWIRE_STREAM_CONNECT_FAILED,
            ),
            (
                "failed to create eventfd: EMFILE",
                ERROR_CODE_DBUS_EVENTFD_CREATE_FAILED,
            ),
            (
                "failed to open user bus: denied",
                ERROR_CODE_DBUS_USER_BUS_OPEN_FAILED,
            ),
            (
                "failed to add D-Bus vtable: invalid",
                ERROR_CODE_DBUS_VTABLE_ADD_FAILED,
            ),
            (
                "failed to request D-Bus name: exists",
                ERROR_CODE_DBUS_NAME_REQUEST_FAILED,
            ),
            ("LLM request failed: offline", ERROR_CODE_LLM_REQUEST_FAILED),
            ("HTTP 429 rate limited", ERROR_CODE_LLM_HTTP_FAILED),
            (
                "Unknown error during processing",
                ERROR_CODE_PROCESSING_UNKNOWN,
            ),
        ];
        for (message, code) in cases {
            assert_eq!(classify_error_text(message).code, code, "message {message}");
        }
    }

    #[test]
    fn classifier_preserves_nested_subject_and_raw_message() {
        let message =
            "Local ASR model check failed for provider 'local': unsupported model family 'whisper'";
        let info = classify_error_text(message);
        assert_eq!(info.code, ERROR_CODE_LOCAL_ASR_UNSUPPORTED_MODEL_TYPE);
        assert_eq!(info.subject, "whisper");
        assert_eq!(info.raw_message, "unsupported model family 'whisper'");

        let message = "Failed to initialize local ASR provider 'local': failed to create VAD from '/tmp/vad.onnx'";
        let info = classify_error_text(message);
        assert_eq!(info.code, ERROR_CODE_VAD_CREATE_FAILED);
        assert_eq!(info.subject, "/tmp/vad.onnx");
    }

    #[test]
    fn classifier_covers_start_reload_and_model_detail_shapes() {
        let cases = [
            (
                "Failed to start recording: ASR provider 'cmd': timed out. waiting",
                ERROR_CODE_ASR_PROVIDER_TIMEOUT,
                "cmd",
                "waiting",
            ),
            (
                "Failed to start command recording: unexpected helper failure",
                ERROR_CODE_START_COMMAND_RECORDING_FAILED,
                "",
                "unexpected helper failure",
            ),
            (
                "Failed to reload ASR backend. prepare failed",
                ERROR_CODE_ASR_BACKEND_RELOAD_FAILED,
                "",
                "prepare failed",
            ),
            (
                "'provider' is missing family for model 'model.demo'",
                ERROR_CODE_LOCAL_ASR_MODEL_TYPE_MISSING,
                "provider",
                "model.demo",
            ),
            (
                "failed to resolve model root '/models': permission denied",
                ERROR_CODE_LOCAL_ASR_MODEL_ROOT_RESOLVE_FAILED,
                "/models",
                "permission denied",
            ),
            (
                "failed to parse '/models/vinput-model.json': bad json",
                ERROR_CODE_LOCAL_ASR_MODEL_PARSE_FAILED,
                "/models/vinput-model.json",
                "bad json",
            ),
        ];
        for (message, code, subject, detail) in cases {
            let info = classify_error_text(message);
            assert_eq!(info.code, code, "message {message}");
            assert_eq!(info.subject, subject, "message {message}");
            assert_eq!(info.detail, detail, "message {message}");
        }
    }

    #[test]
    fn classifier_parses_prompt_uri_without_splitting_reason_colons() {
        let info = classify_error_text(
            "Prompt file load failed: file:///tmp/prompt.txt: No such file: errno 2",
        );
        assert_eq!(info.code, ERROR_CODE_PROMPT_FILE_LOAD_FAILED);
        assert_eq!(info.subject, "file:///tmp/prompt.txt");
        assert_eq!(info.detail, "No such file: errno 2");
    }

    #[test]
    fn blank_message_is_empty() {
        assert_eq!(classify_error_text(" \t\n"), ErrorInfo::default());
    }
}
