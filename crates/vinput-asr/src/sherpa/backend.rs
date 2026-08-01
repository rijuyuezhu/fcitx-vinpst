use std::path::PathBuf;

use vinput_config::{AsrProviderConfig, VadConfig};

use crate::{
    AsrBackend, AsrError, BackendCapabilities, BackendDescriptor, RecognitionContext,
    RecognitionEvent, RecognitionSession,
};

use super::{
    SherpaOnnxOfflineModelLayout, SherpaOnnxOfflineRuntimePlan, SherpaOnnxSpec,
    offline_layout::display_path,
};

/// Feature-gated offline `sherpa-onnx` backend using the official Rust API.
#[cfg(feature = "sherpa-onnx-backend")]
pub struct SherpaOnnxBackend {
    spec: SherpaOnnxSpec,
    descriptor: BackendDescriptor,
    runtime: SherpaOnnxRuntime,
}

#[cfg(feature = "sherpa-onnx-backend")]
enum SherpaOnnxRuntime {
    Offline(SherpaOnnxOfflineRuntime),
    Online(crate::sherpa_online::SherpaOnnxOnlineRuntime),
}

#[cfg(feature = "sherpa-onnx-backend")]
struct SherpaOnnxOfflineRuntime {
    recognizer: std::sync::Arc<sherpa_onnx::OfflineRecognizer>,
    vad: Option<std::sync::Arc<std::sync::Mutex<crate::sherpa_vad::SherpaOnnxVadTrimmer>>>,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl std::fmt::Debug for SherpaOnnxBackend {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SherpaOnnxBackend")
            .field("provider_id", &self.spec.provider_id)
            .field("model", &self.spec.model)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl SherpaOnnxBackend {
    /// Builds an offline or online `sherpa-onnx` backend from typed model metadata.
    pub fn with_config(provider: &AsrProviderConfig) -> Result<Self, AsrError> {
        Self::with_config_and_vad(provider, None)
    }

    /// Builds a backend and enables legacy offline VAD when a model can be resolved.
    pub fn with_config_and_vad(
        provider: &AsrProviderConfig,
        vad_config: Option<&VadConfig>,
    ) -> Result<Self, AsrError> {
        let spec = SherpaOnnxSpec::from_provider(provider)?;
        let model_root = std::env::var_os("VINPUT_SHERPA_MODEL_ROOT")
            .map_or_else(|| PathBuf::from("."), PathBuf::from);
        let online = spec
            .uses_online_runtime(&model_root)
            .map_err(|error| AsrError::Backend(error.to_string()))?;
        let (runtime, description, capabilities) = if online {
            let plan = spec
                .resolve_online_runtime_plan(&model_root)
                .map_err(|error| AsrError::Backend(error.to_string()))?;
            let runtime =
                crate::sherpa_online::SherpaOnnxOnlineRuntime::create(&plan, &spec.provider_id)?;
            (
                SherpaOnnxRuntime::Online(runtime),
                "sherpa-onnx online ASR",
                BackendCapabilities::streaming(),
            )
        } else {
            let plan = spec
                .resolve_offline_runtime_plan(model_root)
                .map_err(|error| AsrError::Backend(error.to_string()))?;
            let config = offline_recognizer_config(&plan);
            let recognizer = sherpa_onnx::OfflineRecognizer::create(&config).ok_or_else(|| {
                AsrError::Backend(format!(
                    "failed to create sherpa-onnx offline recognizer for provider `{}`",
                    spec.provider_id
                ))
            })?;
            let vad = vad_config.and_then(|config| {
                if !config.enabled {
                    return None;
                }
                let Some(plan) = crate::sherpa_vad::SherpaOnnxVadPlan::resolve(config) else {
                    eprintln!(
                        "vinput: VAD enabled but silero_vad.onnx was not found; using untrimmed audio"
                    );
                    return None;
                };
                let Some(trimmer) = crate::sherpa_vad::SherpaOnnxVadTrimmer::create(&plan) else {
                    eprintln!(
                        "vinput: failed to load Silero VAD model {}; using untrimmed audio",
                        plan.model.display()
                    );
                    return None;
                };
                Some(std::sync::Arc::new(std::sync::Mutex::new(trimmer)))
            });
            (
                SherpaOnnxRuntime::Offline(SherpaOnnxOfflineRuntime {
                    recognizer: std::sync::Arc::new(recognizer),
                    vad,
                }),
                "sherpa-onnx offline ASR",
                BackendCapabilities::buffered(),
            )
        };
        let model_id = spec.model.clone().unwrap_or_default();
        Ok(Self {
            descriptor: BackendDescriptor::new(
                spec.provider_id.clone(),
                model_id,
                description,
                capabilities,
            ),
            spec,
            runtime,
        })
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
fn apply_offline_settings(
    config: &mut sherpa_onnx::OfflineRecognizerConfig,
    plan: &SherpaOnnxOfflineRuntimePlan,
) {
    let settings = &plan.settings;
    config.model_config.num_threads = settings.num_threads;
    config.model_config.provider = Some(settings.provider.clone());
    config.model_config.debug = settings.debug;
    config
        .model_config
        .model_type
        .clone_from(&settings.model_type);
    config
        .model_config
        .modeling_unit
        .clone_from(&settings.modeling_unit);
    config.model_config.bpe_vocab = settings.bpe_vocab.as_deref().map(display_path);
    config.model_config.telespeech_ctc = settings.telespeech_ctc.as_deref().map(display_path);
    config.feat_config.sample_rate = settings.sample_rate;
    config.feat_config.feature_dim = settings.feature_dim;
    config.lm_config.model = settings.lm_model.as_deref().map(display_path);
    config.lm_config.scale = settings.lm_scale;
    config.decoding_method = Some(settings.decoding_method.clone());
    config.max_active_paths = settings.max_active_paths;
    config.hotwords_file = plan
        .paths
        .hotwords_file
        .as_deref()
        .or(settings.hotwords_file.as_deref())
        .map(display_path);
    config.hotwords_score = settings.hotwords_score;
    config.rule_fsts = settings.rule_fsts.as_deref().map(display_path);
    config.rule_fars = settings.rule_fars.as_deref().map(display_path);
    config.blank_penalty = settings.blank_penalty;
    config.hr.lexicon = settings.homophone_lexicon.as_deref().map(display_path);
    config.hr.rule_fsts = settings.homophone_rule_fsts.as_deref().map(display_path);
}

#[cfg(feature = "sherpa-onnx-backend")]
fn offline_recognizer_config(
    plan: &SherpaOnnxOfflineRuntimePlan,
) -> sherpa_onnx::OfflineRecognizerConfig {
    let mut config = sherpa_onnx::OfflineRecognizerConfig::default();
    apply_offline_settings(&mut config, plan);
    match &plan.layout {
        SherpaOnnxOfflineModelLayout::Transducer {
            encoder,
            decoder,
            joiner,
            tokens,
        } => {
            config.model_config.transducer = sherpa_onnx::OfflineTransducerModelConfig {
                encoder: Some(display_path(encoder)),
                decoder: Some(display_path(decoder)),
                joiner: Some(display_path(joiner)),
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
        SherpaOnnxOfflineModelLayout::Dolphin { model, tokens } => {
            config.model_config.dolphin = sherpa_onnx::OfflineDolphinModelConfig {
                model: Some(display_path(model)),
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
        SherpaOnnxOfflineModelLayout::Paraformer { model, tokens } => {
            config.model_config.paraformer = sherpa_onnx::OfflineParaformerModelConfig {
                model: Some(display_path(model)),
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
        SherpaOnnxOfflineModelLayout::SenseVoice {
            model,
            tokens,
            language,
            use_itn,
        } => {
            config.model_config.sense_voice = sherpa_onnx::OfflineSenseVoiceModelConfig {
                model: Some(display_path(model)),
                language: Some(language.clone()),
                use_itn: *use_itn,
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
        SherpaOnnxOfflineModelLayout::Qwen3Asr {
            conv_frontend,
            encoder,
            decoder,
            tokenizer,
            max_total_len,
            max_new_tokens,
            temperature,
            top_p,
            seed,
            hotwords,
        } => {
            config.model_config.qwen3_asr = sherpa_onnx::OfflineQwen3ASRModelConfig {
                conv_frontend: Some(display_path(conv_frontend)),
                encoder: Some(display_path(encoder)),
                decoder: Some(display_path(decoder)),
                tokenizer: Some(display_path(tokenizer)),
                max_total_len: *max_total_len,
                max_new_tokens: *max_new_tokens,
                temperature: *temperature,
                top_p: *top_p,
                seed: *seed,
                hotwords: hotwords.as_deref().map(display_path),
            };
        }
        SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor,
            encoder,
            uncached_decoder,
            cached_decoder,
            tokens,
        } => {
            config.model_config.moonshine = sherpa_onnx::OfflineMoonshineModelConfig {
                preprocessor: Some(display_path(preprocessor)),
                encoder: Some(display_path(encoder)),
                uncached_decoder: Some(display_path(uncached_decoder)),
                cached_decoder: Some(display_path(cached_decoder)),
                merged_decoder: None,
            };
            config.model_config.tokens = Some(display_path(tokens));
        }
    }
    config
}

#[cfg(feature = "sherpa-onnx-backend")]
impl AsrBackend for SherpaOnnxBackend {
    fn describe(&self) -> BackendDescriptor {
        self.descriptor.clone()
    }

    fn create_session(
        &self,
        _context: RecognitionContext,
    ) -> Result<Box<dyn RecognitionSession>, AsrError> {
        match &self.runtime {
            SherpaOnnxRuntime::Offline(runtime) => Ok(Box::new(SherpaOnnxRecognitionSession {
                recognizer: std::sync::Arc::clone(&runtime.recognizer),
                vad: runtime.vad.as_ref().map(std::sync::Arc::clone),
                pcm: vinput_audio::PcmSpec::default(),
                samples: Vec::new(),
                events: Vec::new(),
                finished: false,
                cancelled: false,
            })),
            SherpaOnnxRuntime::Online(runtime) => Ok(runtime.create_session()),
        }
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
struct SherpaOnnxRecognitionSession {
    recognizer: std::sync::Arc<sherpa_onnx::OfflineRecognizer>,
    vad: Option<std::sync::Arc<std::sync::Mutex<crate::sherpa_vad::SherpaOnnxVadTrimmer>>>,
    pcm: vinput_audio::PcmSpec,
    samples: Vec<i16>,
    events: Vec<RecognitionEvent>,
    finished: bool,
    cancelled: bool,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl std::fmt::Debug for SherpaOnnxRecognitionSession {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SherpaOnnxRecognitionSession")
            .field("pcm", &self.pcm)
            .field("sample_count", &self.samples.len())
            .field("finished", &self.finished)
            .field("cancelled", &self.cancelled)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl RecognitionSession for SherpaOnnxRecognitionSession {
    fn push_pcm(&mut self, pcm: &vinput_audio::PcmBuffer) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        let next_pcm = pcm.spec();
        if !self.samples.is_empty() && self.pcm != next_pcm {
            return Err(AsrError::Backend(format!(
                "sherpa-onnx PCM spec changed from {} Hz/{} channel(s) to {} Hz/{} channel(s)",
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
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.samples.extend_from_slice(samples);
        Ok(())
    }

    fn finish(&mut self) -> Result<(), AsrError> {
        if self.cancelled {
            return Err(AsrError::Cancelled);
        }
        if self.finished {
            return Err(AsrError::AlreadyFinished);
        }
        self.finished = true;
        let stream = self.recognizer.create_stream();
        let mut samples = self
            .samples
            .iter()
            .map(|sample| f32::from(*sample) / f32::from(i16::MAX))
            .collect::<Vec<_>>();
        if self.pcm.channels == 1
            && let Some(vad) = &self.vad
        {
            samples = vad
                .lock()
                .map_err(|_| AsrError::Backend("sherpa-onnx VAD lock poisoned".to_owned()))?
                .trim(&samples, self.pcm.sample_rate_hz);
        }
        let sample_rate = i32::try_from(self.pcm.sample_rate_hz).map_err(|_| {
            AsrError::Backend(format!(
                "sherpa-onnx sample rate {} does not fit i32",
                self.pcm.sample_rate_hz
            ))
        })?;
        stream.accept_waveform(sample_rate, &samples);
        self.recognizer.decode(&stream);
        let result = stream.get_result().ok_or_else(|| {
            AsrError::Backend("sherpa-onnx recognizer returned no result".to_owned())
        })?;
        let text = result.text.trim().to_owned();
        if text.is_empty() {
            return Err(AsrError::Backend(
                "sherpa-onnx recognizer returned empty text".to_owned(),
            ));
        }
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
