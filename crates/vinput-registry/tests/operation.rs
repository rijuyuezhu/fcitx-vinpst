//! Cooperative registry-operation integration tests.

use std::{
    fs,
    io::Write,
    net::TcpListener,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
    thread,
    time::Duration,
};

use vinput_registry::{
    ArchiveStagingError, ChecksumPolicy, PlannedInstallAsset, RegistryAssetSource,
    RegistryAssetStagingError, RegistryEntryKind, RegistryOperationControl,
    RegistryOperationProgress, ReqwestRegistryAssetSource, materialize_staged_tree_controlled,
    stage_archive_by_format_controlled, stage_planned_asset_controlled,
};

#[test]
fn controlled_http_staging_reports_bytes_and_cleans_up_after_cancel() {
    let body = vec![b'x'; 512 * 1024];
    let (url, server) = serve_slow_body(body.clone());
    let directory = tempfile::tempdir().expect("temp dir");
    let output = directory.path().join("model.tar");
    let events = Arc::new(Mutex::new(Vec::new()));
    let control_slot = Arc::new(OnceLock::<RegistryOperationControl>::new());
    let recorded = Arc::clone(&events);
    let cancel = Arc::clone(&control_slot);
    let control = RegistryOperationControl::new(move |event| {
        if matches!(
            event,
            RegistryOperationProgress::Downloading {
                downloaded_bytes,
                ..
            } if downloaded_bytes >= 64 * 1024
        ) {
            cancel.get().expect("control installed").cancel();
        }
        recorded.lock().expect("events lock").push(event);
    });
    control_slot
        .set(control.clone())
        .expect("install operation control");
    let asset = PlannedInstallAsset {
        entry_kind: RegistryEntryKind::Model,
        entry_id: "fixture".to_owned(),
        source_path: "model.tar".to_owned(),
        target_path: "model.tar".to_owned(),
        urls: vec![url],
        sha256: None,
        size_bytes: Some(body.len() as u64),
        checksum_policy: ChecksumPolicy::Missing,
    };

    let error = stage_planned_asset_controlled(
        &ReqwestRegistryAssetSource::with_timeout(Duration::from_secs(5)),
        &asset,
        &output,
        &control,
    )
    .expect_err("download must cancel");

    assert!(matches!(error, RegistryAssetStagingError::Cancelled { .. }));
    assert!(!output.exists());
    assert!(control.is_cancelled());
    assert!(events.lock().expect("events lock").iter().any(|event| {
        matches!(
            event,
            RegistryOperationProgress::Downloading {
                downloaded_bytes,
                total_bytes: Some(total)
            } if *downloaded_bytes >= 64 * 1024 && *total == body.len() as u64
        )
    }));
    assert!(
        fs::read_dir(directory.path())
            .expect("read temp dir")
            .next()
            .is_none()
    );
    server.join().expect("server thread");
}

#[test]
fn cancellation_after_fetch_success_removes_the_completed_temp_file() {
    let directory = tempfile::tempdir().expect("temp dir");
    let output = directory.path().join("model.tar");
    let control = RegistryOperationControl::default();
    let source = CancelAfterFetchSource {
        control: control.clone(),
    };
    let asset = PlannedInstallAsset {
        entry_kind: RegistryEntryKind::Model,
        entry_id: "fixture".to_owned(),
        source_path: "model.tar".to_owned(),
        target_path: "model.tar".to_owned(),
        urls: vec!["https://example.invalid/model.tar".to_owned()],
        sha256: None,
        size_bytes: Some(7),
        checksum_policy: ChecksumPolicy::Missing,
    };

    let error = stage_planned_asset_controlled(&source, &asset, &output, &control)
        .expect_err("post-fetch cancellation must stop publication");

    assert!(matches!(error, RegistryAssetStagingError::Cancelled { .. }));
    assert!(!output.exists());
    assert!(
        fs::read_dir(directory.path())
            .expect("read temp dir")
            .next()
            .is_none()
    );
}

#[test]
fn controlled_archive_staging_removes_partial_tree_after_cancel() {
    let directory = tempfile::tempdir().expect("temp dir");
    let archive_path = directory.path().join("large.tar");
    let output = directory.path().join("extract");
    write_tar(&archive_path, 2 * 1024 * 1024);
    let control_slot = Arc::new(OnceLock::<RegistryOperationControl>::new());
    let cancel = Arc::clone(&control_slot);
    let control = RegistryOperationControl::new(move |event| {
        if matches!(
            event,
            RegistryOperationProgress::Extracting {
                extracted_bytes,
                ..
            } if extracted_bytes >= 64 * 1024
        ) {
            cancel.get().expect("control installed").cancel();
        }
    });
    control_slot
        .set(control.clone())
        .expect("install operation control");

    let error = stage_archive_by_format_controlled(&archive_path, &output, &control)
        .expect_err("extraction must cancel");

    assert!(matches!(error, ArchiveStagingError::Cancelled { .. }));
    assert!(!output.exists());
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("read temp dir")
            .filter_map(Result::ok)
            .filter(|entry| entry
                .file_name()
                .to_string_lossy()
                .contains(".extract.tmp."))
            .count(),
        0
    );
}

#[test]
fn cancelled_materialization_preserves_existing_target() {
    let directory = tempfile::tempdir().expect("temp dir");
    let source = directory.path().join("source");
    let target = directory.path().join("target");
    fs::create_dir(&source).expect("source dir");
    fs::create_dir(&target).expect("target dir");
    fs::write(source.join("new.txt"), "new").expect("new file");
    fs::write(target.join("old.txt"), "old").expect("old file");
    let control = RegistryOperationControl::default();
    control.cancel();

    let error = materialize_staged_tree_controlled(&source, &target, &control)
        .expect_err("publication must cancel");

    assert!(matches!(
        error,
        vinput_registry::RegistryMaterializeError::Cancelled { .. }
    ));
    assert_eq!(fs::read_to_string(target.join("old.txt")).unwrap(), "old");
    assert!(!target.join("new.txt").exists());
    assert!(source.join("new.txt").exists());
}

fn serve_slow_body(body: Vec<u8>) -> (String, thread::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind server");
    let address = listener.local_addr().expect("server address");
    let handle = thread::spawn(move || {
        let (mut stream, _) = listener.accept().expect("accept request");
        let mut request = [0_u8; 4096];
        let _ = std::io::Read::read(&mut stream, &mut request);
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write headers");
        for chunk in body.chunks(16 * 1024) {
            if stream.write_all(chunk).is_err() {
                break;
            }
            let _ = stream.flush();
            thread::sleep(Duration::from_millis(2));
        }
    });
    (format!("http://{address}/model.tar"), handle)
}

fn write_tar(path: &std::path::Path, size: usize) {
    let file = fs::File::create(path).expect("create tar");
    let mut builder = tar::Builder::new(file);
    let mut header = tar::Header::new_gnu();
    header.set_size(size as u64);
    header.set_mode(0o644);
    header.set_cksum();
    builder
        .append_data(&mut header, "model/data.bin", vec![0_u8; size].as_slice())
        .expect("append tar entry");
    builder.finish().expect("finish tar");
}

struct CancelAfterFetchSource {
    control: RegistryOperationControl,
}

impl RegistryAssetSource for CancelAfterFetchSource {
    fn fetch_asset(&self, _url: &str, destination: &Path) -> Result<(), String> {
        fs::write(destination, b"payload").map_err(|error| error.to_string())?;
        self.control.cancel();
        Ok(())
    }

    fn fetch_asset_controlled(
        &self,
        _url: &str,
        destination: &Path,
        _expected_size: Option<u64>,
        _control: &RegistryOperationControl,
    ) -> Result<(), String> {
        self.fetch_asset("unused", destination)
    }
}
