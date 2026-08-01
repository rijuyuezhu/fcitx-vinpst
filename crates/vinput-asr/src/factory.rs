//! ASR backend factory and config-derived diagnostic state.

use vinput_config::{
    AsrConfig, AsrProviderConfig, AsrProviderKind, VadConfig, redact_url_for_diagnostics,
};
use vinput_protocol::AsrBackendState;

#[cfg(feature = "sherpa-onnx-backend")]
use crate::SherpaOnnxBackend;
use crate::{
    AsrBackend, AsrError, BackendCapabilities, CommandAsrBackend, CommandAsrSpec,
    LegacyCommandBatchRunner, LegacyCommandStreamingRunner, MockAsrBackend, RecognitionContext,
    RemoteAsrBackend, SHERPA_ONNX_PROVIDER_ID,
};

const WARMUP_SCENE_ID: &str = "__vinput_asr_warmup__";

/// Builds ASR backends from typed config entries.
#[derive(Debug, Clone, Copy, Default)]
pub struct AsrBackendFactory;

impl AsrBackendFactory {
    /// Creates a factory.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }

    /// Builds the active backend from ASR config.
    pub fn build_active(config: &AsrConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        if config.active_provider.is_empty() {
            return Err(AsrError::NoActiveProvider);
        }
        let provider = active_provider(config)
            .ok_or_else(|| AsrError::UnknownProvider(config.active_provider.clone()))?;
        Self::build_provider_with_vad(provider, Some(&config.vad))
    }

    /// Builds and prepares the active backend before it becomes effective.
    ///
    /// Preparation creates and cancels one normal recognition session. This
    /// mirrors the legacy warm-reload boundary: a backend that can be
    /// constructed but cannot create a usable session is rejected before swap.
    pub fn build_active_prepared(
        config: &AsrConfig,
        language: Option<String>,
    ) -> Result<Box<dyn AsrBackend>, AsrError> {
        let backend = Self::build_active(config)?;
        Self::prepare_backend(backend.as_ref(), language)?;
        Ok(backend)
    }

    /// Creates and cancels one warmup session for an already built backend.
    pub fn prepare_backend(
        backend: &dyn AsrBackend,
        language: Option<String>,
    ) -> Result<(), AsrError> {
        let descriptor = backend.describe();
        let context = RecognitionContext::normal(WARMUP_SCENE_ID, language);
        let mut session = backend.create_session(context).map_err(|error| {
            AsrError::Backend(format!(
                "failed to prepare ASR backend `{}`: {error}",
                descriptor.provider_id
            ))
        })?;
        session.cancel().map_err(|error| {
            AsrError::Backend(format!(
                "failed to cancel ASR warmup session for `{}`: {error}",
                descriptor.provider_id
            ))
        })
    }

    /// Parses an external command ASR provider into an executable spec.
    pub fn command_spec(provider: &AsrProviderConfig) -> Result<CommandAsrSpec, AsrError> {
        CommandAsrSpec::try_from(provider)
    }

    /// Builds a backend from one provider entry.
    pub fn build_provider(provider: &AsrProviderConfig) -> Result<Box<dyn AsrBackend>, AsrError> {
        Self::build_provider_with_vad(provider, None)
    }

    fn build_provider_with_vad(
        provider: &AsrProviderConfig,
        vad: Option<&VadConfig>,
    ) -> Result<Box<dyn AsrBackend>, AsrError> {
        #[cfg(not(feature = "sherpa-onnx-backend"))]
        let _ = vad;
        if provider.id == "mock" {
            return Ok(Box::new(MockAsrBackend::streaming(
                "mock partial",
                "mock recognition result",
            )));
        }
        if provider.kind == AsrProviderKind::Command {
            if is_legacy_streaming_command_provider(&provider.id) {
                return Ok(Box::new(CommandAsrBackend::with_config_and_capabilities(
                    provider,
                    LegacyCommandStreamingRunner,
                    BackendCapabilities::streaming(),
                )?));
            }
            return Ok(Box::new(CommandAsrBackend::with_config(
                provider,
                LegacyCommandBatchRunner,
            )?));
        }
        if provider.kind == AsrProviderKind::Remote {
            return Ok(Box::new(RemoteAsrBackend::with_config(provider)?));
        }
        if provider.id == SHERPA_ONNX_PROVIDER_ID && provider.kind == AsrProviderKind::Local {
            #[cfg(feature = "sherpa-onnx-backend")]
            {
                return Ok(Box::new(SherpaOnnxBackend::with_config_and_vad(
                    provider, vad,
                )?));
            }
            #[cfg(not(feature = "sherpa-onnx-backend"))]
            {
                let spec = crate::SherpaOnnxSpec::from_provider(provider)?;
                return Err(spec.runtime_unavailable_error());
            }
        }
        unsupported_provider(&provider.id, &provider.kind)
    }

    /// Builds a user-facing ASR state snapshot from config and load outcome.
    #[must_use]
    pub fn state_for_config(config: &AsrConfig) -> AsrBackendState {
        let target_model_id = target_model_id(config);
        let remote_endpoints = remote_endpoints(config);
        match Self::build_active_prepared(config, None) {
            Ok(backend) => {
                let descriptor = backend.describe();
                let mut state = AsrBackendState::ready(descriptor.provider_id, descriptor.model_id);
                state.target_provider_id.clone_from(&config.active_provider);
                state.target_model_id = target_model_id;
                state.remote_endpoints = remote_endpoints;
                state
            }
            Err(error) => {
                let mut state = AsrBackendState::unavailable(
                    config.active_provider.clone(),
                    target_model_id,
                    error.to_string(),
                );
                state.remote_endpoints = remote_endpoints;
                state
            }
        }
    }
}

fn is_legacy_streaming_command_provider(provider_id: &str) -> bool {
    provider_id.ends_with(".streaming")
}

fn active_provider(config: &AsrConfig) -> Option<&AsrProviderConfig> {
    config
        .providers
        .iter()
        .find(|provider| provider.id == config.active_provider)
}

fn target_model_id(config: &AsrConfig) -> String {
    active_provider(config)
        .and_then(|provider| provider.model.clone())
        .unwrap_or_default()
}

fn remote_endpoints(config: &AsrConfig) -> Vec<String> {
    active_provider(config)
        .and_then(|provider| provider.endpoint.as_deref())
        .map(str::trim)
        .filter(|endpoint| !endpoint.is_empty())
        .map(|endpoint| vec![redact_url_for_diagnostics(endpoint)])
        .unwrap_or_default()
}

fn unsupported_provider(
    provider_id: &str,
    kind: &AsrProviderKind,
) -> Result<Box<dyn AsrBackend>, AsrError> {
    Err(AsrError::UnsupportedProviderKind {
        provider_id: provider_id.to_owned(),
        kind: provider_kind_label(kind).to_owned(),
    })
}

pub(crate) fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    use vinput_config::{AsrConfig, AsrProviderConfig, AsrProviderKind};

    use super::AsrBackendFactory;
    use crate::{
        AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
        RecognitionEvent, RecognitionSession,
    };

    struct WarmupBackend {
        context: Arc<Mutex<Option<RecognitionContext>>>,
        cancelled: Arc<Mutex<bool>>,
        fail_create: bool,
    }

    impl AsrBackend for WarmupBackend {
        fn describe(&self) -> BackendDescriptor {
            BackendDescriptor::new(
                "warmup-test",
                "model",
                "warmup test",
                BackendCapabilities::buffered(),
            )
        }

        fn create_session(
            &self,
            context: RecognitionContext,
        ) -> Result<Box<dyn RecognitionSession>, AsrError> {
            if self.fail_create {
                return Err(AsrError::Backend("session init failed".to_owned()));
            }
            *self.context.lock().expect("context lock poisoned") = Some(context);
            Ok(Box::new(WarmupSession {
                cancelled: Arc::clone(&self.cancelled),
            }))
        }
    }

    struct WarmupSession {
        cancelled: Arc<Mutex<bool>>,
    }

    impl RecognitionSession for WarmupSession {
        fn push_audio(&mut self, _samples: &[i16]) -> Result<(), AsrError> {
            Ok(())
        }

        fn finish(&mut self) -> Result<(), AsrError> {
            Ok(())
        }

        fn cancel(&mut self) -> Result<(), AsrError> {
            *self.cancelled.lock().expect("cancel lock poisoned") = true;
            Ok(())
        }

        fn poll_events(&mut self) -> Result<Vec<RecognitionEvent>, AsrError> {
            Ok(Vec::new())
        }
    }

    #[test]
    fn prepare_backend_creates_and_cancels_normal_session() {
        let context = Arc::new(Mutex::new(None));
        let cancelled = Arc::new(Mutex::new(false));
        let backend = WarmupBackend {
            context: Arc::clone(&context),
            cancelled: Arc::clone(&cancelled),
            fail_create: false,
        };

        AsrBackendFactory::prepare_backend(&backend, Some("zh-CN".to_owned())).unwrap();

        let context = context
            .lock()
            .expect("context lock poisoned")
            .clone()
            .expect("warmup context");
        assert_eq!(context.scene_id, "__vinput_asr_warmup__");
        assert_eq!(context.language.as_deref(), Some("zh-CN"));
        assert!(!context.command_mode);
        assert!(*cancelled.lock().expect("cancel lock poisoned"));
    }

    #[test]
    fn prepare_backend_reports_session_creation_failure() {
        let backend = WarmupBackend {
            context: Arc::new(Mutex::new(None)),
            cancelled: Arc::new(Mutex::new(false)),
            fail_create: true,
        };

        let error = AsrBackendFactory::prepare_backend(&backend, None).unwrap_err();

        assert!(
            error
                .to_string()
                .contains("failed to prepare ASR backend `warmup-test`")
        );
        assert!(error.to_string().contains("session init failed"));
    }

    #[test]
    fn state_redacts_remote_endpoint_credentials_and_query_values() {
        let config = AsrConfig {
            active_provider: "remote".to_owned(),
            providers: vec![AsrProviderConfig {
                id: "remote".to_owned(),
                kind: AsrProviderKind::Remote,
                timeout_ms: Some(1_000),
                model: Some("model".to_owned()),
                hotwords_file: None,
                command: None,
                args: Vec::new(),
                env: HashMap::new(),
                endpoint: Some(
                    "https://user:password@asr.example.test/v1?api-key=secret#fragment".to_owned(),
                ),
            }],
            ..AsrConfig::default()
        };

        let state = AsrBackendFactory::state_for_config(&config);

        assert_eq!(
            state.remote_endpoints,
            ["https://asr.example.test/v1?api-key=REDACTED"]
        );
    }
}
