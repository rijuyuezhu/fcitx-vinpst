//! Offline Silero VAD support for the native `sherpa-onnx` backend.

use std::{env, path::PathBuf};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use vinpst_config::VadConfig;

const SILERO_VAD_FILE_NAME: &str = "silero_vad.onnx";
const DEVELOPMENT_VAD_DIR: Option<&str> = option_env!("VINPST_SHERPA_VAD_DEVELOPMENT_DIR");
#[cfg(feature = "sherpa-onnx-backend")]
const SILERO_WINDOW_SIZE: usize = 512;
#[cfg(feature = "sherpa-onnx-backend")]
const SILERO_SAMPLE_RATE_HZ: u32 = 16_000;
#[cfg(feature = "sherpa-onnx-backend")]
const VAD_BUFFER_SIZE_SECONDS: f32 = 30.0;
#[cfg(feature = "sherpa-onnx-backend")]
const COLD_START_GUARD_MS: u32 = 500;

/// Filesystem source selected for the Silero VAD model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SherpaOnnxVadModelSource {
    /// Explicit `VINPST_SHERPA_VAD_MODEL` override.
    ExplicitEnv,
    /// User XDG data directory.
    UserData,
    /// System XDG data directory.
    SystemData,
    /// Repository asset used by development builds.
    DevelopmentAsset,
}

/// Non-mutating diagnostic view of offline Silero VAD availability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxVadProbe {
    /// Whether VAD is enabled in config.
    pub enabled: bool,
    /// Whether a regular Silero model file was resolved.
    pub available: bool,
    /// Resolved regular model file, when available.
    pub model: Option<PathBuf>,
    /// Requested explicit model path, including a missing override.
    pub requested_model: Option<PathBuf>,
    /// Source used or requested for the model.
    pub source: Option<SherpaOnnxVadModelSource>,
    /// Speech probability threshold.
    pub threshold: f32,
    /// Minimum accepted speech duration in seconds.
    pub min_speech_duration: f32,
    /// Minimum silence duration used to close a segment, in seconds.
    pub min_silence_duration: f32,
    /// Padding added before and after each detected segment.
    pub speech_pad_ms: u32,
}

impl SherpaOnnxVadProbe {
    /// Inspects VAD config and model resolution without loading ONNX runtime state.
    #[must_use]
    pub fn inspect(config: &VadConfig) -> Self {
        let resolved = config.enabled.then(resolve_silero_model).flatten();
        let explicit = config
            .enabled
            .then(|| env::var_os("VINPST_SHERPA_VAD_MODEL"))
            .flatten()
            .map(PathBuf::from);
        Self {
            enabled: config.enabled,
            available: resolved.is_some(),
            model: resolved.as_ref().map(|resolved| resolved.path.clone()),
            requested_model: explicit.clone(),
            source: resolved
                .as_ref()
                .map(|resolved| resolved.source)
                .or_else(|| explicit.map(|_| SherpaOnnxVadModelSource::ExplicitEnv)),
            threshold: config.threshold,
            min_speech_duration: config.min_speech_duration,
            min_silence_duration: config.min_silence_duration,
            speech_pad_ms: config.speech_pad_ms,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolvedSileroModel {
    path: PathBuf,
    source: SherpaOnnxVadModelSource,
}

/// Resolved native Silero VAD configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SherpaOnnxVadPlan {
    /// Resolved Silero ONNX model.
    pub model: PathBuf,
    /// Speech probability threshold.
    pub threshold: f32,
    /// Minimum accepted speech duration in seconds.
    pub min_speech_duration: f32,
    /// Minimum silence duration used to close a segment, in seconds.
    pub min_silence_duration: f32,
    /// Padding added before and after each detected segment.
    pub speech_pad_ms: u32,
}

impl SherpaOnnxVadPlan {
    /// Resolves the optional legacy Silero model for an offline ASR config.
    #[must_use]
    pub fn resolve(config: &VadConfig) -> Option<Self> {
        if !config.enabled {
            return None;
        }
        let model = resolve_silero_model()?.path;
        Some(Self {
            model,
            threshold: config.threshold,
            min_speech_duration: config.min_speech_duration,
            min_silence_duration: config.min_silence_duration,
            speech_pad_ms: config.speech_pad_ms,
        })
    }
}

fn resolve_silero_model() -> Option<ResolvedSileroModel> {
    if let Some(explicit) = env::var_os("VINPST_SHERPA_VAD_MODEL") {
        return regular_file(
            PathBuf::from(explicit),
            SherpaOnnxVadModelSource::ExplicitEnv,
        );
    }

    if let Some(data_home) = env::var_os("XDG_DATA_HOME") {
        if let Some(path) = regular_file(
            PathBuf::from(data_home)
                .join("fcitx-vinpst/vad")
                .join(SILERO_VAD_FILE_NAME),
            SherpaOnnxVadModelSource::UserData,
        ) {
            return Some(path);
        }
    } else if let Some(home) = env::var_os("HOME")
        && let Some(path) = regular_file(
            PathBuf::from(home)
                .join(".local/share/fcitx-vinpst/vad")
                .join(SILERO_VAD_FILE_NAME),
            SherpaOnnxVadModelSource::UserData,
        )
    {
        return Some(path);
    }

    let data_dirs =
        env::var_os("XDG_DATA_DIRS").unwrap_or_else(|| "/usr/local/share:/usr/share".into());
    for data_dir in env::split_paths(&data_dirs) {
        if let Some(path) = regular_file(
            data_dir.join("fcitx-vinpst/vad").join(SILERO_VAD_FILE_NAME),
            SherpaOnnxVadModelSource::SystemData,
        ) {
            return Some(path);
        }
    }

    development_silero_model()
}

fn development_silero_model() -> Option<ResolvedSileroModel> {
    let directory = DEVELOPMENT_VAD_DIR.filter(|directory| !directory.is_empty())?;
    regular_file(
        PathBuf::from(directory).join(SILERO_VAD_FILE_NAME),
        SherpaOnnxVadModelSource::DevelopmentAsset,
    )
}

fn regular_file(path: PathBuf, source: SherpaOnnxVadModelSource) -> Option<ResolvedSileroModel> {
    path.is_file()
        .then_some(ResolvedSileroModel { path, source })
}

#[cfg(feature = "sherpa-onnx-backend")]
pub(crate) struct SherpaOnnxVadTrimmer {
    detector: sherpa_onnx::VoiceActivityDetector,
    speech_pad_ms: u32,
}

#[cfg(feature = "sherpa-onnx-backend")]
impl std::fmt::Debug for SherpaOnnxVadTrimmer {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SherpaOnnxVadTrimmer")
            .field("speech_pad_ms", &self.speech_pad_ms)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "sherpa-onnx-backend")]
impl SherpaOnnxVadTrimmer {
    pub(crate) fn create(plan: &SherpaOnnxVadPlan) -> Option<Self> {
        let config = sherpa_onnx::VadModelConfig {
            silero_vad: sherpa_onnx::SileroVadModelConfig {
                model: Some(plan.model.display().to_string()),
                threshold: plan.threshold,
                min_silence_duration: plan.min_silence_duration,
                min_speech_duration: plan.min_speech_duration,
                window_size: i32::try_from(SILERO_WINDOW_SIZE).ok()?,
                max_speech_duration: 0.0,
            },
            sample_rate: i32::try_from(SILERO_SAMPLE_RATE_HZ).ok()?,
            num_threads: 1,
            provider: Some("cpu".to_owned()),
            debug: false,
            ..sherpa_onnx::VadModelConfig::default()
        };
        let detector =
            sherpa_onnx::VoiceActivityDetector::create(&config, VAD_BUFFER_SIZE_SECONDS)?;
        Some(Self {
            detector,
            speech_pad_ms: plan.speech_pad_ms,
        })
    }

    pub(crate) fn trim(&mut self, samples: &[f32], sample_rate_hz: u32) -> Vec<f32> {
        if samples.is_empty() || sample_rate_hz != SILERO_SAMPLE_RATE_HZ {
            return samples.to_vec();
        }

        self.detector.reset();
        let mut chunks = samples.chunks_exact(SILERO_WINDOW_SIZE);
        for chunk in &mut chunks {
            self.detector.accept_waveform(chunk);
        }
        let tail = chunks.remainder();
        if !tail.is_empty() {
            let mut padded = vec![0.0; SILERO_WINDOW_SIZE];
            padded[..tail.len()].copy_from_slice(tail);
            self.detector.accept_waveform(&padded);
        }
        self.detector.flush();

        let mut segments = Vec::new();
        while let Some(segment) = self.detector.front() {
            let start = usize::try_from(segment.start()).unwrap_or_default();
            let length = usize::try_from(segment.n()).unwrap_or_default();
            if length > 0 {
                segments.push((start, length));
            }
            drop(segment);
            self.detector.pop();
        }

        let padding_samples =
            usize::try_from(u64::from(self.speech_pad_ms) * u64::from(sample_rate_hz) / 1_000)
                .unwrap_or(usize::MAX);
        let leading_guard_samples =
            usize::try_from(u64::from(COLD_START_GUARD_MS) * u64::from(sample_rate_hz) / 1_000)
                .unwrap_or(usize::MAX);
        let ranges = padded_segment_ranges(
            samples.len(),
            &segments,
            padding_samples,
            leading_guard_samples,
        );
        let trimmed = collect_ranges(samples, &ranges);
        if trimmed.is_empty() {
            eprintln!("vinpst: VAD found no speech, returning original audio");
            samples.to_vec()
        } else {
            let sample_rate = usize::try_from(sample_rate_hz).unwrap_or(1);
            let first_start = ranges.first().map_or(0, |range| range.start);
            let last_end = ranges.last().map_or(samples.len(), |range| range.end);
            eprintln!(
                "vinpst: VAD trimmed {} -> {} samples leading_removed_ms={} trailing_removed_ms={} pad_ms={}",
                samples.len(),
                trimmed.len(),
                first_start.saturating_mul(1_000) / sample_rate,
                samples.len().saturating_sub(last_end).saturating_mul(1_000) / sample_rate,
                self.speech_pad_ms,
            );
            trimmed
        }
    }
}

#[cfg(any(feature = "sherpa-onnx-backend", test))]
fn padded_segment_ranges(
    sample_count: usize,
    segments: &[(usize, usize)],
    padding_samples: usize,
    leading_guard_samples: usize,
) -> Vec<std::ops::Range<usize>> {
    segments
        .iter()
        .enumerate()
        .filter_map(|(index, &(start, length))| {
            let mut padded_start = start.saturating_sub(padding_samples);
            if index == 0 && padded_start <= leading_guard_samples {
                padded_start = 0;
            }
            let padded_end = start
                .saturating_add(length)
                .saturating_add(padding_samples)
                .min(sample_count);
            (padded_start < padded_end).then_some(padded_start..padded_end)
        })
        .collect()
}

#[cfg(any(feature = "sherpa-onnx-backend", test))]
fn collect_ranges(samples: &[f32], ranges: &[std::ops::Range<usize>]) -> Vec<f32> {
    let mut result = Vec::new();
    for range in ranges {
        result.extend_from_slice(&samples[range.clone()]);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::{
        SherpaOnnxVadModelSource, SherpaOnnxVadPlan, collect_ranges, development_silero_model,
        padded_segment_ranges,
    };
    use vinpst_config::VadConfig;

    #[test]
    fn test_profile_resolves_the_development_vad_asset() {
        let resolved = development_silero_model().expect("test profile development VAD asset");
        assert_eq!(resolved.source, SherpaOnnxVadModelSource::DevelopmentAsset);
        assert!(resolved.path.is_file());
    }

    #[test]
    fn disabled_vad_has_no_runtime_plan() {
        let config = VadConfig {
            enabled: false,
            ..VadConfig::default()
        };
        assert!(SherpaOnnxVadPlan::resolve(&config).is_none());
    }

    #[test]
    fn segment_padding_matches_legacy_concatenation() {
        let samples = (0_u8..20).map(f32::from).collect::<Vec<_>>();
        let ranges = padded_segment_ranges(samples.len(), &[(4, 3), (12, 2)], 2, 0);
        assert_eq!(
            collect_ranges(&samples, &ranges),
            [
                2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0
            ]
        );
    }

    #[test]
    fn no_segments_leave_fallback_decision_to_runtime() {
        let ranges = padded_segment_ranges(2, &[], 300, 500);
        assert!(collect_ranges(&[1.0, 2.0], &ranges).is_empty());
    }

    #[test]
    fn cold_start_guard_preserves_short_leading_context_only() {
        let guarded = padded_segment_ranges(20_000, &[(11_300, 1_000)], 4_800, 8_000);
        assert_eq!(guarded.len(), 1);
        assert_eq!(guarded[0], 0..17_100);

        let long_silence = padded_segment_ranges(40_000, &[(30_000, 1_000)], 4_800, 8_000);
        assert_eq!(long_silence.len(), 1);
        assert_eq!(long_silence[0], 25_200..35_800);
    }
}
