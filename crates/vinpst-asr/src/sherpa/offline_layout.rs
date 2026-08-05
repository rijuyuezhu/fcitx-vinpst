use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{
    InferredOfflineLayout, SherpaOnnxModelPathError, SherpaOnnxOfflineModelLayout,
    SherpaOnnxOfflineSettings,
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
    let family = metadata
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
        })?;
    match family {
        "transducer" => infer_offline_transducer_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "dolphin" => infer_single_model_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
            "dolphin",
            |model, tokens| SherpaOnnxOfflineModelLayout::Dolphin { model, tokens },
        )
        .map(Some),
        "paraformer" => infer_paraformer_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "qwen3_asr" => infer_qwen3_asr_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "moonshine" | "moonshine_v1" => infer_moonshine_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        "sense_voice" => infer_sense_voice_layout_from_metadata(
            model_dir,
            &metadata,
            metadata_path.clone(),
            parse_offline_settings(model_dir, &metadata, &metadata_path, family)?,
        )
        .map(Some),
        _ => Err(SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: display_path(model_dir),
            family: family.to_owned(),
        }),
    }
}

fn infer_sense_voice_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let sense_voice = metadata.pointer("/model/sense_voice");
    let model = sense_voice
        .and_then(|value| value.get("model"))
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| resolve_against(model_dir, value))
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
            || model_dir.join("tokens.txt"),
            |value| resolve_against(model_dir, value),
        );
    if !tokens.is_file() {
        return Err(SherpaOnnxModelPathError::MissingTokensFile {
            path: display_path(model_dir),
        });
    }
    let language = sense_voice
        .and_then(|value| value.get("language"))
        .and_then(serde_json::Value::as_str)
        .or_else(|| metadata.get("language").and_then(serde_json::Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("auto")
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
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let transducer = metadata
        .pointer("/model/transducer")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing object `/model/transducer`".to_owned(),
        })?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::Transducer {
            encoder: required_model_asset(model_dir, transducer, "transducer", "encoder")?,
            decoder: required_model_asset(model_dir, transducer, "transducer", "decoder")?,
            joiner: required_model_asset(model_dir, transducer, "transducer", "joiner")?,
            tokens: metadata_asset_path(
                model_dir,
                metadata,
                "/model/tokens",
                "transducer",
                "tokens",
            )?,
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
            max_total_len: qwen3_i32(qwen3, "max_total_len", 512, &metadata_path)?,
            max_new_tokens: qwen3_i32(qwen3, "max_new_tokens", 128, &metadata_path)?,
            temperature: qwen3_f32(qwen3, "temperature", 1e-6, &metadata_path)?,
            top_p: qwen3_f32(qwen3, "top_p", 0.8, &metadata_path)?,
            seed: qwen3_i32(qwen3, "seed", 42, &metadata_path)?,
            hotwords,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn infer_moonshine_layout_from_metadata(
    model_dir: &Path,
    metadata: &serde_json::Value,
    metadata_path: PathBuf,
    settings: SherpaOnnxOfflineSettings,
) -> Result<InferredOfflineLayout, SherpaOnnxModelPathError> {
    let model_type = metadata
        .pointer("/model_type")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("moonshine_v1");
    if model_type != "moonshine_v1" && model_type != "moonshine" {
        return Err(SherpaOnnxModelPathError::UnsupportedOfflineFamily {
            path: display_path(model_dir),
            family: model_type.to_owned(),
        });
    }
    let moonshine = metadata
        .pointer("/model/moonshine")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| SherpaOnnxModelPathError::InvalidModelMetadata {
            path: display_path(&metadata_path),
            message: "missing object `/model/moonshine`".to_owned(),
        })?;
    let tokens = metadata_asset_path(model_dir, metadata, "/model/tokens", "moonshine", "tokens")?;
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor: required_model_asset(model_dir, moonshine, "moonshine", "preprocessor")?,
            encoder: required_model_asset(model_dir, moonshine, "moonshine", "encoder")?,
            uncached_decoder: required_model_asset(
                model_dir,
                moonshine,
                "moonshine",
                "uncached_decoder",
            )?,
            cached_decoder: required_model_asset(
                model_dir,
                moonshine,
                "moonshine",
                "cached_decoder",
            )?,
            tokens,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
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
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
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
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
}

fn default_offline_settings(family: &str) -> SherpaOnnxOfflineSettings {
    SherpaOnnxOfflineSettings {
        num_threads: 1,
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
    let path = resolve_against(model_dir, &value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: family.to_owned(),
            asset: asset.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(Some(path))
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
    let path = resolve_against(model_dir, value);
    let valid = if allow_directory {
        path.exists()
    } else {
        path.is_file()
    };
    if !valid {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: "qwen3_asr".to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(path)
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
    let path = resolve_against(model_dir, value);
    if !path.is_file() {
        return Err(SherpaOnnxModelPathError::MissingModelAsset {
            family: "qwen3_asr".to_owned(),
            asset: field.to_owned(),
            path: display_path(&path),
        });
    }
    Ok(Some(path))
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
