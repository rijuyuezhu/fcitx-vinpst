//! Capture-device state and persistence.

use vinput_audio::CaptureTarget;
use vinput_protocol::ServiceStatus;

use super::{RuntimeError, RuntimeState, config_io::persist_config_atomically};

impl RuntimeState {
    /// Returns the capture-device config value used by the next recording.
    pub(crate) fn capture_device(&self) -> String {
        self.config.global.capture_device.clone()
    }

    /// Selects the capture device for the next recording and persists it when possible.
    pub(crate) fn set_capture_device(&mut self, target: &str) -> Result<bool, RuntimeError> {
        if self.status != ServiceStatus::Idle {
            return Err(RuntimeError::Busy(self.status));
        }
        let target = CaptureTarget::from_config_value(target).map_err(RuntimeError::Audio)?;
        let normalized = match target {
            CaptureTarget::Default => "default".to_owned(),
            CaptureTarget::Object(value) => value,
        };

        let mut updated = self.config.clone();
        normalized.clone_into(&mut updated.global.capture_device);
        updated.validate().map_err(RuntimeError::InvalidConfig)?;

        let persisted = if let Some(path) = self.config_path.as_deref() {
            persist_config_atomically(path, &updated, "capture-device")?;
            true
        } else {
            false
        };
        self.config = updated;
        Ok(persisted)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use vinput_asr::MockAsrBackend;
    use vinput_config::VinputConfig;

    #[test]
    fn capture_device_is_normalized_and_persisted() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("config.json");
        let config = VinputConfig::bundled_default().unwrap();
        fs::write(&config_path, serde_json::to_vec_pretty(&config).unwrap()).unwrap();
        let mut runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();
        runtime.set_config_path(Some(config_path.clone()));

        assert!(runtime.set_capture_device("  virtual.source  ").unwrap());
        assert_eq!(runtime.capture_device(), "virtual.source");
        let persisted =
            VinputConfig::from_json_str(&fs::read_to_string(config_path).unwrap()).unwrap();
        assert_eq!(persisted.global.capture_device, "virtual.source");
    }

    #[test]
    fn capture_device_change_is_rejected_while_recording() {
        let config = VinputConfig::bundled_default().unwrap();
        let mut runtime =
            RuntimeState::with_asr_backend(config, Box::new(MockAsrBackend::buffered("final")))
                .unwrap();
        runtime.start_recording().unwrap();

        let error = runtime.set_capture_device("virtual.source").unwrap_err();
        assert!(matches!(
            error,
            RuntimeError::Busy(ServiceStatus::Recording)
        ));
        assert_eq!(runtime.capture_device(), "default");
    }
}
