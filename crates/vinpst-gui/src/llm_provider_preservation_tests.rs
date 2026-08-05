use std::collections::HashMap;

use super::*;

fn provider(id: &str) -> LlmProviderConfig {
    LlmProviderConfig {
        id: id.to_owned(),
        base_url: "https://example.invalid/v1".to_owned(),
        api_key: "api-secret".to_owned(),
        model: Some("model-a".to_owned()),
        extra_body: serde_json::json!({"temperature": 0.2}),
        extra: HashMap::from([("future".to_owned(), serde_json::json!({"v": 1}))]),
    }
}

#[test]
fn edit_provider_preserves_exact_immutable_id_and_scene_reference() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.llm.providers.push(provider(" cloud "));
    config.scenes.definitions.push(SceneDefinition {
        id: "cloud-scene".to_owned(),
        label: "Cloud scene".to_owned(),
        prompt: Some("Polish the text".to_owned()),
        provider_id: Some(" cloud ".to_owned()),
        model: None,
        candidate_count: 1,
        timeout_ms: None,
        context_lines: 0,
    });
    config.validate().expect("valid whitespace provider id");
    let configured = config.llm.providers.last().expect("configured provider");
    let mut editor = LlmProviderEditorState::edit(configured);
    editor.update(
        LlmProviderEditorField::Model,
        SecretInput::new("model-b".to_owned()),
    );

    let updated = edit_llm_provider(&config, &editor).expect("edit provider");

    updated.validate().expect("edited config remains valid");
    assert!(
        updated
            .llm
            .providers
            .iter()
            .any(|provider| provider.id == " cloud ")
    );
    assert!(
        !updated
            .llm
            .providers
            .iter()
            .any(|provider| provider.id == "cloud")
    );
    assert_eq!(
        updated
            .scenes
            .definitions
            .iter()
            .find(|scene| scene.id == "cloud-scene")
            .and_then(|scene| scene.provider_id.as_deref()),
        Some(" cloud ")
    );
}

#[test]
fn edit_provider_preserves_untouched_provider_values() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut configured = provider("cloud");
    configured.base_url = " https://example.invalid/v1 ".to_owned();
    configured.api_key = " key-with-padding ".to_owned();
    configured.model = Some(" model-a ".to_owned());
    configured.extra_body = serde_json::json!({
        "authorization": "Bearer provider-secret",
        "temperature": 0.2
    });
    config.llm.providers.push(configured.clone());
    config
        .validate()
        .expect("valid provider with padded values");

    let mut editor = LlmProviderEditorState::edit(&configured);
    editor.update(
        LlmProviderEditorField::BaseUrl,
        SecretInput::new("https://replacement.invalid/v1".to_owned()),
    );
    let updated = edit_llm_provider(&config, &editor).expect("edit provider URL");
    let edited = updated
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == "cloud")
        .expect("edited provider");
    assert_eq!(edited.base_url, "https://replacement.invalid/v1");
    assert_eq!(edited.api_key, configured.api_key);
    assert_eq!(edited.model, configured.model);
    assert_eq!(edited.extra_body, configured.extra_body);

    let mut editor = LlmProviderEditorState::edit(&configured);
    editor.update(
        LlmProviderEditorField::Model,
        SecretInput::new("model-b".to_owned()),
    );
    let updated = edit_llm_provider(&config, &editor).expect("edit provider model");
    let edited = updated
        .llm
        .providers
        .iter()
        .find(|provider| provider.id == "cloud")
        .expect("edited provider");
    assert_eq!(edited.base_url, configured.base_url);
    assert_eq!(edited.api_key, configured.api_key);
    assert_eq!(edited.model.as_deref(), Some("model-b"));
    assert_eq!(edited.extra_body, configured.extra_body);
}

#[test]
fn base_url_input_masks_sensitive_components_conservatively() {
    assert!(!base_url_input_is_secure(""));
    assert!(!base_url_input_is_secure("https://example.invalid/v1"));
    assert!(!base_url_input_is_secure(
        "https://example.invalid/incomplete?"
    ));
    assert!(!base_url_input_is_secure("not yet a URL"));
    assert!(base_url_input_is_secure(
        "https://user:secret@example.invalid/v1"
    ));
    assert!(base_url_input_is_secure(
        "https://example.invalid/v1?api_key=secret"
    ));
    assert!(base_url_input_is_secure(
        "https:example.invalid/v1?api_key=secret"
    ));
    assert!(base_url_input_is_secure(
        "https://example.invalid/v1#secret-fragment"
    ));
    assert!(base_url_input_is_secure(
        "https//example.invalid/v1?api_key=secret"
    ));
    assert!(base_url_input_is_secure(
        "https//user:secret@example.invalid/v1"
    ));
}

#[test]
fn base_url_masking_stays_secure_during_invalid_edits() {
    let mut configured = provider("cloud");
    configured.base_url = "https://example.invalid/v1?api_key=secret".to_owned();
    let mut editor = LlmProviderEditorState::edit(&configured);
    assert!(editor.base_url_secure);

    editor.update(
        LlmProviderEditorField::BaseUrl,
        SecretInput::new("https//example.invalid/v1?api_key=secret".to_owned()),
    );
    assert!(editor.base_url_secure);

    editor.update(
        LlmProviderEditorField::BaseUrl,
        SecretInput::new("temporarily invalid URL".to_owned()),
    );
    assert!(editor.base_url_secure);

    editor.reset();
    assert!(editor.base_url_secure);
}

#[test]
fn connectivity_test_target_uses_unique_referencing_scene_model() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut configured = provider("cloud");
    configured.model = None;
    config.llm.providers.push(configured);
    config.scenes.definitions.push(SceneDefinition {
        id: "cloud-scene".to_owned(),
        label: "Cloud scene".to_owned(),
        prompt: Some("Polish the text".to_owned()),
        provider_id: Some("cloud".to_owned()),
        model: Some("scene-model".to_owned()),
        candidate_count: 1,
        timeout_ms: None,
        context_lines: 0,
    });
    config.validate().expect("valid scene model fallback");

    let target = llm_provider_test_target(&config, "cloud").expect("resolve test target");

    assert_eq!(target.model.as_deref(), Some("scene-model"));
}

#[test]
fn connectivity_test_target_prefers_provider_default_model() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    config.llm.providers.push(provider("cloud"));
    config.scenes.definitions.push(SceneDefinition {
        id: "cloud-scene".to_owned(),
        label: "Cloud scene".to_owned(),
        prompt: None,
        provider_id: Some("cloud".to_owned()),
        model: Some("scene-model".to_owned()),
        candidate_count: 1,
        timeout_ms: None,
        context_lines: 0,
    });
    config.validate().expect("valid provider default model");

    let target = llm_provider_test_target(&config, "cloud").expect("resolve test target");

    assert_eq!(target.model.as_deref(), Some("model-a"));
}

#[test]
fn connectivity_test_target_rejects_missing_or_ambiguous_models() {
    let mut config = VinpstConfig::bundled_default().expect("bundled config");
    let mut configured = provider("cloud");
    configured.model = None;
    config.llm.providers.push(configured);

    let error = llm_provider_test_target(&config, "cloud").expect_err("missing model");
    assert!(error.contains("no default model"));

    for (id, model) in [("cloud-a", "model-a"), ("cloud-b", "model-b")] {
        config.scenes.definitions.push(SceneDefinition {
            id: id.to_owned(),
            label: id.to_owned(),
            prompt: None,
            provider_id: Some("cloud".to_owned()),
            model: Some(model.to_owned()),
            candidate_count: 1,
            timeout_ms: None,
            context_lines: 0,
        });
    }
    config.validate().expect("valid multiple scene models");
    let error = llm_provider_test_target(&config, "cloud").expect_err("ambiguous models");
    assert!(error.contains("multiple Scene models"));
    assert!(!error.contains("model-a"));
    assert!(!error.contains("model-b"));
}
