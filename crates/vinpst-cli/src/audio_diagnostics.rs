use crate::{CaptureTarget, Context, VinpstConfig};

struct AudioDevicesReport {
    devices: Vec<vinpst_audio::AudioDeviceInfo>,
    live: bool,
    enumeration_error: Option<String>,
}

#[cfg(feature = "pipewire-backend")]
fn enumerate_audio_devices() -> AudioDevicesReport {
    use vinpst_audio::AudioDeviceEnumerator as _;

    let mut enumerator = vinpst_audio::pipewire_backend::PipeWireDeviceEnumerator;
    match enumerator
        .enumerate_audio_sources()
        .context("enumerate PipeWire audio sources")
    {
        Ok(devices) => AudioDevicesReport {
            devices,
            live: true,
            enumeration_error: None,
        },
        Err(error) => AudioDevicesReport {
            devices: Vec::new(),
            live: false,
            enumeration_error: Some(format!("{error:#}")),
        },
    }
}

#[cfg(not(feature = "pipewire-backend"))]
fn enumerate_audio_devices() -> AudioDevicesReport {
    AudioDevicesReport {
        devices: Vec::new(),
        live: false,
        enumeration_error: None,
    }
}

#[cfg(feature = "pipewire-backend")]
fn audio_devices_backend_name() -> &'static str {
    "pipewire"
}

#[cfg(not(feature = "pipewire-backend"))]
fn audio_devices_backend_name() -> &'static str {
    "unavailable"
}

pub(crate) fn audio_devices_json(config: &VinpstConfig) -> anyhow::Result<serde_json::Value> {
    let capture_target = CaptureTarget::from_config_value(&config.global.capture_device)
        .context("parse configured capture device")?;
    let audio_report = enumerate_audio_devices();
    Ok(serde_json::json!({
        "ok": true,
        "capture_device": config.global.capture_device,
        "capture_target": capture_target_json(&capture_target),
        "backend": audio_devices_backend_name(),
        "live": audio_report.live,
        "devices": audio_report.devices,
        "enumeration_error": audio_report.enumeration_error,
    }))
}

pub(crate) fn capture_target_json(target: &CaptureTarget) -> serde_json::Value {
    match target {
        CaptureTarget::Default => serde_json::json!({"kind": "default", "target_object": null}),
        CaptureTarget::Object(value) => {
            serde_json::json!({"kind": "object", "target_object": value})
        }
    }
}
