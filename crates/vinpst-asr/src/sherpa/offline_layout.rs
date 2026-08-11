mod moonshine;

use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    InferredOfflineLayout, SherpaOnnxModelPathError, SherpaOnnxOfflineModelLayout,
    SherpaOnnxOfflineSettings, SherpaOnnxOfflineSingleModelKind,
};

pub(super) fn infer_offline_layout(
    model_dir: &Path,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    if let Some(inferred) = infer_offline_layout_from_metadata(model_dir)? {
        return Ok(inferred);
    }
    Ok(InferredOfflineLayout {
        layout: infer_sense_voice_layout_from_files(model_dir, "auto", true)?,
        settings: default_offline_settings("sense_voice"),
        source: "files".to_owned(),
        metadata_path: None,
    })
}

fn infer_offline_layout_from_metadata(
    model_dir: &Path,
) -> Result<Option<InferredOfflineLayout>, SherpaOnnxModelPathError> {
    let metadata_path = model_dir.join("vinpst-model.json");
    if !metadata_path.is_file() {
        return Ok(None);
    }
    let metadata_text = fs::read_to_string(&metadata_path).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: error.kind().to_string(),
        }
    })?;
    let metadata = serde_json::from_str::<serde_json::Value>(&metadata_text).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: error.to_string(),
        }
    })?;
    let family = metadata_family(&metadata, model_dir)?;
    if !is_supported_offline_family(family) {
        return Err(SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: display_path(model_dir),
            family: family.to_owned(),
        });
    }
    let settings = parse_offline_settings(model_dir, &metadata, &metadata_path, family)?;
    let runtime_language = metadata_runtime_language_hint(&metadata, family);
    let runtime_language = runtime_language.as_deref();
    let inferred = match family {
        "transducer" | "nemo_transducer" => infer_offline_transducer_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            family,
        ),
        "dolphin" => infer_single_model_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            "dolphin",
            |model, tokens| SherpaOnnxOfflineModelLayout::Dolphin { model, tokens },
        ),
        "paraformer" => {
            infer_paraformer_layout_from_metadata(model_dir, &metadata, metadata_path, settings)
        }
        "whisper" => infer_whisper_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            runtime_language,
        ),
        "zipformer_ctc" | "fire_red_asr_ctc" | "nemo_ctc" | "wenet_ctc" | "tdnn"
        | "omnilingual" | "medasr" => infer_single_model_ctc_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            family,
        ),
        "telespeech_ctc" => {
            infer_telespeech_layout_from_metadata(model_dir, &metadata, metadata_path, settings)
        }
        "fire_red_asr" => {
            infer_fire_red_asr_layout_from_metadata(model_dir, &metadata, metadata_path, settings)
        }
        "canary" => infer_canary_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            runtime_language,
        ),
        "funasr_nano" => infer_funasr_nano_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            runtime_language,
        ),
        "qwen3_asr" => {
            infer_qwen3_asr_layout_from_metadata(model_dir, &metadata, metadata_path, settings)
        }
        "moonshine" | "moonshine_v1" => moonshine::infer_moonshine_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
        ),
        "sense_voice" => infer_sense_voice_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path,
            settings,
            runtime_language,
        ),
        _ => unreachable!("validated offline model family"),
    }?;
    Ok(Some(inferred))
}

fn is_supported_offline_family(family: &str) -> bool {
    matches!(
        family,
        "transducer"
            | "nemo_transducer"
            | "dolphin"
            | "paraformer"
            | "whisper"
            | "zipformer_ctc"
            | "fire_red_asr_ctc"
            | "nemo_ctc"
            | "wenet_ctc"
            | "tdnn"
            | "omnilingual"
            | "medasr"
            | "telespeech_ctc"
            | "fire_red_asr"
            | "canary"
            | "funasr_nano"
            | "qwen3_asr"
            | "moonshine"
            | "moonshine_v1"
            | "sense_voice"
    )
}

fn metadata_runtime_language_hint(metadata: &serde_json::Value, family: &str) -> Option<String> {
    let family_config = metadata
        .pointer(&format!("/model/{family}"))
        .and_then(serde_json::Value::as_object);
    for field in ["language", "src_lang"] {
        if let Some(value) = family_config
            .and_then(|config| config.get(field))
            .and_then(serde_json::Value::as_str)
        {
            return Some(value.to_owned());
        }
    }
    metadata
        .get("language")
        .and_then(serde_json::Value::as_str)
        .filter(|value| is_runtime_language_hint(value))
        .map(str::to_owned)
}

fn is_runtime_language_hint(value: &str) -> bool {
    !value.is_empty()
        && value != "multilingual"
        && !value.contains('_')
        && value
            .chars()
            .all(|character| character.is_ascii_lowercase() || character == '-' || character == '_')
}

fn metadata_family<'a>(
    metadata: &'a serde_json::Value,
    model_dir: &Path,
) -> Result<&'a str, SherpaOnnxModelPathError> {
    metadata
        .pointer("/family")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            metadata
                .pointer("/model_type")
                .and_then(serde_json::Value::as_str)
        })
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        })
}

fn infer_sense_voice_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    runtime_language: Option<&str>,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let sense_voice = metadata.pointer("/model/sense_voice");
    let model = sense_voice
        .and_then(|value| value.get("model"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| validated_metadata_asset(model_dir, value, "sense_voice", "model", false))
        .transpose()?
        .or_else(|| find_sense_voice_model_file(model_dir))
        .ok_or_else(|| SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        })?;
    if !model.is_file() {
        return Err(SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        });
    }
    let tokens = metadata
        .pointer("/model/tokens")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map_or_else(
            || Ok(model_dir.join("tokens.txt")),
            |value| validated_metadata_asset(model_dir, value, "sense_voice", "tokens", false),
        )?;
    if !tokens.is_file() {
        return Err(SherpaOnnxModelPathError::MissingTokensFile {
            path: display_path(model_dir),
        });
    }
    let language = sense_voice
        .and_then(|value| value.get("language"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or(runtime_language)
        .unwrap_or_default()
        .to_owned();
    let use_itn = sense_voice
        .and_then(|value| value.get("use_itn"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(true);
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::SenseVoice {
            model,
            tokens,
            language,
            use_itn,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_offline_transducer_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    family: &str,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let pointer = format!("/model/{family}");
    let transducer = metadata
        .pointer(&pointer)
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: format!("missing object `{pointer}`"),
        })?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Transducer {
            encoder: required_model_asset(model_dir, transducer, family, "encoder")?,
            decoder: required_model_asset(model_dir, transducer, family, "decoder")?,
            joiner: required_model_asset(model_dir, transducer, family, "joiner")?,
            tokens: metadata_asset_path(model_dir, metadata, "/model/tokens", family, "tokens")?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_single_model_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    family: &str,
    layout: impl FnOnce(PathBuf, PathBuf) -> SherpaOnnxOfflineModelLayout,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let family_config = metadata
        .pointer(&format!("/model/{family}"))
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: format!("missing object `/model/{family}`"),
        })?;
    Ok(InferredOfflineLayout {
        layout: layout(
            required_model_asset(model_dir, family_config, family, "model")?,
            metadata_asset_path(model_dir, metadata, "/model/tokens", family, "tokens")?,
        ),
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_paraformer_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    infer_single_model_layout_from_metadata(
        model_dir,
        metadata,
        metadata_path,
        settings,
        "paraformer",
        |model, tokens| SherpaOnnxOfflineModelLayout::Paraformer { model, tokens },
    )
}

fn infer_whisper_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    runtime_language: Option<&str>,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let language = metadata_optional_string(metadata, "/model/whisper/language")
        .or_else(|| runtime_language.map(str::to_owned))
        .unwrap_or_default();
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Whisper {
            encoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/whisper/encoder",
                "whisper",
                "encoder",
            )?,
            decoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/whisper/decoder",
                "whisper",
                "decoder",
            )?,
            tokens: metadata_asset_path(model_dir, metadata, "/model/tokens", "whisper", "tokens")?,
            language,
            task: metadata_optional_string(metadata, "/model/whisper/task")
                .unwrap_or_else(|| "transcribe".to_owned()),
            tail_paddings: metadata_i32(
                metadata,
                "/model/whisper/tail_paddings",
                -1,
                &metadata_path,
            )?,
            enable_token_timestamps: metadata_boolish(
                metadata,
                "/model/whisper/enable_token_timestamps",
                false,
                &metadata_path,
            )?,
            enable_segment_timestamps: metadata_boolish(
                metadata,
                "/model/whisper/enable_segment_timestamps",
                false,
                &metadata_path,
            )?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_single_model_ctc_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    family: &str,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let native_family = match family {
        "zipformer_ctc" => SherpaOnnxOfflineSingleModelKind::ZipformerCtc,
        "fire_red_asr_ctc" => SherpaOnnxOfflineSingleModelKind::FireRedAsrCtc,
        "nemo_ctc" => SherpaOnnxOfflineSingleModelKind::NemoCtc,
        "wenet_ctc" => SherpaOnnxOfflineSingleModelKind::WenetCtc,
        "tdnn" => SherpaOnnxOfflineSingleModelKind::Tdnn,
        "omnilingual" => SherpaOnnxOfflineSingleModelKind::Omnilingual,
        "medasr" => SherpaOnnxOfflineSingleModelKind::MedAsr,
        _ => unreachable!("validated single-model offline family"),
    };
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::SingleModel {
            family: native_family,
            model: metadata_asset_path(
                model_dir,
                metadata,
                &format!("/model/{family}/model"),
                family,
                "model",
            )?,
            tokens: metadata_asset_path(model_dir, metadata, "/model/tokens", family, "tokens")?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_telespeech_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::SingleModel {
            family: SherpaOnnxOfflineSingleModelKind::TelespeechCtc,
            model: metadata_asset_path(
                model_dir,
                metadata,
                "/model/telespeech_ctc",
                "telespeech_ctc",
                "telespeech_ctc",
            )?,
            tokens: metadata_asset_path(
                model_dir,
                metadata,
                "/model/tokens",
                "telespeech_ctc",
                "tokens",
            )?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_fire_red_asr_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::FireRedAsr {
            encoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/fire_red_asr/encoder",
                "fire_red_asr",
                "encoder",
            )?,
            decoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/fire_red_asr/decoder",
                "fire_red_asr",
                "decoder",
            )?,
            tokens: metadata_asset_path(
                model_dir,
                metadata,
                "/model/tokens",
                "fire_red_asr",
                "tokens",
            )?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_canary_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    runtime_language: Option<&str>,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let fallback = runtime_language.unwrap_or_default();
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Canary {
            encoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/canary/encoder",
                "canary",
                "encoder",
            )?,
            decoder: metadata_asset_path(
                model_dir,
                metadata,
                "/model/canary/decoder",
                "canary",
                "decoder",
            )?,
            tokens: metadata_asset_path(model_dir, metadata, "/model/tokens", "canary", "tokens")?,
            src_lang: metadata_optional_string(metadata, "/model/canary/src_lang")
                .unwrap_or_else(|| fallback.to_owned()),
            tgt_lang: metadata_optional_string(metadata, "/model/canary/tgt_lang")
                .unwrap_or_else(|| fallback.to_owned()),
            use_pnc: metadata_boolish(metadata, "/model/canary/use_pnc", false, &metadata_path)?,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_funasr_nano_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
    runtime_language: Option<&str>,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let tokenizer_value = metadata_optional_string(metadata, "/model/funasr_nano/tokenizer")
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing string `/model/funasr_nano/tokenizer`".to_owned(),
        })?;
    let tokenizer = validated_metadata_asset(
        model_dir,
        &tokenizer_value,
        "funasr_nano",
        "tokenizer",
        true,
    )?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::FunAsrNano {
            encoder_adaptor: metadata_asset_path(
                model_dir,
                metadata,
                "/model/funasr_nano/encoder_adapter",
                "funasr_nano",
                "encoder_adapter",
            )?,
            llm: metadata_asset_path(
                model_dir,
                metadata,
                "/model/funasr_nano/llm",
                "funasr_nano",
                "llm",
            )?,
            embedding: metadata_asset_path(
                model_dir,
                metadata,
                "/model/funasr_nano/embedding",
                "funasr_nano",
                "embedding",
            )?,
            tokenizer,
            max_new_tokens: metadata_positive_i32(
                metadata,
                "/model/funasr_nano/max_new_tokens",
                1024,
                &metadata_path,
            )?,
            temperature: metadata_finite_f32(
                metadata,
                "/model/funasr_nano/temperature",
                1.0,
                &metadata_path,
            )?,
            top_p: metadata_finite_f32(metadata, "/model/funasr_nano/top_p", 0.9, &metadata_path)?,
            seed: metadata_i32(metadata, "/model/funasr_nano/seed", 0, &metadata_path)?,
            language: metadata_optional_string(metadata, "/model/funasr_nano/language")
                .or_else(|| runtime_language.map(str::to_owned))
                .unwrap_or_default(),
            itn: metadata_boolish(metadata, "/model/funasr_nano/itn", false, &metadata_path)?,
            system_prompt: metadata_optional_string(metadata, "/model/funasr_nano/system_prompt"),
            user_prompt: metadata_optional_string(metadata, "/model/funasr_nano/user_prompt"),
            hotwords: metadata_optional_string(metadata, "/model/funasr_nano/hotwords"),
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_qwen3_asr_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let qwen3 = metadata
        .pointer("/model/qwen3_asr")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing object `/model/qwen3_asr`".to_owned(),
        })?;
    let conv_frontend = required_qwen3_asset(model_dir, qwen3, "conv_frontend", false)?;
    let encoder = required_qwen3_asset(model_dir, qwen3, "encoder", false)?;
    let decoder = required_qwen3_asset(model_dir, qwen3, "decoder", false)?;
    let tokenizer = required_qwen3_asset(model_dir, qwen3, "tokenizer", true)?;
    let hotwords = optional_qwen3_asset(model_dir, qwen3, "hotwords")?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Qwen3Asr {
            conv_frontend,
            encoder,
            decoder,
            tokenizer,
            max_total_len: qwen3_i32(qwen3, "max_total_len", 4096, &metadata_path)?,
            max_new_tokens: qwen3_i32(qwen3, "max_new_tokens", 1024, &metadata_path)?,
            temperature: qwen3_f32(qwen3, "temperature", 1.0, &metadata_path)?,
            top_p: qwen3_f32(qwen3, "top_p", 0.9, &metadata_path)?,
            seed: qwen3_i32(qwen3, "seed", 0, &metadata_path)?,
            hotwords,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

pub(crate) fn validated_metadata_asset(
    model_dir: &Path,
    value: &str,
    family: &str,
    asset: &str,
    allow_directory: bool,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let path = resolve_against(model_dir, value);
    let valid_shape = if allow_directory {
        path.exists()
    } else {
        path.is_file()
    };
    if !valid_shape {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: asset.to_owned(),
            path: display_path(&path),
        });
    }
    let canonical_root = fs::canonicalize(model_dir).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinpst-model.json")),
            message: format!("failed to canonicalize model root: {}", error.kind()),
        }
    })?;
    let canonical_path = fs::canonicalize(&path).map_err(|error| {
        SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinpst-model.json")),
            message: format!("failed to canonicalize model asset: {}", error.kind()),
        }
    })?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(SherpaOnnxModelPathError::ModelAssetEscapesRoot {
            family: family.to_owned(),
            asset: asset.to_owned(),
            model_root: display_path(&canonical_root),
            path: display_path(&canonical_path),
        });
    }
    Ok(path)
}

fn required_model_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    family: &str,
    field: &str,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinpst-model.json")),
            message: format!("missing string `/model/{family}/{field}`"),
        })?;
    validated_metadata_asset(model_dir, value, family, field, false)
}

fn metadata_asset_path(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
    field: &str,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = metadata
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinpst-model.json")),
            message: format!("missing string `{pointer}`"),
        })?;
    validated_metadata_asset(model_dir, value, family, field, false)
}

fn default_offline_settings(family: &str) -> SherpaOnnxOfflineSettings {
    SherpaOnnxOfflineSettings {
        num_threads: 4,
        provider: "cpu".to_owned(),
        debug: false,
        model_type: Some(family.to_owned()),
        modeling_unit: Some("cjkchar".to_owned()),
        bpe_vocab: None,
        telespeech_ctc: None,
        sample_rate: 16_000,
        feature_dim: 80,
        lm_model: None,
        lm_scale: 0.5,
        supports_hotwords: false,
        decoding_method: "greedy_search".to_owned(),
        max_active_paths: 4,
        hotwords_file: None,
        hotwords_score: 1.5,
        rule_fsts: None,
        rule_fars: None,
        blank_penalty: 0.0,
        homophone_lexicon: None,
        homophone_rule_fsts: None,
    }
}

fn parse_offline_settings(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<SherpaOnnxOfflineSettings, SherpaOnnxModelPathError> {
    let mut settings = default_offline_settings(family);
    settings.supports_hotwords =
        metadata_boolish(metadata, "/supports_hotwords", false, metadata_path)?;
    parse_offline_model_settings(&mut settings, model_dir, metadata, metadata_path, family)?;
    parse_offline_recognizer_settings(&mut settings, model_dir, metadata, metadata_path, family)?;
    Ok(settings)
}

fn parse_offline_model_settings(
    settings: &mut SherpaOnnxOfflineSettings,
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<(), SherpaOnnxModelPathError> {
    settings.num_threads = metadata_positive_i32(
        metadata,
        "/model/num_threads",
        settings.num_threads,
        metadata_path,
    )?;
    if let Some(provider) = metadata_optional_string(metadata, "/model/provider") {
        settings.provider = provider;
    }
    settings.debug = metadata_boolish(metadata, "/model/debug", settings.debug, metadata_path)?;
    if let Some(model_type) = metadata_optional_string(metadata, "/model/model_type") {
        settings.model_type = Some(model_type);
    }
    if let Some(modeling_unit) = metadata_optional_string(metadata, "/model/modeling_unit") {
        settings.modeling_unit = Some(modeling_unit);
    }
    settings.bpe_vocab =
        metadata_optional_file(model_dir, metadata, "/model/bpe_vocab", family, "bpe_vocab")?;
    settings.telespeech_ctc = metadata_optional_file(
        model_dir,
        metadata,
        "/model/telespeech_ctc",
        family,
        "telespeech_ctc",
    )?;
    Ok(())
}

fn parse_offline_recognizer_settings(
    settings: &mut SherpaOnnxOfflineSettings,
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: &Path,
    family: &str,
) -> Result<(), SherpaOnnxModelPathError> {
    settings.sample_rate = metadata_positive_i32(
        metadata,
        "/recognizer/feat_config/sample_rate",
        settings.sample_rate,
        metadata_path,
    )?;
    settings.feature_dim = metadata_positive_i32(
        metadata,
        "/recognizer/feat_config/feature_dim",
        settings.feature_dim,
        metadata_path,
    )?;
    settings.lm_model = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/lm_config/model",
        family,
        "lm_model",
    )?;
    settings.lm_scale = metadata_finite_f32(
        metadata,
        "/recognizer/lm_config/scale",
        settings.lm_scale,
        metadata_path,
    )?;
    if let Some(decoding_method) = metadata_optional_string(metadata, "/recognizer/decoding_method")
    {
        settings.decoding_method = decoding_method;
    }
    settings.max_active_paths = metadata_positive_i32(
        metadata,
        "/recognizer/max_active_paths",
        settings.max_active_paths,
        metadata_path,
    )?;
    settings.hotwords_file = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hotwords_file",
        family,
        "hotwords_file",
    )?;
    settings.hotwords_score = metadata_finite_f32(
        metadata,
        "/recognizer/hotwords_score",
        settings.hotwords_score,
        metadata_path,
    )?;
    settings.rule_fsts = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/rule_fsts",
        family,
        "rule_fsts",
    )?;
    settings.rule_fars = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/rule_fars",
        family,
        "rule_fars",
    )?;
    settings.blank_penalty = metadata_finite_f32(
        metadata,
        "/recognizer/blank_penalty",
        settings.blank_penalty,
        metadata_path,
    )?;
    settings.homophone_lexicon = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hr/lexicon",
        family,
        "homophone_lexicon",
    )?;
    settings.homophone_rule_fsts = metadata_optional_file(
        model_dir,
        metadata,
        "/recognizer/hr/rule_fsts",
        family,
        "homophone_rule_fsts",
    )?;
    Ok(())
}

fn metadata_optional_string(metadata: &serde_json::Value, pointer: &str) -> Option<String> {
    metadata
        .pointer(pointer)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

fn metadata_optional_file(
    model_dir: &Path,
    metadata: &serde_json::Value,
    pointer: &str,
    family: &str,
    asset: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    let Some(value) = metadata_optional_string(metadata, pointer) else {
        return Ok(None);
    };
    validated_metadata_asset(model_dir, &value, family, asset, false).map(Some)
}

fn metadata_i32(
    metadata: &serde_json::Value,
    pointer: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be a 32-bit integer"),
        })
}

fn metadata_positive_i32(
    metadata: &serde_json::Value,
    pointer: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    let value = value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| *value > 0)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be a positive 32-bit integer"),
        })?;
    Ok(value)
}

fn metadata_boolish(
    metadata: &serde_json::Value,
    pointer: &str,
    default: bool,
    metadata_path: &Path,
) -> Result<bool, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    match (value.as_bool(), value.as_i64()) {
        (Some(value), _) => Ok(value),
        (None, Some(0)) => Ok(false),
        (None, Some(1)) => Ok(true),
        _ => Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be a boolean or 0/1"),
        }),
    }
}

fn metadata_finite_f32(
    metadata: &serde_json::Value,
    pointer: &str,
    default: f32,
    metadata_path: &Path,
) -> Result<f32, SherpaOnnxModelPathError> {
    let Some(value) = metadata.pointer(pointer) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .filter(|value| value.is_finite())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` must be finite numeric value"),
        })?;
    if value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`{pointer}` is outside the f32 range"),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    Ok(value as f32)
}

fn required_qwen3_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    allow_directory: bool,
) -> Result<PathBuf, SherpaOnnxModelPathError> {
    let value = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&model_dir.join("vinpst-model.json")),
            message: format!("missing string `/model/qwen3_asr/{field}`"),
        })?;
    validated_metadata_asset(model_dir, value, "qwen3_asr", field, allow_directory)
}

fn optional_qwen3_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    let Some(value) = config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    validated_metadata_asset(model_dir, value, "qwen3_asr", field, false).map(Some)
}

fn qwen3_i32(
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: i32,
    metadata_path: &Path,
) -> Result<i32, SherpaOnnxModelPathError> {
    let Some(value) = config.get(field) else {
        return Ok(default);
    };
    let value = value
        .as_i64()
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` must be a 32-bit integer"),
        })?;
    Ok(value)
}

fn qwen3_f32(
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: f32,
    metadata_path: &Path,
) -> Result<f32, SherpaOnnxModelPathError> {
    let Some(value) = config.get(field) else {
        return Ok(default);
    };
    let value = value
        .as_f64()
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` must be numeric"),
        })?;
    if !value.is_finite() || value < f64::from(f32::MIN) || value > f64::from(f32::MAX) {
        return Err(SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(metadata_path),
            message: format!("`/model/qwen3_asr/{field}` is outside the f32 range"),
        });
    }
    #[allow(clippy::cast_possible_truncation)]
    let value = value as f32;
    Ok(value)
}

fn infer_sense_voice_layout_from_files(
    model_dir: &Path,
    language: &str,
    use_itn: bool,
) -> Result<SherpaOnnxOfflineModelLayout, SherpaOnnxModelPathError> {
    let model = find_sense_voice_model_file(model_dir).ok_or_else(|| {
        SherpaOnnxModelPathError::UnsupportedOfflineLayout {
            path: display_path(model_dir),
        }
    })?;
    let tokens = model_dir.join("tokens.txt");
    if !tokens.is_file() {
        return Err(SherpaOnnxModelPathError::MissingTokensFile {
            path: display_path(model_dir),
        });
    }
    Ok(SherpaOnnxOfflineModelLayout::SenseVoice {
        model,
        tokens,
        language: language.to_owned(),
        use_itn,
    })
}

fn find_sense_voice_model_file(model_dir: &Path) -> Option<PathBuf> {
    ["model.int8.onnx", "model.onnx"]
        .into_iter()
        .map(|file_name| model_dir.join(file_name))
        .find(|path| path.is_file())
}

pub(super) fn resolve_against(root: &Path, value: &str) -> PathBuf {
    let path = Path::new(value);
    if path.is_absolute() {
        path.to_owned()
    } else {
        root.join(path)
    }
}

pub(super) fn reject_url_like(
    provider_id: &str,
    value: &str,
) -> Result<(), SherpaOnnxModelPathError> {
    if value.contains("://") {
        Err(SherpaOnnxModelPathError::UrlLikePath {
            provider_id: provider_id.to_owned(),
            path: value.to_owned(),
        })
    } else {
        Ok(())
    }
}

pub(super) fn display_path(path: &Path) -> String {
    path.display().to_string()
}
