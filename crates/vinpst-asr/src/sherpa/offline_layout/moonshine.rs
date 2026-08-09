use std::path::{Path, PathBuf};

use super::{display_path, metadata_asset_path, required_model_asset, validated_metadata_asset};
use crate::sherpa::{
    InferredOfflineLayout, SherpaOnnxModelPathError, SherpaOnnxOfflineModelLayout,
    SherpaOnnxOfflineSettings,
};

pub(super) fn infer_moonshine_layout_from_metadata(
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
    if !matches!(model_type, "moonshine" | "moonshine_v1" | "moonshine_v2") {
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
    let encoder = required_model_asset(model_dir, moonshine, "moonshine", "encoder")?;
    let preprocessor = optional_asset(model_dir, moonshine, "preprocessor")?;
    let uncached_decoder = optional_asset(model_dir, moonshine, "uncached_decoder")?;
    let cached_decoder = optional_asset(model_dir, moonshine, "cached_decoder")?;
    let merged_decoder = optional_asset(model_dir, moonshine, "merged_decoder")?;
    let v2_shape = preprocessor.is_none()
        && uncached_decoder.is_none()
        && cached_decoder.is_none()
        && merged_decoder.is_some();
    if model_type == "moonshine_v2" || v2_shape {
        let merged_decoder = merged_decoder.ok_or_else(|| invalid_v2(&metadata_path))?;
        if preprocessor.is_some() || uncached_decoder.is_some() || cached_decoder.is_some() {
            return Err(invalid_v2(&metadata_path));
        }
        return Ok(InferredOfflineLayout {
            layout: SherpaOnnxOfflineModelLayout::MoonshineV2 {
                encoder,
                merged_decoder,
                tokens,
            },
            settings,
            source: "metadata".to_owned(),
            metadata_path: Some(metadata_path),
        });
    }
    Ok(InferredOfflineLayout {
        layout: SherpaOnnxOfflineModelLayout::MoonshineV1 {
            preprocessor: preprocessor.ok_or_else(|| invalid_v1(&metadata_path, "preprocessor"))?,
            encoder,
            uncached_decoder: uncached_decoder
                .ok_or_else(|| invalid_v1(&metadata_path, "uncached_decoder"))?,
            cached_decoder: cached_decoder
                .ok_or_else(|| invalid_v1(&metadata_path, "cached_decoder"))?,
            merged_decoder,
            tokens,
        },
        settings,
        source: "metadata".to_owned(),
        metadata_path: Some(metadata_path),
    })
}

fn optional_asset(
    model_dir: &Path,
    config: &serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<PathBuf>, SherpaOnnxModelPathError> {
    config
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| validated_metadata_asset(model_dir, value, "moonshine", field, false))
        .transpose()
}

fn invalid_v1(metadata_path: &Path, field: &str) -> SherpaOnnxModelPathError {
    SherpaOnnxModelPathError::InvalidModelMetadata {
        path: display_path(metadata_path),
        message: format!("moonshine v1 requires `/model/moonshine/{field}`"),
    }
}

fn invalid_v2(metadata_path: &Path) -> SherpaOnnxModelPathError {
    SherpaOnnxModelPathError::InvalidModelMetadata {
        path: display_path(metadata_path),
        message: "moonshine v2 requires only encoder and merged_decoder assets".to_owned(),
    }
}
