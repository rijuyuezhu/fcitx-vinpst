//! Live registry v2 model metadata parsing.
//!
//! The legacy registry used by `xifan2333/vinput-registry` publishes ASR
//! models in `registry/models.json` with a top-level `items` array. This
//! module parses that live shape without replacing the older `index.json`
//! dry-run planner schema used by existing tests and smoke fixtures.

use std::collections::{BTreeMap, HashSet};

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::RegistryError;

/// Normalized sherpa-onnx model family declared by live registry metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum LiveModelFamily {
    /// Dolphin offline model family.
    Dolphin,
    /// `SenseVoice` offline model family.
    SenseVoice,
    /// Paraformer offline model family.
    Paraformer,
    /// Transducer model family used by offline and streaming models.
    Transducer,
    /// Qwen3 ASR offline model family.
    Qwen3Asr,
    /// Zipformer2 CTC streaming model family.
    Zipformer2Ctc,
    /// Moonshine offline model family.
    Moonshine,
    /// Forward-compatible family not known by this build.
    Other(String),
}

impl LiveModelFamily {
    /// Classifies one non-empty registry family string without rejecting future values.
    #[must_use]
    pub fn classify(value: &str) -> Self {
        match value.trim() {
            "dolphin" => Self::Dolphin,
            "sense_voice" => Self::SenseVoice,
            "paraformer" => Self::Paraformer,
            "transducer" => Self::Transducer,
            "qwen3_asr" => Self::Qwen3Asr,
            "zipformer2_ctc" => Self::Zipformer2Ctc,
            "moonshine" => Self::Moonshine,
            other => Self::Other(other.to_owned()),
        }
    }

    /// Returns the canonical registry family string.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Dolphin => "dolphin",
            Self::SenseVoice => "sense_voice",
            Self::Paraformer => "paraformer",
            Self::Transducer => "transducer",
            Self::Qwen3Asr => "qwen3_asr",
            Self::Zipformer2Ctc => "zipformer2_ctc",
            Self::Moonshine => "moonshine",
            Self::Other(value) => value,
        }
    }
}

/// Parsed `registry/models.json` document from the live registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveModelRegistry {
    /// Live registry schema version.
    pub version: u32,
    /// Live ASR model entries.
    #[serde(default)]
    pub items: Vec<LiveModelEntry>,
}

impl LiveModelRegistry {
    /// Parses a live registry `models.json` document.
    pub fn from_json_str(input: &str) -> Result<Self, RegistryError> {
        let registry: Self = serde_json::from_str(input)?;
        registry.validate()?;
        Ok(registry)
    }

    /// Validates stable live registry invariants used by CLI model listing and install planning.
    pub fn validate(&self) -> Result<(), RegistryError> {
        if self.version == 0 {
            return Err(RegistryError::InvalidVersion);
        }

        let mut ids = HashSet::new();
        let mut short_ids = HashSet::new();
        for item in &self.items {
            item.validate()?;
            if !ids.insert(item.id.as_str()) {
                return Err(RegistryError::DuplicateModelId(item.id.clone()));
            }
            if let Some(short_id) = non_empty_string(item.short_id.as_deref())
                && !short_ids.insert(short_id)
            {
                return Err(RegistryError::DuplicateModelShortId(short_id.to_owned()));
            }
        }

        Ok(())
    }

    /// Finds a model by full registry id.
    #[must_use]
    pub fn model(&self, id: &str) -> Option<&LiveModelEntry> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Finds a model by live registry `short_id`.
    #[must_use]
    pub fn model_by_short_id(&self, short_id: &str) -> Option<&LiveModelEntry> {
        self.items
            .iter()
            .find(|item| item.short_id.as_deref() == Some(short_id))
    }

    /// Finds a model by full id or short id.
    #[must_use]
    pub fn model_by_id_or_short_id(&self, id_or_short_id: &str) -> Option<&LiveModelEntry> {
        self.model(id_or_short_id)
            .or_else(|| self.model_by_short_id(id_or_short_id))
    }
}

/// One ASR model entry from live `registry/models.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveModelEntry {
    /// Stable full model id, for example `model.sherpa-onnx.sense-voice-...`.
    pub id: String,
    /// User-facing short id used by legacy CLI and GUI flows.
    #[serde(default)]
    pub short_id: Option<String>,
    /// Ordered mirror download URLs for the archive.
    #[serde(default)]
    pub urls: Vec<String>,
    /// Expected archive SHA-256 checksum.
    #[serde(default)]
    pub sha256: Option<String>,
    /// Declared archive size in bytes.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Declared language or language group.
    #[serde(default)]
    pub language: Option<String>,
    /// Optional inline title for registries that do not use i18n files.
    #[serde(default)]
    pub title: Option<String>,
    /// Optional inline description for registries that do not use i18n files.
    #[serde(default)]
    pub description: Option<String>,
    /// Pre-built `vinput-model.json` metadata to write after extraction.
    #[serde(default)]
    pub vinput_model: Option<LiveVinputModelMetadata>,
}

impl LiveModelEntry {
    fn validate(&self) -> Result<(), RegistryError> {
        if non_empty_string(Some(&self.id)).is_none() {
            return Err(RegistryError::EmptyId);
        }
        if self.urls.is_empty() {
            return Err(RegistryError::EmptyModelUrls(self.id.clone()));
        }
        if self
            .urls
            .iter()
            .any(|url| non_empty_string(Some(url)).is_none())
        {
            return Err(RegistryError::EmptyModelUrl(self.id.clone()));
        }
        if let Some(sha256) = &self.sha256 {
            validate_sha256(sha256)?;
        }
        Ok(())
    }

    /// Returns the model family from typed `vinput_model` metadata.
    #[must_use]
    pub fn model_family(&self) -> Option<&str> {
        self.vinput_model
            .as_ref()
            .and_then(LiveVinputModelMetadata::model_family)
    }

    /// Classifies the model family while preserving unknown future values.
    #[must_use]
    pub fn classified_model_family(&self) -> Option<LiveModelFamily> {
        self.vinput_model
            .as_ref()
            .and_then(LiveVinputModelMetadata::classified_model_family)
    }

    /// Returns the backend declared by typed `vinput_model` metadata.
    #[must_use]
    pub fn backend(&self) -> Option<&str> {
        self.vinput_model
            .as_ref()
            .and_then(|metadata| non_empty_string(metadata.backend.as_deref()))
    }

    /// Returns whether this model declares hotword support.
    #[must_use]
    pub fn supports_hotwords(&self) -> bool {
        self.vinput_model
            .as_ref()
            .is_some_and(|metadata| metadata.supports_hotwords)
    }

    /// Resolves a display title using inline text, i18n, short id, and id fallback.
    #[must_use]
    pub fn resolved_title(&self, i18n: Option<&LiveRegistryI18n>) -> String {
        self.resolved_i18n_text("title", self.title.as_deref(), i18n)
            .unwrap_or_else(|| {
                self.short_id
                    .as_deref()
                    .and_then(|short_id| non_empty_string(Some(short_id)))
                    .unwrap_or(self.id.as_str())
                    .to_owned()
            })
    }

    /// Resolves a display description using inline text and i18n fallback.
    #[must_use]
    pub fn resolved_description(&self, i18n: Option<&LiveRegistryI18n>) -> Option<String> {
        self.resolved_i18n_text("description", self.description.as_deref(), i18n)
    }

    /// Builds display metadata that can survive registry-independent installed scans.
    #[must_use]
    pub fn installed_display_metadata(
        &self,
        locale: &str,
        i18n: Option<&LiveRegistryI18n>,
    ) -> InstalledModelDisplayMetadata {
        let mut localized_titles = BTreeMap::new();
        let normalized_locale = normalize_locale_tag(locale);
        if !normalized_locale.is_empty()
            && let Some(title) = i18n.and_then(|map| map.model_text(&self.id, "title"))
        {
            localized_titles.insert(normalized_locale, title.to_owned());
        }
        InstalledModelDisplayMetadata {
            registry_id: Some(self.id.clone()),
            fallback_title: non_empty_string(self.title.as_deref()).map(str::to_owned),
            localized_titles,
        }
    }

    fn resolved_i18n_text(
        &self,
        suffix: &str,
        inline: Option<&str>,
        i18n: Option<&LiveRegistryI18n>,
    ) -> Option<String> {
        non_empty_string(inline)
            .map(str::to_owned)
            .or_else(|| i18n.and_then(|map| map.model_text(&self.id, suffix).map(str::to_owned)))
    }
}

/// Optional display metadata persisted into installed `vinput-model.json` files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct InstalledModelDisplayMetadata {
    /// Stable full registry id used independently from the managed directory name.
    #[serde(default)]
    pub registry_id: Option<String>,
    /// Inline registry title used before locale-specific fallbacks.
    #[serde(default)]
    pub fallback_title: Option<String>,
    /// Locale-specific registry titles captured during installation.
    #[serde(default)]
    pub localized_titles: BTreeMap<String, String>,
}

impl InstalledModelDisplayMetadata {
    /// Resolves a title using inline text, exact locales, language fallbacks, then no title.
    #[must_use]
    pub fn resolved_title<'a>(&'a self, locale_candidates: &[String]) -> Option<&'a str> {
        non_empty_string(self.fallback_title.as_deref()).or_else(|| {
            locale_candidates.iter().find_map(|locale| {
                let normalized = normalize_locale_tag(locale);
                self.localized_titles
                    .get(&normalized)
                    .and_then(|title| non_empty_string(Some(title)))
                    .or_else(|| {
                        normalized.split_once('_').and_then(|(language, _)| {
                            self.localized_titles
                                .get(language)
                                .and_then(|title| non_empty_string(Some(title)))
                        })
                    })
            })
        })
    }
}

/// Typed subset of live `vinput_model` metadata while preserving unknown fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveVinputModelMetadata {
    /// Runtime backend, for example `sherpa-offline`.
    #[serde(default)]
    pub backend: Option<String>,
    /// Runtime language hint.
    #[serde(default)]
    pub language: Option<String>,
    /// Declared model size in bytes.
    #[serde(default)]
    pub size_bytes: Option<u64>,
    /// Whether the model supports hotwords.
    #[serde(default)]
    pub supports_hotwords: bool,
    /// Runtime mode, for example `offline` or `streaming`.
    #[serde(default)]
    pub runtime: Option<String>,
    /// Model family used by native backend mapping, for example `sense_voice`.
    #[serde(default)]
    pub family: Option<String>,
    /// Legacy fallback family key used by some historical metadata.
    #[serde(default)]
    pub model_type: Option<String>,
    /// Recognizer configuration subtree kept as raw JSON for backend-specific mapping.
    #[serde(default)]
    pub recognizer: Option<Value>,
    /// Model-file configuration subtree kept as raw JSON for backend-specific mapping.
    #[serde(default)]
    pub model: Option<Value>,
    /// Optional installed display metadata added by the Rust installer.
    #[serde(default)]
    pub display: Option<InstalledModelDisplayMetadata>,
    /// Additional metadata fields not yet typed by Rust.
    #[serde(flatten)]
    pub extra: BTreeMap<String, Value>,
}

impl LiveVinputModelMetadata {
    /// Returns the preferred model family, matching legacy `family` then `model_type` fallback.
    #[must_use]
    pub fn model_family(&self) -> Option<&str> {
        non_empty_string(self.family.as_deref())
            .or_else(|| non_empty_string(self.model_type.as_deref()))
    }

    /// Classifies the preferred model family without rejecting future registry values.
    #[must_use]
    pub fn classified_model_family(&self) -> Option<LiveModelFamily> {
        self.model_family().map(LiveModelFamily::classify)
    }

    /// Serializes the metadata back to JSON for `vinput-model.json` materialization.
    pub fn to_raw_value(&self) -> Result<Value, RegistryError> {
        serde_json::to_value(self).map_err(RegistryError::from)
    }
}

/// Flat i18n map loaded from live registry `i18n/*.json` files.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct LiveRegistryI18n {
    /// Raw translation entries keyed by strings such as `<model-id>.title`.
    #[serde(flatten)]
    pub entries: BTreeMap<String, String>,
}

impl LiveRegistryI18n {
    /// Parses a live registry i18n JSON object.
    pub fn from_json_str(input: &str) -> Result<Self, RegistryError> {
        let entries = serde_json::from_str(input)?;
        Ok(Self { entries })
    }

    /// Merges translation layers from lowest to highest priority.
    ///
    /// Later layers replace earlier values for the same key. This matches the
    /// legacy registry order of `en_US`, preferred locale, then local overrides.
    #[must_use]
    pub fn merge_layers(layers: impl IntoIterator<Item = Self>) -> Self {
        let mut merged = Self::default();
        for layer in layers {
            merged.entries.extend(layer.entries);
        }
        merged
    }

    /// Returns whether this translation map contains no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Gets a raw translation value by key.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries
            .get(key)
            .and_then(|value| non_empty_string(Some(value)))
    }

    /// Gets a model translation using `<model-id>.<suffix>`.
    #[must_use]
    pub fn model_text(&self, model_id: &str, suffix: &str) -> Option<&str> {
        self.get(&format!("{model_id}.{suffix}"))
    }
}

fn normalize_locale_tag(input: &str) -> String {
    input
        .split(':')
        .next()
        .unwrap_or(input)
        .split('.')
        .next()
        .unwrap_or(input)
        .split('@')
        .next()
        .unwrap_or(input)
        .replace('-', "_")
}

fn non_empty_string(input: Option<&str>) -> Option<&str> {
    input.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed)
        }
    })
}

fn validate_sha256(input: &str) -> Result<(), RegistryError> {
    let valid = input.len() == 64
        && input
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte));
    if valid {
        Ok(())
    } else {
        Err(RegistryError::InvalidSha256(input.to_owned()))
    }
}
