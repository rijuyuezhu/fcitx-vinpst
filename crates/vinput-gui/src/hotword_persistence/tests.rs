use super::*;

#[test]
fn unsupported_extended_attribute_queries_are_treated_as_empty() {
    for errno in [Errno::NOTSUP, Errno::OPNOTSUPP] {
        let error = io::Error::from_raw_os_error(errno.raw_os_error());
        assert!(extended_attribute_query_is_unsupported(&error));
    }
    assert!(!extended_attribute_query_is_unsupported(&io::Error::from(
        io::ErrorKind::PermissionDenied
    )));
}

#[test]
fn missing_prerequisite_exists_before_commit() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");

    with_prepared_hotword_file(Some(&path), || {
        assert_eq!(fs::read_to_string(&path).expect("prepared file"), "");
        Ok(())
    })
    .expect("commit prepared file");

    assert!(path.is_file());
}

#[test]
fn missing_prerequisite_rejects_external_creation_after_snapshot() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");

    let error = prepare_missing_hotword_file_with(Some(&path), || {
        fs::write(&path, "external\n").expect("external creation");
    })
    .expect_err("reject raced external creation");

    assert!(error.contains("created outside the GUI"));
    assert_eq!(
        fs::read_to_string(&path).expect("external content"),
        "external\n"
    );
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("directory entries")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn failed_commit_preserves_prepared_and_existing_files() {
    let directory = tempfile::tempdir().expect("temp dir");
    let missing = directory.path().join("missing.txt");
    let modified = directory.path().join("modified.txt");
    let existing = directory.path().join("existing.txt");
    fs::write(&existing, "keep\n").expect("existing fixture");

    let missing_error = with_prepared_hotword_file(Some(&missing), || {
        assert!(missing.is_file());
        Err::<(), _>("fixture commit failure".to_owned())
    })
    .expect_err("preserve missing prerequisite");
    assert!(missing_error.contains("prepared for this update was preserved"));
    assert_eq!(fs::read_to_string(&missing).expect("prepared content"), "");

    let modified_error = with_prepared_hotword_file(Some(&modified), || {
        fs::write(&modified, "external\n").expect("modify prepared file");
        Err::<(), _>("fixture commit failure".to_owned())
    })
    .expect_err("preserve externally modified prerequisite");
    assert!(modified_error.contains("prepared for this update was preserved"));
    assert_eq!(
        fs::read_to_string(&modified).expect("modified content"),
        "external\n"
    );

    let existing_error = with_prepared_hotword_file(Some(&existing), || {
        Err::<(), _>("fixture commit failure".to_owned())
    })
    .expect_err("preserve existing prerequisite");
    assert_eq!(existing_error, "fixture commit failure");
    assert_eq!(
        fs::read_to_string(&existing).expect("existing content"),
        "keep\n"
    );
}
#[test]
fn content_save_is_atomic_conflict_aware_and_retryable_after_reload_failure() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("write fixture");
    let baseline = read_hotword_snapshot(&path).expect("read baseline");

    let outcome = save_hotword_content_with_reload(&path, &baseline, "beta\n", || {
        Ok("fixture reload".to_owned())
    })
    .expect("save content");
    assert!(outcome.summary.contains("fixture reload"));
    assert_eq!(outcome.activation_error, None);
    assert_eq!(
        outcome
            .baseline
            .as_ref()
            .map(|snapshot| snapshot.content.as_str()),
        Some("beta\n")
    );
    assert_eq!(fs::read_to_string(&path).expect("saved content"), "beta\n");

    let loaded = read_hotword_snapshot(&path).expect("read saved content");
    fs::write(&path, "external\n").expect("external update");
    let conflict = save_hotword_content_with_reload(&path, &loaded, "gamma\n", || {
        Ok("unreachable reload".to_owned())
    })
    .expect_err("reject external update");
    assert!(conflict.contains("changed outside"));
    assert_eq!(
        fs::read_to_string(&path).expect("external content"),
        "external\n"
    );

    let external = read_hotword_snapshot(&path).expect("read external content");
    let reload_outcome = save_hotword_content_with_reload(&path, &external, "delta\n", || {
        Err("fixture reload failure".to_owned())
    })
    .expect("preserve published content after reload failure");
    assert!(
        reload_outcome
            .activation_error
            .as_deref()
            .is_some_and(|error| error.contains("rollback was skipped"))
    );
    assert!(reload_outcome.retry_activation);
    assert_eq!(
        fs::read_to_string(&path).expect("published content"),
        "delta\n"
    );
}

#[test]
fn content_save_disables_retry_after_reload_window_file_changes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "delta\n").expect("write fixture");
    let concurrent_baseline = read_hotword_snapshot(&path).expect("read concurrent baseline");
    let concurrent_outcome =
        save_hotword_content_with_reload(&path, &concurrent_baseline, "gui-write\n", || {
            fs::write(&path, "concurrent-write\n").expect("concurrent update");
            Err("fixture reload failure".to_owned())
        })
        .expect("preserve concurrent reload-window update");
    assert!(concurrent_outcome.activation_error.is_some());
    assert!(!concurrent_outcome.retry_activation);
    assert!(concurrent_outcome.baseline.is_none());

    let same_content_baseline = read_hotword_snapshot(&path).expect("read same-content baseline");
    let replacement = directory.path().join("replacement.txt");
    let same_content_outcome = save_hotword_content_with_reload(
        &path,
        &same_content_baseline,
        "gui-same-content\n",
        || {
            fs::write(&replacement, "gui-same-content\n").expect("replacement content");
            fs::rename(&replacement, &path).expect("replace with same content");
            Err("fixture reload failure".to_owned())
        },
    )
    .expect("detect same-content external replacement");
    assert!(same_content_outcome.activation_error.is_some());
    assert!(!same_content_outcome.retry_activation);
    assert!(same_content_outcome.baseline.is_none());
    assert_eq!(
        fs::read_to_string(&path).expect("same replacement content"),
        "gui-same-content\n"
    );

    let missing_path = directory.path().join("new-hotwords.txt");
    let missing_baseline = read_hotword_snapshot(&missing_path).expect("read missing baseline");
    let missing_outcome =
        save_hotword_content_with_reload(&missing_path, &missing_baseline, "gui-create\n", || {
            fs::write(&missing_path, "concurrent-create\n").expect("concurrent create");
            Err("fixture reload failure".to_owned())
        })
        .expect("preserve concurrent creation");
    assert!(missing_outcome.activation_error.is_some());
    assert!(!missing_outcome.retry_activation);
    assert!(missing_outcome.baseline.is_none());
    assert_eq!(
        fs::read_to_string(&missing_path).expect("concurrent created content"),
        "concurrent-create\n"
    );
}

#[test]
fn atomic_publication_keeps_path_available_and_preserves_recovery() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    let published = compare_and_swap_hotword_file_with_exchange_hook(
        &path,
        &baseline,
        b"beta\n",
        || {},
        || {
            assert_eq!(
                fs::read_to_string(&path).expect("configured path before exchange"),
                "alpha\n"
            );
        },
    )
    .expect("publish loaded version");
    assert!(published.previous_version_preserved);
    assert_eq!(
        fs::read_to_string(&path).expect("published content"),
        "beta\n"
    );
    let recovery_files = fs::read_dir(directory.path())
        .expect("recovery directory")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate
                .file_name()
                .is_some_and(|name| name.to_string_lossy().ends_with(".recovery"))
        })
        .collect::<Vec<_>>();
    assert_eq!(recovery_files.len(), 1);
    assert_eq!(
        fs::read_to_string(&recovery_files[0]).expect("recovery content"),
        "alpha\n"
    );
}

#[test]
fn atomic_exchange_rolls_back_late_external_replacement() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    let replacement = directory.path().join("replacement.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    let error = compare_and_swap_hotword_file_with_exchange_hook(
        &path,
        &baseline,
        b"gui-write\n",
        || {},
        || {
            fs::write(&replacement, "external\n").expect("external replacement");
            fs::rename(&replacement, &path).expect("install external replacement");
        },
    )
    .expect_err("rollback late external replacement");

    assert!(error.contains("external version was restored"));
    assert_eq!(
        fs::read_to_string(&path).expect("restored external content"),
        "external\n"
    );
    assert_eq!(
        fs::read_dir(directory.path())
            .expect("transaction directory")
            .filter_map(Result::ok)
            .count(),
        1
    );
}

#[test]
fn hotword_snapshot_rejects_extended_attributes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    let file = fs::File::create(&path).expect("hotword fixture");
    file.set_xattr("user.vinput-test", b"fixture")
        .expect("set fixture xattr");

    let error = read_hotword_snapshot(&path).expect_err("reject extended metadata");
    assert!(error.contains("extended attributes or ACL metadata"));
}

#[test]
fn temporary_publication_rejects_owner_or_group_mismatch() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("hotword fixture");
    let mut baseline = read_hotword_snapshot(&path).expect("baseline");
    let (_, temporary_file) = create_sibling_file(
        directory.path(),
        path.file_name().expect("file name"),
        "tmp",
    )
    .expect("temporary file");
    prepare_temporary_hotword_metadata(&temporary_file, &baseline)
        .expect("matching owner and group");

    baseline.version.as_mut().expect("version").uid =
        baseline.version.expect("version").uid.wrapping_add(1);
    let error = prepare_temporary_hotword_metadata(&temporary_file, &baseline)
        .expect_err("reject ownership mismatch");
    assert!(error.contains("ownership cannot be preserved"));
}

#[test]
fn atomic_publication_restores_special_mode_bits_after_writing() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("hotword fixture");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o6755)).expect("special mode fixture");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {})
        .expect("publish with special mode");
    assert_eq!(
        fs::metadata(&path).expect("published metadata").mode() & 0o7777,
        0o6755
    );
}

#[test]
fn atomic_publication_rejects_changes_after_preparation() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    let direct_write_error =
        compare_and_swap_hotword_file(&path, &baseline, b"gui-write\n", || {
            fs::write(&path, "external-write\n").expect("external write");
        })
        .expect_err("reject write after preparation");
    assert!(direct_write_error.contains("changed outside"));
    assert_eq!(
        fs::read_to_string(&path).expect("external content"),
        "external-write\n"
    );

    let same_content_baseline = read_hotword_snapshot(&path).expect("same baseline");
    let replacement = directory.path().join("same-content-replacement.txt");
    let same_content_error =
        compare_and_swap_hotword_file(&path, &same_content_baseline, b"gui-write\n", || {
            fs::write(&replacement, "external-write\n").expect("replacement content");
            fs::rename(&replacement, &path).expect("atomic external replacement");
        })
        .expect_err("reject same-content replacement after preparation");
    assert!(same_content_error.contains("changed outside"));
    assert_eq!(
        fs::read_to_string(&path).expect("same-content external file"),
        "external-write\n"
    );
}

#[test]
fn post_publication_target_validation_marks_result_unapplied() {
    let mut outcome = HotwordContentSaveOutcome {
        summary: "Hotword content saved; daemon ASR backend applied.".to_owned(),
        activation_error: None,
        baseline: None,
        retry_activation: false,
    };
    append_activation_error(
        &mut outcome,
        "The configured hotword target changed after publication.".to_owned(),
    );
    assert_eq!(outcome.summary, "Hotword content was saved to disk.");
    assert!(!outcome.retry_activation);
    assert!(
        outcome
            .activation_error
            .as_deref()
            .is_some_and(|error| error.contains("target changed"))
    );
}

#[test]
fn published_directory_sync_failure_is_reported_as_committed() {
    let outcome = finish_published_hotword(Path::new("."), true, |_| {
        Err("fixture final sync failure".to_owned())
    });
    assert!(outcome.previous_version_preserved);
    assert!(
        outcome
            .durability_error
            .as_deref()
            .is_some_and(|error| error.contains("was published"))
    );
}

#[test]
fn external_symlink_validation_failure_keeps_configured_path() {
    use std::os::unix::fs::symlink;

    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    let external = directory.path().join("external.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    fs::write(&external, "external\n").expect("external target");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    let error = compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {
        fs::remove_file(&path).expect("remove loaded target");
        symlink(&external, &path).expect("external symlink replacement");
    })
    .expect_err("reject external symlink");
    assert!(error.contains("symbolic link"));
    assert!(error.contains("configured path remained in place"));
    assert!(
        fs::symlink_metadata(&path)
            .expect("external metadata")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        fs::read_to_string(&path).expect("external symlink target"),
        "external\n"
    );
}

#[test]
fn atomic_publication_rejects_permission_changes_after_loading() {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};

    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("initial permissions");
    let baseline = read_hotword_snapshot(&path).expect("baseline");

    let error = compare_and_swap_hotword_file(&path, &baseline, b"beta\n", || {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).expect("external chmod");
    })
    .expect_err("reject permission change after loading");
    assert!(error.contains("changed outside"));
    assert_eq!(
        fs::metadata(&path).expect("restored metadata").mode() & 0o777,
        0o640
    );
    assert_eq!(
        fs::read_to_string(&path).expect("restored content"),
        "alpha\n"
    );
}

#[test]
fn path_reload_confirmation_rejects_external_config_changes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let config_path = directory.path().join("config.json");
    let mut updated = VinputConfig::bundled_default().expect("bundled config");
    updated.asr.providers[0].hotwords_file = Some(
        directory
            .path()
            .join("old.txt")
            .to_string_lossy()
            .into_owned(),
    );
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&updated).expect("serialize updated config"),
    )
    .expect("write updated config");
    ensure_hotword_path_update_current(&config_path, &updated)
        .expect("unchanged path config is current");

    let mut superseding = updated.clone();
    superseding.asr.providers[0].hotwords_file = Some(
        directory
            .path()
            .join("new.txt")
            .to_string_lossy()
            .into_owned(),
    );
    fs::write(
        &config_path,
        serde_json::to_vec_pretty(&superseding).expect("serialize superseding config"),
    )
    .expect("write superseding config");
    let error = ensure_hotword_path_update_current(&config_path, &updated)
        .expect_err("reject superseding path config");
    assert!(error.contains("changed during reload"));
}

#[test]
fn recovery_sync_failure_keeps_configured_path_available() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    let recovery = preserve_current_hotword_file(
        &path,
        directory.path(),
        path.file_name().expect("file name"),
    )
    .expect("preserve configured file");
    assert!(path.exists());
    assert!(recovery.exists());

    let error = synchronize_recovery_or_remove(&recovery, directory.path(), |_| {
        Err("fixture directory sync failure".to_owned())
    })
    .expect_err("sync failure must abort publication");
    assert!(error.contains("configured path remained available"));
    assert_eq!(
        fs::read_to_string(&path).expect("available content"),
        "alpha\n"
    );
    assert!(!recovery.exists());
}

#[test]
fn missing_current_target_preserves_the_recovery_version() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "alpha\n").expect("initial content");
    let recovery = preserve_current_hotword_file(
        &path,
        directory.path(),
        path.file_name().expect("file name"),
    )
    .expect("preserve configured file");
    fs::remove_file(&path).expect("external target removal");

    let error = read_current_hotword_or_preserve_recovery(&path, &recovery)
        .expect_err("missing target must keep recovery");
    assert!(error.contains("previous version was preserved"));
    assert_eq!(
        fs::read_to_string(&recovery).expect("recovery content"),
        "alpha\n"
    );
}

#[test]
fn repeated_publication_keeps_allocating_recovery_files() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("hotwords.txt");
    fs::write(&path, "revision-0\n").expect("initial revision");

    for revision in 1..=70_u8 {
        let baseline = read_hotword_snapshot(&path).expect("revision baseline");
        let content = format!("revision-{revision}\n");
        compare_and_swap_hotword_file(&path, &baseline, content.as_bytes(), || {})
            .expect("publish revision");
    }

    assert_eq!(
        fs::read_to_string(&path).expect("latest revision"),
        "revision-70\n"
    );
    let recovery_count = fs::read_dir(directory.path())
        .expect("recovery directory")
        .filter_map(Result::ok)
        .filter(|entry| entry.file_name().to_string_lossy().ends_with(".recovery"))
        .count();
    assert_eq!(recovery_count, 70);
}
