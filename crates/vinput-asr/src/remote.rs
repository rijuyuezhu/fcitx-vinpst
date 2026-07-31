//! OpenAI-compatible remote ASR backend and blocking HTTP transport.

use std::fmt;

use vinput_audio::{PcmBuffer, PcmSpec, i16_samples_to_le_bytes};
use vinput_config::{AsrProviderConfig, AsrProviderKind};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

const OPENAI_TRANSCRIPTIONS_PATH: &str = "/audio/transcriptions";
const API_KEY_ENV: &str = "VINPUT_ASR_API_KEY";
const MODEL_ENV: &str = "VINPUT_ASR_MODEL";
const LANGUAGE_ENV: &str = "VINPUT_ASR_LANGUAGE";
const PROMPT_ENV: &str = "VINPUT_ASR_PROMPT";

/// Parsed OpenAI-compatible remote ASR provider specification.
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteAsrSpec {
    /// Stable provider id.
    pub provider_id: String,
    /// Fully resolved `/audio/transcriptions` endpoint.
    pub url: String,
    /// Remote model id.
    pub model_id: String,
    /// Optional bearer credential.
    api_key: String,
    /// Optional provider-level language fallback.
    pub language: Option<String>,
    /// Optional recognition prompt or bias text.
    pub prompt: Option<String>,
    /// Optional request timeout in milliseconds.
    pub timeout_ms: Option<u64>,
}

impl fmt::Debug for RemoteAsrSpec {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteAsrSpec")
            .field("provider_id", &self.provider_id)
            .field("url", &self.url)
            .field("model_id", &self.model_id)
            .field(
                "api_key",
                &if self.api_key.is_empty() {
                    ""
                } else {
                    "<redacted>"
                },
            )
            .field("language", &self.language)
            .field("prompt", &self.prompt)
            .field("timeout_ms", &self.timeout_ms)
            .finish()
    }
}

impl RemoteAsrSpec {
    /// Returns whether this provider sends a bearer credential.
    #[must_use]
    pub fn has_api_key(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn api_key(&self) -> &str {
        &self.api_key
    }
}

impl TryFrom<&AsrProviderConfig> for RemoteAsrSpec {
    type Error = AsrError;

    fn try_from(provider: &AsrProviderConfig) -> Result<Self, Self::Error> {
        if provider.kind != AsrProviderKind::Remote {
            return Err(AsrError::Backend(format!(
                "provider `{}` is not a remote ASR provider",
                provider.id
            )));
        }
        let endpoint = provider
            .endpoint
            .as_deref()
            .map(str::trim)
            .filter(|endpoint| !endpoint.is_empty())
            .ok_or_else(|| {
                AsrError::Backend(format!(
                    "remote ASR provider `{}` must configure an endpoint",
                    provider.id
                ))
            })?;
        let url = build_openai_compatible_transcriptions_url(endpoint)?;
        let model_id = provider
            .model
            .as_deref()
            .map(str::trim)
            .filter(|model| !model.is_empty())
            .or_else(|| {
                provider
                    .env
                    .get(MODEL_ENV)
                    .map(String::as_str)
                    .map(str::trim)
                    .filter(|model| !model.is_empty())
            })
            .ok_or_else(|| {
                AsrError::Backend(format!(
                    "remote ASR provider `{}` must configure a model",
                    provider.id
                ))
            })?
            .to_owned();
        Ok(Self {
            provider_id: provider.id.clone(),
            url,
            model_id,
            api_key: trimmed_env(provider, API_KEY_ENV).unwrap_or_default(),
            language: trimmed_env(provider, LANGUAGE_ENV),
            prompt: trimmed_env(provider, PROMPT_ENV),
            timeout_ms: provider.timeout_ms,
        })
    }
}

fn trimmed_env(provider: &AsrProviderConfig, key: &str) -> Option<String> {
    provider
        .env
        .get(key)
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

/// Resolves a base URL or full transcription endpoint.
pub fn build_openai_compatible_transcriptions_url(endpoint: &str) -> Result<String, AsrError> {
    let mut url = reqwest::Url::parse(endpoint).map_err(|error| {
        AsrError::Backend(format!("invalid remote ASR endpoint `{endpoint}`: {error}"))
    })?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(AsrError::Backend(format!(
            "unsupported remote ASR endpoint scheme `{}`; expected http or https",
            url.scheme()
        )));
    }
    if !url
        .path()
        .trim_end_matches('/')
        .ends_with(OPENAI_TRANSCRIPTIONS_PATH)
    {
        let path = url.path().trim_end_matches('/');
        url.set_path(&format!("{path}{OPENAI_TRANSCRIPTIONS_PATH}"));
    }
    Ok(url.to_string())
}

/// Fully built OpenAI-compatible transcription request.
#[derive(Clone, PartialEq, Eq)]
pub struct RemoteAsrRequest {
    /// Fully resolved request URL.
    pub url: String,
    /// Remote model id.
    pub model: String,
    /// Optional language hint.
    pub language: Option<String>,
    /// Optional recognition prompt.
    pub prompt: Option<String>,
    /// PCM16LE WAV payload.
    pub wav_bytes: Vec<u8>,
    /// Optional request timeout.
    pub timeout_ms: Option<u64>,
    api_key: String,
}

impl fmt::Debug for RemoteAsrRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RemoteAsrRequest")
            .field("url", &self.url)
            .field("model", &self.model)
            .field("language", &self.language)
            .field("prompt", &self.prompt)
            .field(
                "wav_bytes",
                &format_args!("<{} bytes>", self.wav_bytes.len()),
            )
            .field("timeout_ms", &self.timeout_ms)
            .field(
                "authorization",
                &if self.api_key.is_empty() {
                    ""
                } else {
                    "Bearer <redacted>"
                },
            )
            .finish()
    }
}

impl RemoteAsrRequest {
    /// Returns whether this request sends bearer authorization.
    #[must_use]
    pub fn has_authorization(&self) -> bool {
        !self.api_key.is_empty()
    }

    fn api_key(&self) -> &str {
        &self.api_key
    }
}

/// Synchronous transport seam for one remote transcription request.
pub trait RemoteAsrTransport: Send + Sync {
    /// Sends one request and returns its raw response body.
    fn transcribe(&self, request: &RemoteAsrRequest) -> Result<String, AsrError>;
}

/// Blocking reqwest multipart transport.
#[derive(Debug, Clone, Copy, Default)]
pub struct ReqwestRemoteAsrTransport;

impl ReqwestRemoteAsrTransport {
    /// Creates the production HTTP transport.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

impl RemoteAsrTransport for ReqwestRemoteAsrTransport {
    fn transcribe(&self, request: &RemoteAsrRequest) -> Result<String, AsrError> {
        let request = request.clone();
        std::thread::spawn(move || send_remote_asr_request_blocking(&request))
            .join()
            .map_err(|_| AsrError::Backend("remote ASR HTTP worker thread panicked".to_owned()))?
    }
}

fn send_remote_asr_request_blocking(request: &RemoteAsrRequest) -> Result<String, AsrError> {
    let file = reqwest::blocking::multipart::Part::bytes(request.wav_bytes.clone())
        .file_name("audio.wav")
        .mime_str("audio/wav")
        .map_err(|error| {
            AsrError::Backend(format!("failed to build remote ASR WAV part: {error}"))
        })?;
    let mut form = reqwest::blocking::multipart::Form::new()
        .part("file", file)
        .text("model", request.model.clone());
    if let Some(language) = &request.language {
        form = form.text("language", language.clone());
    }
    if let Some(prompt) = &request.prompt {
        form = form.text("prompt", prompt.clone());
    }

    let client = reqwest::blocking::Client::new();
    let mut builder = client.post(&request.url).multipart(form);
    if !request.api_key().is_empty() {
        builder = builder.bearer_auth(request.api_key());
    }
    if let Some(timeout_ms) = request.timeout_ms {
        builder = builder.timeout(std::time::Duration::from_millis(timeout_ms));
    }
    let response = builder.send().map_err(|error| {
        if error.is_timeout() {
            AsrError::Backend("remote ASR HTTP request timed out".to_owned())
        } else {
            AsrError::Backend(format!("remote ASR HTTP request failed: {error}"))
        }
    })?;
    let status = response.status();
    let body = response.text().map_err(|error| {
        AsrError::Backend(format!("remote ASR HTTP response read failed: {error}"))
    })?;
    if !status.is_success() {
        return Err(AsrError::Backend(format!(
            "remote ASR provider returned HTTP {status}: {body}"
        )));
    }
    Ok(body)
}

/// Extracts the required non-empty `text` field from an OpenAI-compatible response.
pub fn extract_openai_compatible_transcription(response_body: &str) -> Result<String, AsrError> {
    let response = serde_json::from_str::<serde_json::Value>(response_body)
        .map_err(|error| AsrError::Backend(format!("invalid remote ASR response JSON: {error}")))?;
    response
        .get("text")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(ToOwned::to_owned)
        .ok_or_else(|| AsrError::Backend("remote ASR response missing final text".to_owned()))
}

/// OpenAI-compatible buffered remote ASR backend.
#[derive(Debug, Clone)]
pub struct RemoteAsrBackend<T = ReqwestRemoteAsrTransport> {
    spec: RemoteAsrSpec,
    descriptor: BackendDescriptor,
    transport: T,
}

impl RemoteAsrBackend<ReqwestRemoteAsrTransport> {
    /// Creates a production remote ASR backend from typed config.
    pub fn with_config(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        Ok(Self::with_transport(
            RemoteAsrSpec::try_from(provider)?,
            ReqwestRemoteAsrTransport,
        ))
    }
}

impl<T> RemoteAsrBackend<T> {
    /// Creates a backend with an injected transport.
    #[must_use]
    pub fn with_transport(spec: RemoteAsrSpec, transport: T) -> Self {
        let descriptor = BackendDescriptor::new(
            spec.provider_id.clone(),
            spec.model_id.clone(),
            "Remote ASR",
            BackendCapabilities::buffered(),
        );
        Self {
            spec,
            descriptor,
            transport,
        }
    }

    /// Returns the parsed remote provider spec.
    #[must_use]
    pub const fn spec(&self) -> &RemoteAsrSpec {
        &self.spec
    }
}

impl<T: RemoteAsrTransport + Clone + 'static> AsrBackend for RemoteAsrBackend<T> {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        Ok(Box::new(RemoteRecognitionSession {
            spec: self.spec.clone(),
            context,
            transport: self.transport.clone(),
            pcm: PcmSpec::default(),
            samples: Vec::new(),
            finished: false,
            cancelled: false,
            events: Vec::new(),
        }))
    }
}

#[derive(Debug)]
struct RemoteRecognitionSession<T> {
    spec: RemoteAsrSpec,
    context: RecognitionContext,
    transport: T,
    pcm: PcmSpec,
    samples: Vec<i16>,
    finished: bool,
    cancelled: bool,
    events: Vec<RecognitionEvent>,
}

impl<T: RemoteAsrTransport> RecognitionSession for RemoteRecognitionSession<T> {
    fn push_pcm(&mut self, pcm: &PcmBuffer) -> Result<(), AsrError> {
        self.ensure_writable()?;
        let next_pcm = pcm.spec();
        if !self.samples.is_empty() && self.pcm != next_pcm {
            return Err(AsrError::Backend(format!(
                "remote ASR PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
                self.pcm.sample_rate_hz,
                self.pcm.channels,
                next_pcm.sample_rate_hz,
                next_pcm.channels
            )));
        }
        self.pcm = next_pcm;
        self.samples.extend_from_slice(pcm.samples());
        Ok(())
    }

    fn push_audio(&mut self, samples: &[i16]) -> Result<(), AsrError> {
        self.ensure_writable()?;
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        self.ensure_writable()?;
        self.finished = true;
        if self.samples.is_empty() {
            return Err(AsrError::Backend(
                "remote ASR request contains no audio".to_owned(),
            ));
        }
        let request = RemoteAsrRequest {
            url: self.spec.url.clone(),
            model: self.spec.model_id.clone(),
            language: self
                .context
                .language
                .clone()
                .filter(|language| !language.trim().is_empty())
                .or_else(|| self.spec.language.clone()),
            prompt: self.spec.prompt.clone(),
            wav_bytes: encode_pcm16le_wav(self.pcm, &self.samples)?,
            timeout_ms: self.spec.timeout_ms,
            api_key: self.spec.api_key().to_owned(),
        };
        let text = extract_openai_compatible_transcription(&self.transport.transcribe(&request)?)?;
        self.events = vec![
            RecognitionEvent::FinalText { text },
            RecognitionEvent::Completed,
        ];
        Ok(())
    }

    fn cancel(&mut self) -> Result<(), AsrError> {
        self.cancelled = true;
        self.events.clear();
        Ok(())
    }

    fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
        Ok(std::mem::take(&mut self.events))
    }
}

impl<T> RemoteRecognitionSession<T> {
    fn ensure_writable(&self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        Ok(())
    }
}

/// Encodes signed 16-bit interleaved PCM as a canonical RIFF/WAVE payload.
pub fn encode_pcm16le_wav(pcm: PcmSpec, samples: &[i16]) -> Result<Vec<u8>, AsrError> {
    pcm.validate()
        .map_err(|error| AsrError::Backend(format!("invalid remote ASR PCM spec: {error}")))?;
    if !samples.len().is_multiple_of(usize::from(pcm.channels)) {
        return Err(AsrError::Backend(format!(
            "remote ASR PCM sample count {} is not aligned to {} channel(s)",
            samples.len(),
            pcm.channels
        )));
    }
    let data = i16_samples_to_le_bytes(samples);
    let data_len = u32::try_from(data.len())
        .map_err(|_| AsrError::Backend("remote ASR WAV payload is too large".to_owned()))?;
    let riff_size = 36_u32
        .checked_add(data_len)
        .ok_or_else(|| AsrError::Backend("remote ASR WAV payload is too large".to_owned()))?;
    let byte_rate = pcm
        .sample_rate_hz
        .checked_mul(u32::from(pcm.channels))
        .and_then(|value| value.checked_mul(2))
        .ok_or_else(|| AsrError::Backend("remote ASR PCM byte rate overflow".to_owned()))?;
    let block_align = pcm
        .channels
        .checked_mul(2)
        .ok_or_else(|| AsrError::Backend("remote ASR PCM block alignment overflow".to_owned()))?;

    let mut wav = Vec::with_capacity(44 + data.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&riff_size.to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&pcm.channels.to_le_bytes());
    wav.extend_from_slice(&pcm.sample_rate_hz.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&data);
    Ok(wav)
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        io::{Read, Write},
        net::TcpListener,
        sync::{Arc, Mutex},
        thread,
        time::Duration,
    };

    use vinput_audio::{PcmBuffer, PcmSpec};
    use vinput_config::{AsrProviderConfig, AsrProviderKind};

    use super::{
        RemoteAsrBackend, RemoteAsrRequest, RemoteAsrSpec, RemoteAsrTransport,
        ReqwestRemoteAsrTransport, build_openai_compatible_transcriptions_url, encode_pcm16le_wav,
        extract_openai_compatible_transcription,
    };
    use crate::{AsrBackend, AsrBackendFactory, AsrError, RecognitionContext, RecognitionEvent};

    #[derive(Debug, Clone)]
    struct StaticTransport {
        response: Result<String, String>,
        request: Arc<Mutex<Option<RemoteAsrRequest>>>,
    }

    impl StaticTransport {
        fn success(body: impl Into<String>) -> Self {
            Self {
                response: Ok(body.into()),
                request: Arc::new(Mutex::new(None)),
            }
        }
    }

    impl RemoteAsrTransport for StaticTransport {
        fn transcribe(&self, request: &RemoteAsrRequest) -> Result<String, AsrError> {
            *self.request.lock().expect("request lock poisoned") = Some(request.clone());
            self.response
                .clone()
                .map_err(|error| AsrError::Backend(error.clone()))
        }
    }

    fn provider(endpoint: &str) -> AsrProviderConfig {
        AsrProviderConfig {
            id: "remote-test".to_owned(),
            kind: AsrProviderKind::Remote,
            timeout_ms: Some(2_500),
            model: Some("whisper-test".to_owned()),
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: HashMap::from([
                ("VINPUT_ASR_API_KEY".to_owned(), "secret-token".to_owned()),
                ("VINPUT_ASR_LANGUAGE".to_owned(), "en".to_owned()),
                ("VINPUT_ASR_PROMPT".to_owned(), "names".to_owned()),
            ]),
            endpoint: Some(endpoint.to_owned()),
        }
    }

    #[test]
    fn builds_openai_compatible_transcription_urls() {
        assert_eq!(
            build_openai_compatible_transcriptions_url("https://api.example/v1").unwrap(),
            "https://api.example/v1/audio/transcriptions"
        );
        assert_eq!(
            build_openai_compatible_transcriptions_url(
                "https://api.example/v1/audio/transcriptions"
            )
            .unwrap(),
            "https://api.example/v1/audio/transcriptions"
        );
        assert!(
            build_openai_compatible_transcriptions_url("ftp://api.example/v1")
                .unwrap_err()
                .to_string()
                .contains("unsupported remote ASR endpoint scheme")
        );
    }

    #[test]
    fn remote_spec_uses_typed_and_legacy_environment_fields() {
        let spec = RemoteAsrSpec::try_from(&provider("https://api.example/v1")).unwrap();
        assert_eq!(spec.provider_id, "remote-test");
        assert_eq!(spec.model_id, "whisper-test");
        assert_eq!(spec.language.as_deref(), Some("en"));
        assert_eq!(spec.prompt.as_deref(), Some("names"));
        assert_eq!(spec.timeout_ms, Some(2_500));
        assert!(spec.has_api_key());
        let debug = format!("{spec:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret-token"));

        let mut env_model = provider("https://api.example/v1");
        env_model.model = Some("  ".to_owned());
        env_model
            .env
            .insert("VINPUT_ASR_MODEL".to_owned(), "env-model".to_owned());
        assert_eq!(
            RemoteAsrSpec::try_from(&env_model).unwrap().model_id,
            "env-model"
        );
    }

    #[test]
    fn remote_spec_requires_http_endpoint_and_model() {
        let mut missing_model = provider("https://api.example/v1");
        missing_model.model = None;
        missing_model.env.remove("VINPUT_ASR_MODEL");
        assert!(
            RemoteAsrSpec::try_from(&missing_model)
                .unwrap_err()
                .to_string()
                .contains("must configure a model")
        );
        let invalid_scheme = provider("ftp://api.example/v1");
        assert!(
            RemoteAsrSpec::try_from(&invalid_scheme)
                .unwrap_err()
                .to_string()
                .contains("expected http or https")
        );
    }

    #[test]
    fn wav_encoder_preserves_pcm_layout_and_samples() {
        let wav = encode_pcm16le_wav(
            PcmSpec {
                sample_rate_hz: 8_000,
                channels: 2,
            },
            &[1_000, -1_000, 2_000, -2_000],
        )
        .unwrap();
        let decoded = PcmBuffer::from_wav_pcm16le_bytes(&wav).unwrap();
        assert_eq!(decoded.sample_rate_hz(), 8_000);
        assert_eq!(decoded.channels(), 2);
        assert_eq!(decoded.samples(), &[1_000, -1_000, 2_000, -2_000]);
    }

    #[test]
    fn injected_transport_receives_wav_and_context_language() {
        let spec = RemoteAsrSpec::try_from(&provider("https://api.example/v1")).unwrap();
        let transport = StaticTransport::success(r#"{"text":" remote result "}"#);
        let seen = Arc::clone(&transport.request);
        let backend = RemoteAsrBackend::with_transport(spec, transport);
        let mut session = backend
            .create_session(RecognitionContext::normal("raw", Some("zh-CN".to_owned())))
            .unwrap();
        session
            .push_pcm(&PcmBuffer::new(16_000, vec![0, 1_000, -1_000]).unwrap())
            .unwrap();
        session.finish().unwrap();
        assert_eq!(
            session.poll_events().unwrap(),
            [
                RecognitionEvent::FinalText {
                    text: "remote result".to_owned()
                },
                RecognitionEvent::Completed
            ]
        );
        let request = seen
            .lock()
            .expect("request lock poisoned")
            .clone()
            .expect("captured request");
        assert_eq!(request.model, "whisper-test");
        assert_eq!(request.language.as_deref(), Some("zh-CN"));
        assert_eq!(request.prompt.as_deref(), Some("names"));
        assert_eq!(request.timeout_ms, Some(2_500));
        assert!(request.has_authorization());
        assert_eq!(&request.wav_bytes[0..4], b"RIFF");
        let debug = format!("{request:?}");
        assert!(debug.contains("Bearer <redacted>"));
        assert!(!debug.contains("secret-token"));
    }

    #[test]
    fn warmup_creates_and_cancels_without_http_request() {
        let spec = RemoteAsrSpec::try_from(&provider("https://api.example/v1")).unwrap();
        let transport = StaticTransport::success(r#"{"text":"unused"}"#);
        let seen = Arc::clone(&transport.request);
        let backend = RemoteAsrBackend::with_transport(spec, transport);
        AsrBackendFactory::prepare_backend(&backend, Some("zh".to_owned())).unwrap();
        assert!(seen.lock().expect("request lock poisoned").is_none());
    }

    #[test]
    fn response_requires_non_empty_text() {
        assert_eq!(
            extract_openai_compatible_transcription(r#"{"text":" hello "}"#).unwrap(),
            "hello"
        );
        assert!(
            extract_openai_compatible_transcription(r#"{"text":""}"#)
                .unwrap_err()
                .to_string()
                .contains("missing final text")
        );
        assert!(
            extract_openai_compatible_transcription("not-json")
                .unwrap_err()
                .to_string()
                .contains("invalid remote ASR response JSON")
        );
    }

    struct CapturedHttpRequest {
        head: String,
        body: Vec<u8>,
    }

    fn serve_single_response(
        status: &str,
        body: &str,
        delay: Duration,
    ) -> (String, thread::JoinHandle<CapturedHttpRequest>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let status = status.to_owned();
        let response_body = body.to_owned();
        let handle = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut received = Vec::new();
            let mut buffer = [0_u8; 8_192];
            let (header_end, content_length) = loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0, "HTTP client closed before request headers");
                received.extend_from_slice(&buffer[..count]);
                if let Some(index) = received.windows(4).position(|window| window == b"\r\n\r\n") {
                    let end = index + 4;
                    let head = String::from_utf8_lossy(&received[..end]);
                    let content_length = head.lines().find_map(|line| {
                        line.split_once(':').and_then(|(name, value)| {
                            name.eq_ignore_ascii_case("content-length")
                                .then(|| value.trim().parse::<usize>().unwrap())
                        })
                    });
                    break (end, content_length);
                }
            };
            let expected_len = header_end + content_length.expect("content-length header");
            while received.len() < expected_len {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0, "HTTP client closed before request body");
                received.extend_from_slice(&buffer[..count]);
            }
            thread::sleep(delay);
            write!(
                stream,
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                response_body.len(),
                response_body
            )
            .unwrap();
            CapturedHttpRequest {
                head: String::from_utf8_lossy(&received[..header_end]).into_owned(),
                body: received[header_end..expected_len].to_vec(),
            }
        });
        (format!("http://{address}/v1/audio/transcriptions"), handle)
    }

    fn request(url: String, timeout_ms: Option<u64>) -> RemoteAsrRequest {
        RemoteAsrRequest {
            url,
            model: "fixture-model".to_owned(),
            language: Some("zh".to_owned()),
            prompt: Some("fixture prompt".to_owned()),
            wav_bytes: encode_pcm16le_wav(PcmSpec::default(), &[0, 100, -100]).unwrap(),
            timeout_ms,
            api_key: "fixture-token".to_owned(),
        }
    }

    #[test]
    fn reqwest_transport_posts_multipart_wav_and_bearer() {
        let (url, handle) =
            serve_single_response("200 OK", r#"{"text":"via remote http"}"#, Duration::ZERO);
        let body = ReqwestRemoteAsrTransport
            .transcribe(&request(url, Some(2_000)))
            .unwrap();
        assert_eq!(
            extract_openai_compatible_transcription(&body).unwrap(),
            "via remote http"
        );
        let captured = handle.join().unwrap();
        assert!(
            captured
                .head
                .starts_with("POST /v1/audio/transcriptions HTTP/1.1")
        );
        let lower_head = captured.head.to_ascii_lowercase();
        assert!(lower_head.contains("authorization: bearer fixture-token"));
        assert!(lower_head.contains("content-type: multipart/form-data; boundary="));
        assert!(captured.body.windows(4).any(|window| window == b"RIFF"));
        let body_text = String::from_utf8_lossy(&captured.body);
        assert!(body_text.contains("name=\"file\""));
        assert!(body_text.contains("filename=\"audio.wav\""));
        assert!(body_text.contains("name=\"model\""));
        assert!(body_text.contains("fixture-model"));
        assert!(body_text.contains("name=\"language\""));
        assert!(body_text.contains("zh"));
        assert!(body_text.contains("name=\"prompt\""));
        assert!(body_text.contains("fixture prompt"));
    }

    #[test]
    fn reqwest_transport_reports_http_body_and_timeout() {
        let (url, handle) = serve_single_response(
            "503 Service Unavailable",
            r#"{"error":"offline"}"#,
            Duration::ZERO,
        );
        let error = ReqwestRemoteAsrTransport
            .transcribe(&request(url, Some(2_000)))
            .unwrap_err();
        handle.join().unwrap();
        assert!(error.to_string().contains("HTTP 503"));
        assert!(error.to_string().contains("offline"));

        let (url, handle) =
            serve_single_response("200 OK", r#"{"text":"late"}"#, Duration::from_millis(200));
        let error = ReqwestRemoteAsrTransport
            .transcribe(&request(url, Some(20)))
            .unwrap_err();
        assert!(error.to_string().contains("timed out"));
        handle.join().unwrap();
    }
}
