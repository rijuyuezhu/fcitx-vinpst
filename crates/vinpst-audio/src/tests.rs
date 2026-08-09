use super::{
    AudioDeviceEnumerator, AudioDeviceInfo, AudioError, AudioRecorder, AudioSource, CaptureTarget,
    CapturedAudio, DEFAULT_CHANNELS, DEFAULT_SAMPLE_RATE_HZ, MockAudioDeviceEnumerator,
    MockAudioRecorder, MockAudioSource, PcmBuffer, PcmChunkRange, PcmSpec, RecorderAudioSource,
    SourceAudioRecorder,
};

fn wav_pcm16le_bytes(sample_rate_hz: u32, channels: u16, samples: &[i16]) -> Vec<u8> {
    let mut data = Vec::new();
    for sample in samples {
        data.extend_from_slice(&sample.to_le_bytes());
    }
    let data_len = u32::try_from(data.len()).expect("test data should fit in u32");
    let byte_rate = sample_rate_hz * u32::from(channels) * 2;
    let block_align = channels * 2;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes());
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate_hz.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&16_u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(&data);
    wav
}

#[test]
fn encodes_i16_samples_as_little_endian_bytes() {
    assert_eq!(
        super::i16_samples_to_le_bytes(&[0x1234, -2]),
        vec![0x34, 0x12, 0xfe, 0xff]
    );
}

#[test]
fn decodes_raw_pcm16le_bytes_with_explicit_spec() {
    let pcm = PcmBuffer::from_pcm16le_bytes(
        PcmSpec {
            sample_rate_hz: 8_000,
            channels: 2,
        },
        &[0x34, 0x12, 0xfe, 0xff],
    )
    .unwrap();
    assert_eq!(pcm.sample_rate_hz(), 8_000);
    assert_eq!(pcm.channels(), 2);
    assert_eq!(pcm.samples(), &[0x1234, -2]);
}

#[test]
fn raw_pcm16le_rejects_odd_byte_count() {
    assert_eq!(
        PcmBuffer::from_pcm16le_bytes(PcmSpec::default(), &[0]).unwrap_err(),
        AudioError::OddPcmByteCount(1)
    );
}

#[test]
fn rejects_zero_sample_rate() {
    assert_eq!(
        PcmBuffer::new(0, vec![1]).unwrap_err(),
        AudioError::InvalidSampleRate(0)
    );
}

#[test]
fn chunk_ranges_split_complete_frames() {
    let pcm = PcmBuffer::new(1_000, vec![0, 1, 2, 3, 4]).unwrap();

    let ranges = pcm.chunk_ranges_by_frames(2).unwrap();

    assert_eq!(
        ranges,
        vec![
            PcmChunkRange {
                start_frame: 0,
                frame_len: 2,
            },
            PcmChunkRange {
                start_frame: 2,
                frame_len: 2,
            },
            PcmChunkRange {
                start_frame: 4,
                frame_len: 1,
            },
        ]
    );
}

#[test]
fn chunk_ranges_count_multi_channel_frames() {
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 1_000,
            channels: 2,
        },
        vec![10, 11, 20, 21, 30, 31],
    )
    .unwrap();

    let ranges = pcm.chunk_ranges_by_frames(1).unwrap();

    assert_eq!(
        ranges,
        vec![
            PcmChunkRange {
                start_frame: 0,
                frame_len: 1,
            },
            PcmChunkRange {
                start_frame: 1,
                frame_len: 1,
            },
            PcmChunkRange {
                start_frame: 2,
                frame_len: 1,
            },
        ]
    );
}

#[test]
fn pcm_chunk_range_serializes_stable_fields() {
    let range = PcmChunkRange {
        start_frame: 4,
        frame_len: 2,
    };

    let json = serde_json::to_value(range).unwrap();

    assert_eq!(json["start_frame"], 4);
    assert_eq!(json["frame_len"], 2);
}

#[test]
fn chunked_pcm_preserves_interleaved_multi_channel_frames() {
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 1_000,
            channels: 2,
        },
        vec![10, 11, 20, 21, 30, 31],
    )
    .unwrap();

    let chunks = pcm.chunks_by_frames(2).unwrap();

    assert_eq!(chunks.len(), 2);
    assert_eq!(chunks[0].spec(), pcm.spec());
    assert_eq!(chunks[0].samples(), &[10, 11, 20, 21]);
    assert_eq!(chunks[1].spec(), pcm.spec());
    assert_eq!(chunks[1].samples(), &[30, 31]);
}

#[test]
fn chunking_empty_pcm_returns_no_chunks() {
    let pcm = PcmBuffer::at_default_rate(Vec::new());

    assert!(pcm.chunk_ranges_by_frames(10).unwrap().is_empty());
    assert!(pcm.chunks_by_frames(10).unwrap().is_empty());
}

#[test]
fn chunking_rejects_zero_frames_per_chunk() {
    let pcm = PcmBuffer::at_default_rate(vec![1, 2, 3]);

    assert_eq!(
        pcm.chunk_ranges_by_frames(0).unwrap_err(),
        AudioError::InvalidChunkFrameCount(0)
    );
    assert_eq!(
        pcm.chunks_by_frames(0).unwrap_err(),
        AudioError::InvalidChunkFrameCount(0)
    );
}

#[test]
fn reports_duration_at_sample_rate() {
    let pcm = PcmBuffer::new(1_000, vec![0; 1_500]).unwrap();
    assert_eq!(pcm.duration_ms(), 1_500);
    assert_eq!(pcm.frame_len(), 1_500);
    assert_eq!(
        PcmBuffer::at_default_rate(vec![0]).sample_rate_hz(),
        DEFAULT_SAMPLE_RATE_HZ
    );
    assert_eq!(
        PcmBuffer::at_default_rate(vec![0]).channels(),
        DEFAULT_CHANNELS
    );
}

#[test]
fn multi_channel_duration_counts_frames_not_samples() {
    let pcm = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: 1_000,
            channels: 2,
        },
        vec![0; 2_000],
    )
    .unwrap();
    assert_eq!(pcm.len(), 2_000);
    assert_eq!(pcm.frame_len(), 1_000);
    assert_eq!(pcm.duration_ms(), 1_000);
}

#[test]
fn pcm_spec_rejects_zero_channels() {
    let error = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            channels: 0,
        },
        vec![1],
    )
    .unwrap_err();
    assert_eq!(error, AudioError::InvalidChannelCount(0));
}

#[test]
fn pcm_buffer_rejects_unaligned_multi_channel_samples() {
    let error = PcmBuffer::with_spec(
        PcmSpec {
            sample_rate_hz: DEFAULT_SAMPLE_RATE_HZ,
            channels: 2,
        },
        vec![1, 2, 3],
    )
    .unwrap_err();
    assert_eq!(
        error,
        AudioError::UnalignedSamples {
            samples: 3,
            channels: 2,
        }
    );
}

#[test]
fn pcm_spec_deserialization_defaults_to_mono() {
    let spec: PcmSpec = serde_json::from_str(r#"{"sample_rate_hz":16000}"#).unwrap();
    assert_eq!(spec.sample_rate_hz, DEFAULT_SAMPLE_RATE_HZ);
    assert_eq!(spec.channels, DEFAULT_CHANNELS);
}

#[test]
fn pcm_buffer_preserves_explicit_spec() {
    let spec = PcmSpec {
        sample_rate_hz: 48_000,
        channels: 2,
    };
    let pcm = PcmBuffer::with_spec(spec, vec![1, -1]).unwrap();
    assert_eq!(pcm.spec(), spec);
    assert_eq!(pcm.sample_rate_hz(), 48_000);
    assert_eq!(pcm.channels(), 2);
}

#[test]
fn wav_pcm16le_parser_preserves_metadata_and_samples() {
    let bytes = wav_pcm16le_bytes(48_000, 2, &[1_000, -1_000, 2_000, -2_000]);
    let pcm = PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap();

    assert_eq!(pcm.sample_rate_hz(), 48_000);
    assert_eq!(pcm.channels(), 2);
    assert_eq!(pcm.samples(), &[1_000, -1_000, 2_000, -2_000]);
}

#[test]
fn wav_pcm16le_parser_skips_unknown_padded_chunks() {
    let bytes = wav_pcm16le_bytes(16_000, 1, &[100, -100]);
    let riff_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let mut with_junk = Vec::new();
    with_junk.extend_from_slice(&bytes[..12]);
    with_junk.extend_from_slice(b"JUNK");
    with_junk.extend_from_slice(&3_u32.to_le_bytes());
    with_junk.extend_from_slice(b"abc");
    with_junk.push(0);
    with_junk.extend_from_slice(&bytes[12..]);
    with_junk[4..8].copy_from_slice(&(riff_size + 12).to_le_bytes());

    let pcm = PcmBuffer::from_wav_pcm16le_bytes(&with_junk).unwrap();
    assert_eq!(pcm.sample_rate_hz(), 16_000);
    assert_eq!(pcm.channels(), 1);
    assert_eq!(pcm.samples(), &[100, -100]);
}

#[test]
fn wav_pcm16le_parser_rejects_inconsistent_layout_metadata() {
    let mut bytes = wav_pcm16le_bytes(16_000, 2, &[1, -1]);
    bytes[32..34].copy_from_slice(&2_u16.to_le_bytes());
    assert_eq!(
        PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
        AudioError::InvalidWav("block align does not match channel count".to_owned())
    );

    let mut bytes = wav_pcm16le_bytes(16_000, 2, &[1, -1]);
    bytes[28..32].copy_from_slice(&16_000_u32.to_le_bytes());
    assert_eq!(
        PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
        AudioError::InvalidWav("byte rate does not match sample format".to_owned())
    );
}

#[test]
fn wav_pcm16le_parser_rejects_unsupported_format() {
    let mut bytes = wav_pcm16le_bytes(16_000, 1, &[1]);
    bytes[20..22].copy_from_slice(&3_u16.to_le_bytes());
    assert_eq!(
        PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
        AudioError::InvalidWav("only PCM format tag 1 is supported".to_owned())
    );

    let mut bytes = wav_pcm16le_bytes(16_000, 1, &[1]);
    bytes[34..36].copy_from_slice(&24_u16.to_le_bytes());
    assert_eq!(
        PcmBuffer::from_wav_pcm16le_bytes(&bytes).unwrap_err(),
        AudioError::InvalidWav("only 16-bit samples are supported".to_owned())
    );
}

#[test]
fn gain_matches_frozen_i16_clamp_and_truncation() {
    let pcm = PcmBuffer::at_default_rate(vec![1, -1, 20_000, -20_000]).with_gain(1.5);
    assert_eq!(pcm.samples(), &[1, -1, 30_000, -30_000]);

    let saturated = PcmBuffer::at_default_rate(vec![20_000, -20_000]).with_gain(2.0);
    assert_eq!(saturated.samples(), &[i16::MAX, i16::MIN]);
}

#[test]
fn non_finite_gain_is_ignored() {
    let original = PcmBuffer::at_default_rate(vec![100, -100]);
    assert_eq!(original.with_gain(f32::NAN), original);
    assert_eq!(original.with_gain(f32::INFINITY), original);
}

#[test]
fn quiet_peak_normalization_matches_frozen_default_policy() {
    let mut quiet = PcmBuffer::at_default_rate(vec![0, 1_000, -500, 0]);
    quiet.normalize_quiet_to_full_scale();
    assert_eq!(quiet.samples(), &[0, i16::MAX, -16_384, 0]);

    let mut loud = PcmBuffer::at_default_rate(vec![0, 4_000, -2_000, 0]);
    let loud_before = loud.clone();
    loud.normalize_quiet_to_full_scale();
    assert_eq!(loud, loud_before);

    let mut just_below_threshold = PcmBuffer::at_default_rate(vec![3_276]);
    just_below_threshold.normalize_quiet_to_full_scale();
    assert_eq!(just_below_threshold.samples(), &[i16::MAX]);

    let mut just_above_threshold = PcmBuffer::at_default_rate(vec![3_277]);
    just_above_threshold.normalize_quiet_to_full_scale();
    assert_eq!(just_above_threshold.samples(), &[3_277]);

    let mut silence = PcmBuffer::at_default_rate(vec![0, 0, 0]);
    silence.normalize_quiet_to_full_scale();
    assert_eq!(silence.samples(), &[0, 0, 0]);
}

#[test]
fn captured_audio_reports_pcm_duration_and_source() {
    let captured = CapturedAudio::named(PcmBuffer::new(1_000, vec![0; 250]).unwrap(), "fixture");
    assert_eq!(captured.duration_ms(), 250);
    assert_eq!(captured.source_name.as_deref(), Some("fixture"));
}

#[test]
fn capture_target_parses_config_values() {
    assert_eq!(
        CaptureTarget::from_config_value("default").unwrap(),
        CaptureTarget::Default
    );
    assert_eq!(
        CaptureTarget::from_config_value("  alsa_input.usb-mic  ").unwrap(),
        CaptureTarget::Object("alsa_input.usb-mic".to_owned())
    );
    assert_eq!(
        CaptureTarget::from_config_value("  ").unwrap_err(),
        AudioError::InvalidCaptureTarget("  ".to_owned())
    );
    assert_eq!(
        CaptureTarget::Object("node".to_owned()).target_object(),
        Some("node")
    );
    assert_eq!(CaptureTarget::Default.target_object(), None);
}

#[test]
fn audio_device_info_maps_to_capture_target() {
    let device = AudioDeviceInfo::new(42, "alsa_input.usb-mic", "USB Microphone");

    assert_eq!(device.id, 42);
    assert_eq!(device.name, "alsa_input.usb-mic");
    assert_eq!(device.description, "USB Microphone");
    assert_eq!(
        device.capture_target(),
        CaptureTarget::Object("alsa_input.usb-mic".to_owned())
    );
}

#[test]
fn mock_audio_device_enumerator_preserves_backend_order() {
    let devices = vec![
        AudioDeviceInfo::new(7, "first", "First source"),
        AudioDeviceInfo::new(8, "second", "Second source"),
    ];
    let mut enumerator = MockAudioDeviceEnumerator::new(devices.clone());

    assert_eq!(enumerator.enumerate_audio_sources().unwrap(), devices);
    assert_eq!(
        MockAudioDeviceEnumerator::default()
            .enumerate_audio_sources()
            .unwrap(),
        Vec::new()
    );
}

#[test]
fn mock_audio_recorder_can_emit_configured_frame_chunks() {
    let recording = CapturedAudio::anonymous(
        PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 1_000,
                channels: 2,
            },
            vec![10, 11, 20, 21, 30, 31],
        )
        .unwrap(),
    );
    let mut recorder = MockAudioRecorder::once(recording)
        .with_chunk_frames(2)
        .unwrap();
    let chunks = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<i16>>::new()));
    let seen_chunks = chunks.clone();
    recorder.set_chunk_callback(Some(Box::new(move |chunk| {
        seen_chunks.lock().unwrap().push(chunk.samples().to_vec());
    })));

    recorder.begin_recording(CaptureTarget::Default).unwrap();
    recorder.stop_and_get_buffer().unwrap();

    assert_eq!(
        *chunks.lock().unwrap(),
        vec![vec![10, 11, 20, 21], vec![30, 31]]
    );
}

#[test]
fn mock_audio_recorder_rejects_zero_frame_chunk_config() {
    let recording = CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![1]));

    assert!(matches!(
        MockAudioRecorder::once(recording).with_chunk_frames(0),
        Err(AudioError::InvalidChunkFrameCount(0))
    ));
}

#[test]
fn mock_audio_recorder_tracks_legacy_lifecycle() {
    use std::sync::{Arc, Mutex};

    let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![1, -1]), "fixture");
    let seen_chunk = Arc::new(Mutex::new(Vec::<i16>::new()));
    let seen_chunk_for_callback = Arc::clone(&seen_chunk);
    let mut recorder = MockAudioRecorder::once(captured.clone());

    assert!(!recorder.is_recording());
    assert_eq!(
        recorder.stop_and_get_buffer().unwrap_err(),
        AudioError::RecorderNotRecording
    );
    recorder
        .begin_recording(CaptureTarget::Object("mic".to_owned()))
        .unwrap();
    assert_eq!(recorder.target(), &CaptureTarget::Object("mic".to_owned()));
    assert!(recorder.is_recording());
    assert_eq!(
        recorder
            .begin_recording(CaptureTarget::Default)
            .unwrap_err(),
        AudioError::RecorderAlreadyRecording
    );
    recorder.set_chunk_callback(Some(Box::new(move |pcm| {
        *seen_chunk_for_callback.lock().unwrap() = pcm.samples().to_vec();
    })));

    assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
    assert!(!recorder.is_recording());
    assert_eq!(*seen_chunk.lock().unwrap(), vec![1, -1]);
}

#[test]
fn mock_audio_recorder_cancel_discards_active_recording() {
    let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![7]), "fixture");
    let mut recorder = MockAudioRecorder::once(captured.clone());

    recorder.begin_recording(CaptureTarget::Default).unwrap();
    recorder.cancel_recording().unwrap();
    assert!(!recorder.is_recording());
    assert_eq!(
        recorder.stop_and_get_buffer().unwrap_err(),
        AudioError::RecorderNotRecording
    );

    recorder.begin_recording(CaptureTarget::Default).unwrap();
    assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
}

#[test]
fn recorder_audio_source_bridges_stateful_recorder() {
    let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![9, -9]), "fixture");
    let recorder = MockAudioRecorder::once(captured.clone());
    let mut source = RecorderAudioSource::new(
        recorder,
        CaptureTarget::Object("alsa_input.usb-mic".to_owned()),
    );

    assert_eq!(source.read_buffer().unwrap(), captured);
    assert_eq!(
        source.recorder().target(),
        &CaptureTarget::Object("alsa_input.usb-mic".to_owned())
    );
    assert!(!source.recorder().is_recording());
}

#[test]
fn recorder_audio_source_cancels_after_stop_error() {
    let recorder = MockAudioRecorder::from_recordings(Vec::new());
    let mut source = RecorderAudioSource::new(recorder, CaptureTarget::Default);

    assert_eq!(
        source.read_buffer().unwrap_err(),
        AudioError::SourceExhausted
    );
    assert!(!source.recorder().is_recording());
}

#[test]
fn source_audio_recorder_can_emit_configured_frame_chunks() {
    let captured = CapturedAudio::anonymous(
        PcmBuffer::with_spec(
            PcmSpec {
                sample_rate_hz: 1_000,
                channels: 2,
            },
            vec![10, 11, 20, 21, 30, 31],
        )
        .unwrap(),
    );
    let source = MockAudioSource::once(captured);
    let mut recorder = SourceAudioRecorder::new(Box::new(source))
        .with_chunk_frames(2)
        .unwrap();
    let chunks = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Vec<i16>>::new()));
    let seen_chunks = chunks.clone();
    recorder.set_chunk_callback(Some(Box::new(move |chunk| {
        seen_chunks.lock().unwrap().push(chunk.samples().to_vec());
    })));

    recorder.begin_recording(CaptureTarget::Default).unwrap();
    assert_eq!(
        *chunks.lock().unwrap(),
        vec![vec![10, 11, 20, 21], vec![30, 31]]
    );
    recorder.stop_and_get_buffer().unwrap();

    assert_eq!(
        *chunks.lock().unwrap(),
        vec![vec![10, 11, 20, 21], vec![30, 31]]
    );
}

#[test]
fn source_audio_recorder_rejects_zero_frame_chunk_config() {
    let captured = CapturedAudio::anonymous(PcmBuffer::at_default_rate(vec![1]));
    let source = MockAudioSource::once(captured);

    assert!(matches!(
        SourceAudioRecorder::new(Box::new(source)).with_chunk_frames(0),
        Err(AudioError::InvalidChunkFrameCount(0))
    ));
}

#[test]
fn source_audio_recorder_wraps_audio_source_lifecycle() {
    use std::sync::{Arc, Mutex};

    let captured = CapturedAudio::named(PcmBuffer::at_default_rate(vec![3, -3]), "fixture");
    let seen_chunk = Arc::new(Mutex::new(Vec::<i16>::new()));
    let seen_chunk_for_callback = Arc::clone(&seen_chunk);
    let source = MockAudioSource::once(captured.clone());
    let mut recorder = SourceAudioRecorder::new(Box::new(source));

    assert_eq!(
        recorder.stop_and_get_buffer().unwrap_err(),
        AudioError::RecorderNotRecording
    );
    recorder
        .begin_recording(CaptureTarget::Object("mic".to_owned()))
        .unwrap();
    assert_eq!(recorder.target(), &CaptureTarget::Object("mic".to_owned()));
    assert!(recorder.is_recording());
    recorder.set_chunk_callback(Some(Box::new(move |pcm| {
        *seen_chunk_for_callback.lock().unwrap() = pcm.samples().to_vec();
    })));

    assert_eq!(recorder.stop_and_get_buffer().unwrap(), captured);
    assert!(!recorder.is_recording());
    assert_eq!(*seen_chunk.lock().unwrap(), vec![3, -3]);
}

#[test]
fn mock_audio_source_returns_frames_in_order() {
    let first = CapturedAudio::named(PcmBuffer::at_default_rate(vec![1]), "first");
    let second = CapturedAudio::named(PcmBuffer::at_default_rate(vec![2]), "second");
    let mut source = MockAudioSource::from_frames(vec![first.clone(), second.clone()]);
    assert_eq!(source.read_buffer().unwrap(), first);
    assert_eq!(source.read_buffer().unwrap(), second);
    assert_eq!(
        source.read_buffer().unwrap_err(),
        AudioError::SourceExhausted
    );
}
