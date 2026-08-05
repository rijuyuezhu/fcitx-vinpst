use pipewire::spa::static_dict;

fn global_with_props(
    id: u32,
    type_: pipewire::types::ObjectType,
    props: Option<&pipewire::spa::utils::dict::DictRef>,
) -> pipewire::registry::GlobalObject<&pipewire::spa::utils::dict::DictRef> {
    pipewire::registry::GlobalObject {
        id,
        permissions: pipewire::permissions::PermissionFlags::empty(),
        type_,
        version: 0,
        props,
    }
}

#[test]
fn pipewire_global_maps_audio_source_metadata() {
    let props = static_dict! {
        "media.class" => "Audio/Source",
        "node.name" => "alsa_input.usb-mic",
        "node.description" => "USB Microphone",
    };
    let global = global_with_props(42, pipewire::types::ObjectType::Node, Some(&props));

    let device = super::audio_device_from_global(&global).unwrap();
    assert_eq!(device.id, 42);
    assert_eq!(device.name, "alsa_input.usb-mic");
    assert_eq!(device.description, "USB Microphone");
}

#[test]
fn pipewire_global_ignores_non_source_nodes() {
    let sink_props = static_dict! {
        "media.class" => "Audio/Sink",
        "node.name" => "alsa_output.speaker",
        "node.description" => "Speakers",
    };
    let source_props = static_dict! {
        "media.class" => "Audio/Source",
        "node.name" => "alsa_input.usb-mic",
    };
    let sink = global_with_props(7, pipewire::types::ObjectType::Node, Some(&sink_props));
    let device = global_with_props(8, pipewire::types::ObjectType::Device, Some(&source_props));
    let missing_props = global_with_props(9, pipewire::types::ObjectType::Node, None);

    assert_eq!(super::audio_device_from_global(&sink), None);
    assert_eq!(super::audio_device_from_global(&device), None);
    assert_eq!(super::audio_device_from_global(&missing_props), None);
}

#[test]
fn pipewire_global_defaults_missing_name_fields() {
    let props = static_dict! {
        "media.class" => "Audio/Source",
    };
    let global = global_with_props(13, pipewire::types::ObjectType::Node, Some(&props));

    let device = super::audio_device_from_global(&global).unwrap();
    assert_eq!(device.id, 13);
    assert_eq!(device.name, "");
    assert_eq!(device.description, "");
}

#[test]
fn pipewire_probe_initializes_client_library() {
    super::probe_client_linkage();
}

#[test]
fn pipewire_live_test_env_gates_are_explicit() {
    assert_eq!(
        super::TEST_PIPEWIRE_ENUMERATE_ENV,
        "VINPST_TEST_PIPEWIRE_ENUMERATE"
    );
    assert_eq!(
        super::TEST_PIPEWIRE_CONTEXT_ENV,
        "VINPST_TEST_PIPEWIRE_CONTEXT"
    );
    assert_eq!(
        super::TEST_PIPEWIRE_RECORD_ENV,
        "VINPST_TEST_PIPEWIRE_RECORD"
    );
    assert_eq!(
        super::TEST_PIPEWIRE_SWITCH_SOURCE_A_ENV,
        "VINPST_TEST_PIPEWIRE_SWITCH_SOURCE_A"
    );
    assert_eq!(
        super::TEST_PIPEWIRE_SWITCH_SOURCE_B_ENV,
        "VINPST_TEST_PIPEWIRE_SWITCH_SOURCE_B"
    );
    assert_eq!(
        super::TEST_PIPEWIRE_SWITCH_SUMMARY_ENV,
        "VINPST_TEST_PIPEWIRE_SWITCH_SUMMARY"
    );
    assert!(!super::TEST_PIPEWIRE_ENUMERATE_ENV.is_empty());
    assert!(!super::TEST_PIPEWIRE_CONTEXT_ENV.is_empty());
    assert!(!super::TEST_PIPEWIRE_RECORD_ENV.is_empty());
}

#[test]
fn pipewire_capture_reuse_is_enabled_by_default_and_has_legacy_opt_outs() {
    assert_eq!(super::CAPTURE_REUSE_ENV, "VINPST_CAPTURE_REUSE");
    for value in [None, Some(""), Some("1"), Some("true"), Some("yes")] {
        assert!(super::capture_stream_reuse_enabled_from(value));
    }
    for value in [
        Some("0"),
        Some("false"),
        Some("False"),
        Some("no"),
        Some("No"),
    ] {
        assert!(!super::capture_stream_reuse_enabled_from(value));
    }
}

#[test]
fn pipewire_idle_destroy_delay_matches_upstream_parsing_and_bounds() {
    assert_eq!(
        super::CAPTURE_IDLE_DESTROY_ENV,
        "VINPST_CAPTURE_IDLE_DESTROY_MS"
    );
    for value in [None, Some(""), Some("invalid"), Some("-1")] {
        assert_eq!(
            super::capture_idle_destroy_ms_from(value),
            super::DEFAULT_CAPTURE_IDLE_DESTROY_MS
        );
    }
    assert_eq!(super::capture_idle_destroy_ms_from(Some("0")), 0);
    assert_eq!(super::capture_idle_destroy_ms_from(Some("-0")), 0);
    assert_eq!(super::capture_idle_destroy_ms_from(Some(" 2500ms")), 2_500);
    assert_eq!(
        super::capture_idle_destroy_ms_from(Some("999999")),
        super::MAX_CAPTURE_IDLE_DESTROY_MS
    );
}

#[test]
fn pipewire_idle_expiration_is_generation_guarded() {
    let mut state = super::PipeWireWorkerState {
        idle_generation: 7,
        ..Default::default()
    };

    assert!(super::should_expire_idle_stream(&state, 7));
    assert!(!super::should_expire_idle_stream(&state, 6));
    state.recording = true;
    assert!(!super::should_expire_idle_stream(&state, 7));
}

#[test]
fn pipewire_start_timing_defaults_and_first_buffer_probe_are_stable() {
    assert_eq!(
        super::duration_millis(std::time::Duration::from_millis(1_234)),
        1_234
    );
    let recorder = super::PipeWireAudioRecorder::new();
    assert_eq!(
        recorder.last_start_timing(),
        super::PipeWireStartTiming::default()
    );
    assert_eq!(recorder.first_buffer_latency_ms(), None);
    recorder
        .first_buffer_latency_ms
        .store(42, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(recorder.first_buffer_latency_ms(), Some(42));
}

#[test]
fn pipewire_first_buffer_latency_is_recorded_only_once() {
    let latency = std::sync::atomic::AtomicU64::new(super::UNKNOWN_TIMING_MS);
    let armed_at = std::time::Instant::now()
        .checked_sub(std::time::Duration::from_millis(5))
        .expect("five milliseconds should be representable");

    let first = super::record_first_buffer_latency(Some(armed_at), &latency);
    let recorded = latency.load(std::sync::atomic::Ordering::Relaxed);
    let second = super::record_first_buffer_latency(Some(armed_at), &latency);

    assert!(first.is_some());
    assert_eq!(first, Some(recorded));
    assert_eq!(second, None);
    assert_eq!(latency.load(std::sync::atomic::Ordering::Relaxed), recorded);
}

#[test]
fn pipewire_stream_reuse_requires_identical_target_and_pcm_plan() {
    let default = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Default);
    let same = default.clone();
    let other = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
        "alsa_input.usb-mic".to_owned(),
    ));

    assert!(!super::can_reuse_stream(None, &default));
    assert!(super::can_reuse_stream(Some(&default), &same));
    assert!(!super::can_reuse_stream(Some(&default), &other));
}

#[test]
fn pipewire_recording_pcm_policy_matches_asr_default() {
    assert_eq!(super::RECORDING_FORMAT, "S16LE");
    assert_eq!(
        super::RECORDING_SAMPLE_RATE_HZ,
        super::DEFAULT_SAMPLE_RATE_HZ
    );
    assert_eq!(super::RECORDING_CHANNELS, super::DEFAULT_CHANNELS);
    assert_eq!(
        super::recording_pcm_spec(),
        super::PcmSpec::mono_i16(super::DEFAULT_SAMPLE_RATE_HZ)
    );
}

#[test]
fn pipewire_stream_config_preserves_target_and_pcm_policy() {
    let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
        "alsa_input.test".to_owned(),
    ));

    assert_eq!(
        config.target,
        super::CaptureTarget::Object("alsa_input.test".to_owned())
    );
    assert_eq!(config.format, super::RECORDING_FORMAT);
    assert_eq!(config.pcm_spec, super::recording_pcm_spec());
}

#[test]
fn pipewire_recorder_tracks_idle_state_and_stream_plan() {
    let mut recorder = super::PipeWireAudioRecorder::new();

    super::AudioRecorder::set_chunk_callback(&mut recorder, None);

    assert_eq!(recorder.target(), &super::CaptureTarget::Default);
    assert_eq!(
        recorder.stream_config(),
        &super::PipeWireStreamConfig::for_target(super::CaptureTarget::Default)
    );
    assert!(!super::AudioRecorder::is_recording(&recorder));
    assert_eq!(
        super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap_err(),
        super::AudioError::RecorderNotRecording
    );
    super::AudioRecorder::cancel_recording(&mut recorder).unwrap();
}

#[test]
fn pipewire_recording_params_encode_requested_audio_policy() {
    let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
        "alsa_input.usb-mic".to_owned(),
    ));
    let values = super::pipewire_recording_param_values(&config).unwrap();
    let pod = pipewire::spa::pod::Pod::from_bytes(&values).unwrap();
    let mut audio_info = pipewire::spa::param::audio::AudioInfoRaw::new();
    audio_info.parse(pod).unwrap();

    assert_eq!(
        audio_info.format(),
        pipewire::spa::param::audio::AudioFormat::S16LE
    );
    assert_eq!(audio_info.rate(), super::RECORDING_SAMPLE_RATE_HZ);
    assert_eq!(audio_info.channels(), u32::from(super::RECORDING_CHANNELS));
    assert_eq!(
        super::pipewire_capture_source_name(&config),
        "pipewire:alsa_input.usb-mic"
    );
}

#[test]
fn pipewire_recording_error_includes_stream_plan() {
    let config = super::PipeWireStreamConfig::for_target(super::CaptureTarget::Object(
        "alsa_input.usb-mic".to_owned(),
    ));

    let error = super::pipewire_recording_error(&config, "worker panicked").to_string();

    assert!(error.contains("PipeWire recorder stream setup failed"));
    assert!(error.contains("alsa_input.usb-mic"));
    assert!(error.contains("S16LE"));
    assert!(error.contains("16000"));
    assert!(error.contains("channels: 1"));
    assert!(error.contains("worker panicked"));
}

#[test]
fn pipewire_recorder_live_capture_when_enabled() {
    if !super::live_test_enabled(super::TEST_PIPEWIRE_RECORD_ENV) {
        return;
    }
    let record_ms = std::env::var(super::TEST_PIPEWIRE_RECORD_MS_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(100);
    let minimum_peak = std::env::var(super::TEST_PIPEWIRE_MIN_PEAK_ENV)
        .ok()
        .and_then(|value| value.parse::<i16>().ok())
        .unwrap_or(0);
    let mut recorder = super::PipeWireAudioRecorder::new();
    super::AudioRecorder::begin_recording(&mut recorder, super::CaptureTarget::Default).unwrap();

    assert!(super::AudioRecorder::is_recording(&recorder));
    std::thread::sleep(std::time::Duration::from_millis(record_ms));
    let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();
    eprintln!(
        "PipeWire live capture: source={:?} frames={} duration_ms={} peak_abs={} first_buffer_ms={:?}",
        captured.source_name,
        captured.pcm.frame_len(),
        captured.pcm.duration_ms(),
        captured.pcm.peak_abs(),
        recorder.first_buffer_latency_ms(),
    );

    assert!(!super::AudioRecorder::is_recording(&recorder));
    assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
    assert_eq!(captured.source_name.as_deref(), Some("pipewire:default"));
    assert!(
        captured.pcm.peak_abs() >= minimum_peak,
        "PipeWire live capture peak {} was below required minimum {minimum_peak}",
        captured.pcm.peak_abs(),
    );
}

#[test]
fn pipewire_recorder_live_callback_survives_multiple_recordings_when_enabled() {
    if !super::live_test_enabled(super::TEST_PIPEWIRE_RECORD_ENV) {
        return;
    }
    let mut recorder = super::PipeWireAudioRecorder::new();
    let callback_count = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let callback_count_for_callback = std::sync::Arc::clone(&callback_count);
    super::AudioRecorder::set_chunk_callback(
        &mut recorder,
        Some(Box::new(move |_chunk| {
            *callback_count_for_callback.lock().unwrap() += 1;
        })),
    );

    for index in 0..2 {
        super::AudioRecorder::begin_recording(&mut recorder, super::CaptureTarget::Default)
            .unwrap();
        let timing = recorder.last_start_timing();
        assert_eq!(timing.stream_reused, index > 0);
        assert_eq!(timing.created_new_stream, index == 0);
        std::thread::sleep(std::time::Duration::from_millis(100));
        let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();
        assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
        assert!(recorder.first_buffer_latency_ms().is_some());
    }

    assert!(*callback_count.lock().unwrap() >= 2);
}

#[test]
fn pipewire_recorder_live_rebuilds_for_target_switch_when_enabled() {
    let Ok(source_a) = std::env::var(super::TEST_PIPEWIRE_SWITCH_SOURCE_A_ENV) else {
        return;
    };
    let source_b = std::env::var(super::TEST_PIPEWIRE_SWITCH_SOURCE_B_ENV)
        .expect("second PipeWire switch source must be configured");
    let summary_path = std::env::var(super::TEST_PIPEWIRE_SWITCH_SUMMARY_ENV)
        .expect("PipeWire switch summary path must be configured");
    let record_ms = std::env::var(super::TEST_PIPEWIRE_RECORD_MS_ENV)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(500);
    let minimum_peak = std::env::var(super::TEST_PIPEWIRE_MIN_PEAK_ENV)
        .ok()
        .and_then(|value| value.parse::<i16>().ok())
        .unwrap_or(512);
    let mut recorder = super::PipeWireAudioRecorder::new();
    let mut recordings = Vec::new();

    for source in [&source_a, &source_b] {
        super::AudioRecorder::begin_recording(
            &mut recorder,
            super::CaptureTarget::Object(source.clone()),
        )
        .unwrap();
        let timing = recorder.last_start_timing();
        assert!(!timing.stream_reused);
        assert!(timing.created_new_stream);
        std::thread::sleep(std::time::Duration::from_millis(record_ms));
        let captured = super::AudioRecorder::stop_and_get_buffer(&mut recorder).unwrap();
        let expected_source = format!("pipewire:{source}");
        assert_eq!(
            captured.source_name.as_deref(),
            Some(expected_source.as_str())
        );
        assert_eq!(captured.pcm.spec(), super::recording_pcm_spec());
        assert!(
            captured.pcm.peak_abs() >= minimum_peak,
            "PipeWire source {source} peak {} was below required minimum {minimum_peak}",
            captured.pcm.peak_abs(),
        );
        recordings.push(serde_json::json!({
            "source": source,
            "reported_source": captured.source_name,
            "frames": captured.pcm.frame_len(),
            "duration_ms": captured.pcm.duration_ms(),
            "peak_abs": captured.pcm.peak_abs(),
            "first_buffer_ms": recorder.first_buffer_latency_ms(),
            "stream_reused": timing.stream_reused,
            "created_new_stream": timing.created_new_stream,
            "create_stream_ms": timing.create_stream_ms,
            "set_active_ms": timing.set_active_ms,
            "start_total_ms": timing.start_total_ms,
        }));
    }

    std::fs::write(
        summary_path,
        serde_json::to_vec_pretty(&serde_json::json!({
            "ok": true,
            "same_recorder": true,
            "target_switch_rebuilt_stream": true,
            "recordings": recordings,
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn pipewire_enumerator_lists_sources_when_enabled() {
    if !super::live_test_enabled(super::TEST_PIPEWIRE_ENUMERATE_ENV) {
        return;
    }
    let mut enumerator = super::PipeWireDeviceEnumerator;
    let _devices = super::AudioDeviceEnumerator::enumerate_audio_sources(&mut enumerator).unwrap();
}

#[test]
fn pipewire_probe_creates_client_context_when_enabled() {
    if !super::live_test_enabled(super::TEST_PIPEWIRE_CONTEXT_ENV) {
        return;
    }
    super::probe_client_context().unwrap();
}
