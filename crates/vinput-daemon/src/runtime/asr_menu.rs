//! ASR provider menu state and selection helpers.

use vinput_config::{AsrProviderKind, VinputConfig};

use super::RuntimeState;

/// ASR menu state exposed through the Rust D-Bus extension.
pub(crate) type AsrMenuStateTuple = (
    String,
    String,
    String,
    bool,
    String,
    Vec<(String, String, String)>,
);

impl RuntimeState {
    /// Returns configured providers plus target/effective reload state.
    pub(crate) fn asr_menu_state(&self) -> AsrMenuStateTuple {
        let state = self.asr_backend_state();
        (
            state.target_provider_id,
            state.effective_provider_id,
            state.effective_model_id,
            state.reload_in_progress,
            state.last_error,
            self.config
                .asr
                .providers
                .iter()
                .map(|provider| {
                    (
                        provider.id.clone(),
                        provider_kind_label(&provider.kind).to_owned(),
                        provider.model.clone().unwrap_or_default(),
                    )
                })
                .collect(),
        )
    }
}

/// Selects a configured provider in an owned config snapshot.
pub(crate) fn select_asr_provider(
    mut config: VinputConfig,
    provider_id: &str,
) -> Result<VinputConfig, vinput_asr::AsrError> {
    if !config
        .asr
        .providers
        .iter()
        .any(|provider| provider.id == provider_id)
    {
        return Err(vinput_asr::AsrError::UnknownProvider(
            provider_id.to_owned(),
        ));
    }
    provider_id.clone_into(&mut config.asr.active_provider);
    Ok(config)
}

fn provider_kind_label(kind: &AsrProviderKind) -> &'static str {
    match kind {
        AsrProviderKind::Local => "local",
        AsrProviderKind::Remote => "remote",
        AsrProviderKind::Command => "command",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vinput_asr::MockAsrBackend;
    use vinput_config::{AsrProviderConfig, AsrProviderKind};

    #[test]
    fn asr_menu_state_reports_target_effective_and_provider_rows() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers.push(AsrProviderConfig {
            id: "cmd".to_owned(),
            kind: AsrProviderKind::Command,
            timeout_ms: None,
            model: Some("cmd-model".to_owned()),
            hotwords_file: None,
            command: Some("cat".to_owned()),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });
        let runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();

        let state = runtime.asr_menu_state();
        assert_eq!(state.0, "sherpa-onnx");
        assert_eq!(state.1, "mock");
        assert_eq!(state.2, "mock-buffered");
        assert!(!state.3);
        assert!(state.4.is_empty());
        assert_eq!(state.5[0].0, "sherpa-onnx");
        assert_eq!(state.5[0].1, "local");
        assert_eq!(
            state.5[1],
            (
                "cmd".to_owned(),
                "command".to_owned(),
                "cmd-model".to_owned()
            )
        );
    }

    #[test]
    fn provider_selection_accepts_configured_and_rejects_unknown_ids() {
        let mut config = VinputConfig::bundled_default().unwrap();
        config.asr.providers.push(AsrProviderConfig {
            id: "mock".to_owned(),
            kind: AsrProviderKind::Local,
            timeout_ms: None,
            model: None,
            hotwords_file: None,
            command: None,
            args: Vec::new(),
            env: std::collections::HashMap::new(),
            endpoint: None,
        });

        let selected = select_asr_provider(config.clone(), "mock").unwrap();
        assert_eq!(selected.asr.active_provider, "mock");
        let error = select_asr_provider(config, "missing").unwrap_err();
        assert!(matches!(error, vinput_asr::AsrError::UnknownProvider(id) if id == "missing"));
    }
}
