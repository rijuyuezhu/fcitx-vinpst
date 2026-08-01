use std::collections::HashSet;

use crate::{
    AsrConfig, AsrProviderConfig, AsrProviderKind, ConfigError, GlobalConfig, LlmAdapterConfig,
    LlmConfig, LlmProviderConfig, RegistryConfig, SceneDefinition, ScenesConfig, VadConfig,
    VinputConfig,
};

pub(crate) fn validate_config(config: &VinputConfig) -> Result<(), ConfigError> {
    validate_registry(&config.registry)?;
    validate_global(&config.global)?;
    validate_scenes(&config.scenes, &config.llm)?;
    validate_asr(&config.asr)?;
    validate_llm(&config.llm)?;
    Ok(())
}

fn validate_registry(registry: &RegistryConfig) -> Result<(), ConfigError> {
    let mut registry_base_urls = HashSet::new();
    for base_url in &registry.base_urls {
        if base_url.trim().is_empty() {
            return Err(ConfigError::InvalidRegistryBaseUrl(base_url.clone()));
        }
        if !registry_base_urls.insert(base_url.as_str()) {
            return Err(ConfigError::DuplicateRegistryBaseUrl(base_url.clone()));
        }
    }
    Ok(())
}

fn validate_global(global: &GlobalConfig) -> Result<(), ConfigError> {
    if global.default_language.trim().is_empty() {
        return Err(ConfigError::InvalidDefaultLanguage);
    }
    if global.capture_device.trim().is_empty() {
        return Err(ConfigError::InvalidCaptureDevice);
    }
    if !global.duck_output_volume.is_finite() || !(0.0..=1.0).contains(&global.duck_output_volume) {
        return Err(ConfigError::InvalidDuckOutputVolume(
            global.duck_output_volume,
        ));
    }
    Ok(())
}

fn validate_scenes(scenes: &ScenesConfig, llm: &LlmConfig) -> Result<(), ConfigError> {
    let mut scene_ids = HashSet::new();
    for scene in &scenes.definitions {
        validate_scene_definition(scene, &mut scene_ids, llm)?;
    }

    if !scene_ids.contains(scenes.active_scene.as_str()) {
        return Err(ConfigError::UnknownActiveScene(scenes.active_scene.clone()));
    }
    Ok(())
}

fn validate_scene_definition<'a>(
    scene: &'a SceneDefinition,
    scene_ids: &mut HashSet<&'a str>,
    llm: &LlmConfig,
) -> Result<(), ConfigError> {
    if scene.id.trim().is_empty() {
        return Err(ConfigError::InvalidSceneId(scene.id.clone()));
    }
    if scene.label.trim().is_empty() {
        return Err(ConfigError::InvalidSceneLabel(scene.id.clone()));
    }
    if !scene_ids.insert(scene.id.as_str()) {
        return Err(ConfigError::DuplicateSceneId(scene.id.clone()));
    }
    if scene.candidate_count > 32 {
        return Err(ConfigError::TooManyCandidates {
            scene_id: scene.id.clone(),
            candidate_count: scene.candidate_count,
        });
    }
    if let Some(provider_id) = &scene.provider_id {
        if provider_id.trim().is_empty() {
            return Err(ConfigError::InvalidSceneProviderId(scene.id.clone()));
        }
        if !llm
            .providers
            .iter()
            .any(|provider| provider.id == *provider_id)
        {
            return Err(ConfigError::UnknownSceneProviderId {
                scene_id: scene.id.clone(),
                provider_id: provider_id.clone(),
            });
        }
    }
    if scene
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidSceneModelId(scene.id.clone()));
    }
    if scene
        .prompt
        .as_deref()
        .is_some_and(|prompt| prompt.trim().is_empty())
    {
        return Err(ConfigError::InvalidScenePrompt(scene.id.clone()));
    }

    if scene.timeout_ms == Some(0) {
        return Err(ConfigError::InvalidSceneTimeoutMs(scene.id.clone()));
    }
    if scene.context_lines > 32 {
        return Err(ConfigError::TooManyContextLines {
            scene_id: scene.id.clone(),
            context_lines: scene.context_lines,
        });
    }
    Ok(())
}

fn validate_asr(asr: &AsrConfig) -> Result<(), ConfigError> {
    validate_vad(&asr.vad)?;
    let mut provider_ids = HashSet::new();
    for provider in &asr.providers {
        validate_asr_provider(provider, &mut provider_ids)?;
    }

    if !asr.active_provider.is_empty() && asr.active_provider.trim().is_empty() {
        return Err(ConfigError::InvalidActiveAsrProviderId);
    }

    if !asr.active_provider.is_empty()
        && !asr.providers.is_empty()
        && !provider_ids.contains(asr.active_provider.as_str())
    {
        return Err(ConfigError::UnknownActiveAsrProvider(
            asr.active_provider.clone(),
        ));
    }
    Ok(())
}

fn validate_vad(vad: &VadConfig) -> Result<(), ConfigError> {
    if !vad.threshold.is_finite() || !(0.05..=0.95).contains(&vad.threshold) {
        return Err(ConfigError::InvalidVadThreshold(vad.threshold));
    }
    if !vad.min_speech_duration.is_finite() || !(0.05..=2.0).contains(&vad.min_speech_duration) {
        return Err(ConfigError::InvalidVadMinSpeechDuration(
            vad.min_speech_duration,
        ));
    }
    if !vad.min_silence_duration.is_finite() || !(0.05..=5.0).contains(&vad.min_silence_duration) {
        return Err(ConfigError::InvalidVadMinSilenceDuration(
            vad.min_silence_duration,
        ));
    }
    if vad.speech_pad_ms > 2_000 {
        return Err(ConfigError::InvalidVadSpeechPadMs(vad.speech_pad_ms));
    }
    Ok(())
}

fn validate_asr_provider<'a>(
    provider: &'a AsrProviderConfig,
    provider_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if provider.id.trim().is_empty() {
        return Err(ConfigError::InvalidAsrProviderId(provider.id.clone()));
    }
    if !provider_ids.insert(provider.id.as_str()) {
        return Err(ConfigError::DuplicateAsrProviderId(provider.id.clone()));
    }
    if provider
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderModelId(provider.id.clone()));
    }
    if provider
        .hotwords_file
        .as_deref()
        .is_some_and(|hotwords_file| hotwords_file.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderHotwordsFile(
            provider.id.clone(),
        ));
    }
    if provider.kind != AsrProviderKind::Command
        && provider
            .command
            .as_deref()
            .is_some_and(|command| command.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderCommand(provider.id.clone()));
    }
    if provider
        .endpoint
        .as_deref()
        .is_some_and(|endpoint| endpoint.trim().is_empty())
    {
        return Err(ConfigError::InvalidAsrProviderEndpoint(provider.id.clone()));
    }
    if provider.timeout_ms == Some(0) {
        return Err(ConfigError::InvalidAsrProviderTimeoutMs(
            provider.id.clone(),
        ));
    }
    if provider.kind == AsrProviderKind::Command
        && provider
            .command
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(ConfigError::InvalidCommandAsrProviderCommand(
            provider.id.clone(),
        ));
    }
    if provider.kind == AsrProviderKind::Remote
        && provider
            .endpoint
            .as_deref()
            .map(str::trim)
            .unwrap_or_default()
            .is_empty()
    {
        return Err(ConfigError::InvalidRemoteAsrProviderEndpoint(
            provider.id.clone(),
        ));
    }
    for key in provider.env.keys() {
        if key.trim().is_empty() {
            return Err(ConfigError::InvalidProviderEnvKey {
                provider_id: provider.id.clone(),
                key: key.clone(),
            });
        }
    }
    Ok(())
}

fn validate_llm(llm: &LlmConfig) -> Result<(), ConfigError> {
    let mut llm_provider_ids = HashSet::new();
    for provider in &llm.providers {
        validate_llm_provider(provider, &mut llm_provider_ids)?;
    }

    let mut adapter_ids = HashSet::new();
    for adapter in &llm.adapters {
        validate_llm_adapter(adapter, &mut adapter_ids)?;
    }
    Ok(())
}

fn validate_llm_provider<'a>(
    provider: &'a LlmProviderConfig,
    provider_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if provider.id.trim().is_empty() {
        return Err(ConfigError::InvalidLlmProviderId(provider.id.clone()));
    }
    if !provider_ids.insert(provider.id.as_str()) {
        return Err(ConfigError::DuplicateLlmProviderId(provider.id.clone()));
    }
    if provider.base_url.trim().is_empty() {
        return Err(ConfigError::InvalidLlmProviderBaseUrl(provider.id.clone()));
    }
    if provider
        .model
        .as_deref()
        .is_some_and(|model| model.trim().is_empty())
    {
        return Err(ConfigError::InvalidLlmProviderModelId(provider.id.clone()));
    }
    if !provider.extra_body.is_object() {
        return Err(ConfigError::InvalidLlmProviderExtraBody(
            provider.id.clone(),
        ));
    }
    Ok(())
}

fn validate_llm_adapter<'a>(
    adapter: &'a LlmAdapterConfig,
    adapter_ids: &mut HashSet<&'a str>,
) -> Result<(), ConfigError> {
    if adapter.id.trim().is_empty() {
        return Err(ConfigError::InvalidLlmAdapterId(adapter.id.clone()));
    }
    if !adapter_ids.insert(adapter.id.as_str()) {
        return Err(ConfigError::DuplicateLlmAdapterId(adapter.id.clone()));
    }
    if adapter.command.trim().is_empty() {
        return Err(ConfigError::InvalidLlmAdapterCommand(adapter.id.clone()));
    }
    if adapter
        .working_dir
        .as_deref()
        .is_some_and(|working_dir| working_dir.trim().is_empty())
    {
        return Err(ConfigError::InvalidLlmAdapterWorkingDir(adapter.id.clone()));
    }
    for key in adapter.env.keys() {
        if key.trim().is_empty() {
            return Err(ConfigError::InvalidLlmAdapterEnvKey {
                adapter_id: adapter.id.clone(),
                key: key.clone(),
            });
        }
    }
    Ok(())
}
