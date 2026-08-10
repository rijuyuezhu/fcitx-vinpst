//! OpenAI-compatible text adapter request building and processor seams.

use std::{
    fmt,
    path::{Path, PathBuf},
    time::Duration,
};

use vinpst_config::{
    COMMAND_SCENE_ID, LlmProviderConfig, RAW_SCENE_ID, SceneDefinition, redact_url_for_diagnostics,
};
use vinpst_http::{
    HttpClientError, MAX_PROVIDER_RESPONSE_BYTES, ResponseBodyError,
    blocking_client_from_environment_with_connect_timeout, read_provider_response_text,
    reqwest_error_category,
};
use vinpst_protocol::{Candidate, CandidateSource, RecognitionPayload};

use crate::payload::{normal_mode_payload, trim_ascii_whitespace};
use crate::prompt::{
    build_constraints_suffix, render_legacy_prompt_placeholders_with_context, wrap_xml_block,
};
use crate::{
    PromptContext, has_legacy_prompt_interpolation, is_prompt_file_uri, load_prompt_file_uri,
};
use crate::{
    TextAdapter, TextError, TextProcessReport, TextProcessor, TextRequest, command_mode_payload,
    load_recent_input_context_prefix, scene_needs_postprocessing,
};

const OPENAI_COMPATIBLE_CHAT_COMPLETIONS_PATH: &str = "/chat/completions";
const OPENAI_COMPATIBLE_MODELS_PATH: &str = "/models";
const OPENAI_COMPATIBLE_MODEL_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(10);
const OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER: (&str, &str) =
    ("Content-Type", "application/json");
const OPENAI_COMPATIBLE_AUTHORIZATION_HEADER: &str = "Authorization";
const OPENAI_COMPATIBLE_BEARER_PREFIX: &str = "Bearer ";

/// Builds the legacy OpenAI-compatible chat-completions endpoint URL.
///
/// Empty base URLs are not requestable. If the base URL already ends with
/// `/chat/completions`, it is preserved verbatim; otherwise trailing slashes are
/// removed before appending exactly one path separator.
#[must_use]
pub fn build_openai_compatible_chat_url(base_url: &str) -> Option<String> {
    build_openai_compatible_endpoint_url(base_url, OPENAI_COMPATIBLE_CHAT_COMPLETIONS_PATH)
}

/// Builds the OpenAI-compatible model-list endpoint URL used by the management UI.
#[must_use]
pub fn build_openai_compatible_models_url(base_url: &str) -> Option<String> {
    build_openai_compatible_endpoint_url(base_url, OPENAI_COMPATIBLE_MODELS_PATH)
}

fn build_openai_compatible_endpoint_url(base_url: &str, suffix: &str) -> Option<String> {
    if base_url.is_empty() {
        return None;
    }
    if let Ok(mut url) = reqwest::Url::parse(base_url) {
        let path = url.path().trim_end_matches('/').to_owned();
        if path.ends_with(suffix) {
            if url.path() != path {
                url.set_path(&path);
            }
        } else {
            url.set_path(&format!("{path}{suffix}"));
        }
        return Some(url.to_string());
    }
    if base_url.ends_with(suffix) {
        return Some(base_url.to_owned());
    }
    let mut url = base_url.to_owned();
    while url.ends_with('/') {
        url.pop();
    }
    url.push_str(suffix);
    Some(url)
}

/// Stable, secret-free failures from OpenAI-compatible model discovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenAiModelDiscoveryError {
    /// No requestable base URL was configured.
    #[error("provider base URL is empty")]
    MissingBaseUrl,
    /// The shared provider client could not be constructed.
    #[error("HTTP client setup failed")]
    Client(#[from] HttpClientError),
    /// The GET request failed before a response was available.
    #[error("HTTP request {0}")]
    Request(&'static str),
    /// The endpoint returned a non-success status.
    #[error("HTTP endpoint returned status {0}")]
    Status(u16),
    /// The bounded response body could not be read.
    #[error("HTTP response body failed: {0}")]
    Body(#[from] ResponseBodyError),
    /// The response was not valid JSON.
    #[error("HTTP response is not valid JSON")]
    InvalidJson,
}

/// Discovers OpenAI-compatible model ids from one provider.
pub fn discover_openai_compatible_model_ids(
    base_url: &str,
    api_key: &str,
) -> Result<Vec<String>, OpenAiModelDiscoveryError> {
    let url = build_openai_compatible_models_url(base_url)
        .ok_or(OpenAiModelDiscoveryError::MissingBaseUrl)?;
    let client = blocking_client_from_environment_with_connect_timeout(
        OPENAI_COMPATIBLE_MODEL_DISCOVERY_TIMEOUT,
    )?;
    let mut request = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(OPENAI_COMPATIBLE_MODEL_DISCOVERY_TIMEOUT);
    if !api_key.is_empty() {
        request = request.bearer_auth(api_key);
    }
    let response = request
        .send()
        .map_err(|error| OpenAiModelDiscoveryError::Request(reqwest_error_category(&error)))?;
    if !response.status().is_success() {
        return Err(OpenAiModelDiscoveryError::Status(
            response.status().as_u16(),
        ));
    }
    let body = read_provider_response_text(response)?;
    parse_openai_compatible_model_ids(&body)
}

fn parse_openai_compatible_model_ids(body: &str) -> Result<Vec<String>, OpenAiModelDiscoveryError> {
    let document = serde_json::from_str::<serde_json::Value>(body)
        .map_err(|_| OpenAiModelDiscoveryError::InvalidJson)?;
    let mut models = document
        .get("data")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("id").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>();
    models.sort_unstable();
    models.dedup();
    Ok(models)
}

/// Builds the legacy OpenAI-compatible request headers.
///
/// The API key string is used as configured. Legacy CLI/GUI paths trim the value
/// while editing config, but the daemon request path only checks whether it is
/// empty before appending the `Authorization: Bearer ...` header.
#[must_use]
pub fn build_openai_compatible_headers(api_key: &str) -> Vec<(String, String)> {
    let mut headers = vec![(
        OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER.0.to_owned(),
        OPENAI_COMPATIBLE_JSON_CONTENT_TYPE_HEADER.1.to_owned(),
    )];
    if !api_key.is_empty() {
        headers.push((
            OPENAI_COMPATIBLE_AUTHORIZATION_HEADER.to_owned(),
            format!("{OPENAI_COMPATIBLE_BEARER_PREFIX}{api_key}"),
        ));
    }
    headers
}

/// Extracts candidate strings from the legacy OpenAI-compatible chat response shape.
///
/// The legacy post-processor asks providers to return a chat-completions response
/// whose first choice message content is itself a JSON object containing a
/// `candidates` string array. Invalid or unexpected shapes return an empty list.
#[must_use]
pub fn extract_openai_compatible_candidates(response_body: &str) -> Vec<String> {
    let Ok(response) = serde_json::from_str::<serde_json::Value>(response_body) else {
        return Vec::new();
    };
    let Some(content) = response
        .get("choices")
        .and_then(serde_json::Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(serde_json::Value::as_str)
    else {
        return Vec::new();
    };
    let Ok(content) = serde_json::from_str::<serde_json::Value>(content) else {
        return Vec::new();
    };

    content
        .get("candidates")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_str)
        .map(|candidate| {
            candidate.trim_matches(|character| matches!(character, ' ' | '\t' | '\r' | '\n'))
        })
        .filter(|candidate| !candidate.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

/// Converts OpenAI-compatible candidate strings into the daemon payload shape.
///
/// The first LLM candidate becomes the default committed text, matching the
/// legacy recognition payload normalization rule. Empty candidate lists return
/// `None` so callers can fall back to raw ASR/command candidates.
#[must_use]
pub fn openai_compatible_candidates_to_payload(
    candidates: impl IntoIterator<Item = String>,
) -> Option<RecognitionPayload> {
    let candidates = candidates
        .into_iter()
        .map(|candidate| Candidate::new(candidate, CandidateSource::Llm))
        .collect::<Vec<_>>();
    let commit_text = candidates.first()?.text.clone();
    Some(RecognitionPayload {
        commit_text,
        candidates,
    })
}

/// Parses an OpenAI-compatible chat response into a daemon recognition payload.
///
/// Invalid response shapes or empty candidate lists return `None` so callers can
/// fall back to raw ASR or command-mode fallback candidates.
#[must_use]
pub fn openai_compatible_response_to_payload(response_body: &str) -> Option<RecognitionPayload> {
    openai_compatible_candidates_to_payload(extract_openai_compatible_candidates(response_body))
}

const OPENAI_COMPATIBLE_PROTECTED_EXTRA_BODY_KEYS: &[&str] =
    &["messages", "stream", "response_format"];

/// Merges provider-specific OpenAI-compatible request fields into a request body.
///
/// The legacy daemon lets user config pass through provider-specific top-level
/// fields, but refuses `messages`, `stream`, and `response_format` because they
/// are required for the non-streaming JSON-candidates response contract. The
/// returned list contains ignored protected keys in input iteration order so
/// callers can log diagnostics without exposing secret values.
pub fn merge_openai_compatible_extra_body(
    request: &mut serde_json::Value,
    extra_body: &serde_json::Value,
) -> Vec<String> {
    let Some(request) = request.as_object_mut() else {
        return Vec::new();
    };
    let Some(extra_body) = extra_body.as_object() else {
        return Vec::new();
    };

    let mut ignored = Vec::new();
    for (key, value) in extra_body {
        if OPENAI_COMPATIBLE_PROTECTED_EXTRA_BODY_KEYS.contains(&key.as_str()) {
            ignored.push(key.clone());
            continue;
        }
        request.insert(key.clone(), value.clone());
    }
    ignored
}

/// OpenAI-compatible chat-completions request body built from a scene prompt.
#[derive(Clone, PartialEq)]
pub struct OpenAiCompatibleChatRequest {
    /// Fully resolved chat-completions endpoint URL.
    pub url: String,
    /// Request headers for the chat-completions request.
    pub headers: Vec<(String, String)>,
    /// JSON body for a non-streaming chat-completions request.
    pub body: serde_json::Value,
    /// Protected `extra_body` keys that were ignored while building the body.
    pub ignored_extra_body_keys: Vec<String>,
}

impl OpenAiCompatibleChatRequest {
    /// Returns the request URL with userinfo, fragment, and query values redacted.
    #[must_use]
    pub fn redacted_url(&self) -> String {
        redact_url_for_diagnostics(&self.url)
    }

    /// Returns request headers with secrets redacted for logs or diagnostics.
    #[must_use]
    pub fn redacted_headers(&self) -> Vec<(String, String)> {
        redact_openai_compatible_headers(&self.headers)
    }
}

impl fmt::Debug for OpenAiCompatibleChatRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let body_keys = self
            .body
            .as_object()
            .map(|body| body.keys().map(String::as_str).collect::<Vec<_>>())
            .unwrap_or_default();
        formatter
            .debug_struct("OpenAiCompatibleChatRequest")
            .field("url", &self.redacted_url())
            .field("headers", &self.redacted_headers())
            .field("body_keys", &body_keys)
            .field("ignored_extra_body_keys", &self.ignored_extra_body_keys)
            .finish()
    }
}

fn redact_openai_compatible_headers(headers: &[(String, String)]) -> Vec<(String, String)> {
    headers
        .iter()
        .map(|(name, value)| {
            let value = if name.eq_ignore_ascii_case(OPENAI_COMPATIBLE_AUTHORIZATION_HEADER) {
                "<redacted>".to_owned()
            } else {
                value.clone()
            };
            (name.clone(), value)
        })
        .collect()
}

/// Builds the legacy OpenAI-compatible non-streaming request body.
///
/// This helper only pins request assembly: prompt-file resolution,
/// legacy `{{asr}}`/`{{selected}}`/`{{context}}` interpolation, XML fallback,
/// candidate constraints, JSON-object response format, and protected
/// `extra_body` handling.
pub fn build_openai_compatible_chat_request(
    request: &TextRequest<'_>,
    provider: &LlmProviderConfig,
    context_prefix: &str,
) -> Result<Option<OpenAiCompatibleChatRequest>, TextError> {
    let Some(url) = build_openai_compatible_chat_url(&provider.base_url) else {
        return Ok(None);
    };
    let headers = build_openai_compatible_headers(&provider.api_key);
    let prompt = request.scene.prompt.as_deref().unwrap_or_default();
    if prompt.is_empty() && request.scene.id != COMMAND_SCENE_ID {
        return Ok(None);
    }
    let base_prompt = if is_prompt_file_uri(prompt) {
        load_prompt_file_uri(prompt)?
    } else {
        prompt.to_owned()
    };
    let prompt_context = PromptContext::from_request(request);
    let mut user_content = if has_legacy_prompt_interpolation(&base_prompt) {
        render_legacy_prompt_placeholders_with_context(
            &base_prompt,
            &prompt_context,
            context_prefix,
        )
    } else {
        let mut content = base_prompt;
        if !content.is_empty() && !content.ends_with('\n') {
            content.push_str("\n\n");
        } else if !content.is_empty() {
            content.push('\n');
        }
        if request.scene.id == COMMAND_SCENE_ID {
            if !request.raw_text.is_empty() {
                content.push_str(&wrap_xml_block("vinput-asr", request.raw_text));
                content.push_str("\n\n");
            }
            if let Some(selected_text) = request.selected_text.filter(|text| !text.is_empty()) {
                content.push_str(&wrap_xml_block("vinput-selected", selected_text));
                content.push_str("\n\n");
            }
            if !context_prefix.is_empty() {
                content.push_str(&wrap_xml_block("vinput-context", context_prefix));
                content.push('\n');
            }
        } else {
            if !context_prefix.is_empty() {
                content.push_str(&wrap_xml_block("vinput-context", context_prefix));
                content.push('\n');
            }
            if !request.raw_text.is_empty() {
                content.push_str(&wrap_xml_block("vinput-asr", request.raw_text));
                content.push('\n');
            }
        }
        content
    };
    user_content.push_str(&build_constraints_suffix(request.scene.candidate_count));

    let model = request
        .scene
        .model
        .as_deref()
        .or(provider.model.as_deref())
        .unwrap_or_default();
    let mut body = serde_json::json!({
        "model": model,
        "stream": false,
        "temperature": 0.2,
        "response_format": {"type": "json_object"},
        "messages": [
            {
                "role": "user",
                "content": user_content,
            }
        ],
    });
    let ignored_extra_body_keys =
        merge_openai_compatible_extra_body(&mut body, &provider.extra_body);

    Ok(Some(OpenAiCompatibleChatRequest {
        url,
        headers,
        body,
        ignored_extra_body_keys,
    }))
}

/// Builds an OpenAI-compatible request using a recent-input context cache file.
///
/// The cache is read according to `request.scene.context_lines`; missing cache
/// files produce an empty context prefix. This keeps filesystem policy out of
/// HTTP transport while matching the legacy prompt assembly path.
pub fn build_openai_compatible_chat_request_from_context_cache(
    request: &TextRequest<'_>,
    provider: &LlmProviderConfig,
    context_cache_path: impl AsRef<Path>,
) -> Result<Option<OpenAiCompatibleChatRequest>, TextError> {
    let context_prefix =
        load_recent_input_context_prefix(context_cache_path, request.scene.context_lines);
    build_openai_compatible_chat_request(request, provider, &context_prefix)
}

/// Transport seam for OpenAI-compatible chat-completions providers.
pub trait OpenAiCompatibleChatTransport: Send + Sync {
    /// Sends a fully built request and returns the raw response body.
    fn send(
        &self,
        request: &OpenAiCompatibleChatRequest,
        timeout_ms: Option<u64>,
    ) -> Result<String, TextError>;
}

/// Blocking HTTP transport for OpenAI-compatible chat-completions providers.
///
/// The transport sends the already-built request body and headers as-is, applies
/// the optional per-scene timeout to the request, and returns successful response
/// bodies for the existing candidate parser. Non-success responses are reported
/// by status only; provider error bodies are never surfaced in diagnostics.
///
/// The reqwest blocking client is created and dropped inside a dedicated thread
/// so daemon code can call this synchronous seam from a Tokio runtime without
/// dropping reqwest's internal blocking runtime in an async context.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestOpenAiCompatibleChatTransport;

impl ReqwestOpenAiCompatibleChatTransport {
    /// Creates a transport with reqwest's default blocking client settings.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl OpenAiCompatibleChatTransport for ReqwestOpenAiCompatibleChatTransport {
    fn send(
        &self,
        request: &OpenAiCompatibleChatRequest,
        timeout_ms: Option<u64>,
    ) -> Result<String, TextError> {
        let request = request.clone();
        std::thread::spawn(move || send_openai_compatible_request_blocking(&request, timeout_ms))
            .join()
            .map_err(|_| {
                TextError::AdapterFailed("OpenAI-compatible HTTP worker thread panicked".to_owned())
            })?
    }
}

fn send_openai_compatible_request_blocking(
    request: &OpenAiCompatibleChatRequest,
    timeout_ms: Option<u64>,
) -> Result<String, TextError> {
    let client = blocking_client_from_environment_with_connect_timeout(Duration::from_secs(5))
        .map_err(|error| {
            TextError::AdapterFailed(format!(
                "OpenAI-compatible HTTP client setup failed: {error}"
            ))
        })?;
    let mut builder = client.post(&request.url).json(&request.body);
    for (name, value) in &request.headers {
        builder = builder.header(name, value);
    }
    if let Some(timeout_ms) = timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(timeout_ms));
    }

    let diagnostic_url = request.redacted_url();
    let response = builder.send().map_err(|error| {
        if error.is_timeout() {
            TextError::AdapterFailed("OpenAI-compatible HTTP request timed out".to_owned())
        } else {
            TextError::AdapterFailed(format!(
                "OpenAI-compatible HTTP request failed for `{diagnostic_url}`: {}",
                reqwest_error_category(&error)
            ))
        }
    })?;
    let status = response.status();
    if !status.is_success() {
        return Err(TextError::AdapterFailed(format!(
            "OpenAI-compatible provider returned HTTP {status}"
        )));
    }
    let body = read_provider_response_text(response).map_err(|error| match error {
        ResponseBodyError::TimedOut => {
            TextError::AdapterFailed("OpenAI-compatible HTTP response body timed out".to_owned())
        }
        ResponseBodyError::TooLarge => TextError::AdapterFailed(format!(
            "OpenAI-compatible HTTP response body exceeds {MAX_PROVIDER_RESPONSE_BYTES}-byte limit"
        )),
        ResponseBodyError::InvalidUtf8 => TextError::AdapterFailed(
            "OpenAI-compatible HTTP response body is not valid UTF-8".to_owned(),
        ),
        ResponseBodyError::Read => TextError::AdapterFailed(format!(
            "OpenAI-compatible HTTP response body read failed for `{diagnostic_url}`: {error}"
        )),
    })?;
    Ok(body)
}

/// Text adapter backed by an OpenAI-compatible chat transport.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTextAdapter<T> {
    provider: LlmProviderConfig,
    transport: T,
    context_cache_path: Option<PathBuf>,
}

impl<T> OpenAiCompatibleTextAdapter<T> {
    /// Creates an adapter without recent-input context cache wiring.
    #[must_use]
    pub fn new(provider: LlmProviderConfig, transport: T) -> Self {
        Self {
            provider,
            transport,
            context_cache_path: None,
        }
    }

    /// Adds a recent-input context cache path used by scenes with context lines.
    #[must_use]
    pub fn with_context_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.context_cache_path = Some(path.into());
        self
    }
}

impl<T: OpenAiCompatibleChatTransport> TextAdapter for OpenAiCompatibleTextAdapter<T> {
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id != COMMAND_SCENE_ID
            && trim_ascii_whitespace(request.raw_text).is_empty()
        {
            return Ok(normal_mode_payload(request.raw_text, Vec::<String>::new()));
        }
        if request.scene.id == COMMAND_SCENE_ID
            && (trim_ascii_whitespace(request.raw_text).is_empty()
                || request.selected_text.unwrap_or_default().is_empty())
        {
            return Ok(pre_request_fallback_payload(request));
        }

        let built = if let Some(context_cache_path) = &self.context_cache_path {
            build_openai_compatible_chat_request_from_context_cache(
                request,
                &self.provider,
                context_cache_path,
            )?
        } else {
            build_openai_compatible_chat_request(request, &self.provider, "")?
        };
        let Some(built) = built else {
            return Ok(pre_request_fallback_payload(request));
        };

        let response_body = self
            .transport
            .send(&built, Some(request.scene.effective_timeout_ms()))?;
        let candidates = extract_openai_compatible_candidates(&response_body);
        if request.scene.id == COMMAND_SCENE_ID {
            return Ok(command_mode_payload(
                request.selected_text.unwrap_or_default(),
                request.raw_text,
                candidates,
            ));
        }
        Ok(normal_mode_payload(request.raw_text, candidates))
    }
}

fn pre_request_fallback_payload(request: &TextRequest<'_>) -> RecognitionPayload {
    if request.scene.id != COMMAND_SCENE_ID {
        return normal_mode_payload(request.raw_text, Vec::<String>::new());
    }
    let normalized_asr = trim_ascii_whitespace(request.raw_text);
    let fallback_text = if normalized_asr.is_empty() {
        request.selected_text.unwrap_or_default()
    } else {
        normalized_asr
    };
    let mut payload = normal_mode_payload(fallback_text, Vec::<String>::new());
    fallback_text.clone_into(&mut payload.commit_text);
    payload
}

fn post_request_fallback_payload(request: &TextRequest<'_>) -> RecognitionPayload {
    if request.scene.id == COMMAND_SCENE_ID {
        command_mode_payload(
            request.selected_text.unwrap_or_default(),
            request.raw_text,
            Vec::<String>::new(),
        )
    } else {
        normal_mode_payload(request.raw_text, Vec::<String>::new())
    }
}

fn select_openai_compatible_provider<'a>(
    providers: &'a [LlmProviderConfig],
    scene: &SceneDefinition,
) -> Result<Option<&'a LlmProviderConfig>, TextError> {
    if let Some(provider_id) = scene.provider_id.as_deref() {
        return providers
            .iter()
            .find(|provider| provider.id == provider_id)
            .map(Some)
            .ok_or_else(|| TextError::UnknownProvider {
                scene_id: scene.id.clone(),
                provider_id: provider_id.to_owned(),
            });
    }

    match providers {
        [] => Ok(None),
        [provider] => Ok(Some(provider)),
        _ => Err(TextError::AmbiguousProvider(scene.id.clone())),
    }
}

/// Text processor that selects an OpenAI-compatible provider per scene.
#[derive(Debug, Clone)]
pub struct OpenAiCompatibleTextProcessor<T> {
    providers: Vec<LlmProviderConfig>,
    transport: T,
    context_cache_path: Option<PathBuf>,
}

impl<T> OpenAiCompatibleTextProcessor<T> {
    /// Creates a processor from OpenAI-compatible provider config entries.
    #[must_use]
    pub fn new(providers: Vec<LlmProviderConfig>, transport: T) -> Self {
        Self {
            providers,
            transport,
            context_cache_path: None,
        }
    }

    /// Adds a recent-input context cache path used by scenes with context lines.
    #[must_use]
    pub fn with_context_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.context_cache_path = Some(path.into());
        self
    }

    /// Returns configured OpenAI-compatible providers.
    #[must_use]
    pub fn providers(&self) -> &[LlmProviderConfig] {
        &self.providers
    }
}

impl<T> TextProcessor for OpenAiCompatibleTextProcessor<T>
where
    T: OpenAiCompatibleChatTransport + Clone,
{
    fn finish(&self, request: &TextRequest<'_>) -> Result<RecognitionPayload, TextError> {
        if request.scene.id == RAW_SCENE_ID || !scene_needs_postprocessing(request.scene) {
            return Ok(RecognitionPayload::raw(request.raw_text));
        }
        let Some(provider) = select_openai_compatible_provider(&self.providers, request.scene)?
        else {
            return Ok(pre_request_fallback_payload(request));
        };
        let mut adapter =
            OpenAiCompatibleTextAdapter::new(provider.clone(), self.transport.clone());
        if let Some(context_cache_path) = &self.context_cache_path {
            adapter = adapter.with_context_cache_path(context_cache_path.clone());
        }
        adapter.finish(request)
    }

    fn finish_report(&self, request: &TextRequest<'_>) -> Result<TextProcessReport, TextError> {
        match self.finish(request) {
            Ok(payload) => Ok(TextProcessReport::success(payload)),
            Err(error @ TextError::PromptFileLoad(_)) => Ok(TextProcessReport {
                payload: pre_request_fallback_payload(request),
                warning: Some(error),
            }),
            Err(error @ TextError::AdapterFailed(_)) => Ok(TextProcessReport {
                payload: post_request_fallback_payload(request),
                warning: Some(error),
            }),
            Err(error) => Err(error),
        }
    }
}
