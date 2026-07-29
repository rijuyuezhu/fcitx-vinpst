# Registry contract

`vinput-registry` owns local registry metadata parsing and planning. It stays separate from download and extraction side effects so CLI validation can run deterministically in tests and smoke checks.

## Module layout

The registry crate is split so each side-effectful boundary stays reviewable:

- `schema.rs`: legacy dry-run registry index, model, adapter, asset, summary, validation, and URL resolution helpers;
- `live.rs`: live registry v2 `registry/models.json` parsing, `short_id` lookup, i18n title/description fallback, and typed/raw `vinput_model` metadata;
- `script.rs`: live `registry/providers.json` and `registry/adapters.json` parsing, legacy-compatible managed script paths, mirror-backed executable publication, environment defaults, and guarded config materialization;
- `installed.rs`: shared installed-model discovery, typed `vinput-model.json` loading, stable registry-id/display-title access, and regular-file inventory for both the current flat Rust managed layout and the legacy two-level engine/model layout;
- `plan.rs`: planned assets, dry-run install plans, checksum policy planning, and target path calculation;
- `error.rs`: `RegistryError`;
- `fetch.rs`: registry text fetch boundary, ordered mirror fallback, and the concrete `ReqwestRegistryTextSource` for HTTP index text fetching;
- `cache.rs`: text-only registry index cache read/write boundary with same-directory temporary file and rename updates;
- `checksum.rs`: SHA-256 verification helpers for in-memory bytes, readers, and files;
- `asset.rs`: asset download/staging boundary, concrete `ReqwestRegistryAssetSource`, temp-file staging, checksum verification, and final staged-file publication;
- `archive.rs`: archive extraction safety policy helpers plus plain-tar `stage_tar_archive`, zstd-compressed `stage_tar_zst_archive`, and bzip2-compressed `stage_tar_bz2_archive` extraction-to-staging boundaries;
- `staging.rs`: side-effect-free archive path planning from one dry-run asset action or a full dry-run install plan to staged asset, extraction tree, and materialization target paths;
- `materialize.rs`: staged-tree materialization boundary using direct renames when possible, a target-filesystem copy plus sibling rename fallback for cross-device publication, and explicit rollback errors for replacement failures;
- `install.rs`: live model install orchestration that connects injectable asset download, checksum verification, archive staging, vinput-model.json writing, and materialization without mutating user config;
- `tests.rs`: behavior-preserving schema, safety, planning, injected-source fetch, local HTTP fetch, stale-cache fallback, checksum helper, asset staging, tar/tar.zst/tar.bz2 archive staging, live model install orchestration, materialization, and archive safety policy coverage.

Future archive wrappers beyond plain tar, `.tar.zst`, and `.tar.bz2`, richer config mutation code, and active-model selection should use separate modules or explicit sub-boundaries and must not be hidden inside schema, dry-run planning, concrete HTTP text fetch code, text cache code, checksum helper code, asset staging code, archive path planning code, archive staging code, install orchestration, or materialization code.

## Registry shape

The historical Rust dry-run fixture remains an `index.json`-style JSON object with:

- `version`: registry schema version.
- `models`: ASR model entries with `id`, `label`, `provider`, and `assets`.
- `adapters`: optional text adapter entries with `id`, `label`, `kind`, and `assets`.

Each asset path must be a safe relative path. Optional `sha256` checksums must be lowercase 64-character hexadecimal strings.

The live registry v2 model catalog is a separate `registry/models.json` shape with:

- `version`: live registry schema version.
- `items`: ASR model entries with `id`, `short_id`, ordered `urls`, `sha256`, `size_bytes`, `language`, and optional inline `title`/`description`.
- `vinput_model`: typed metadata for fields such as `backend`, `family`, `model_type`, `runtime`, `supports_hotwords`, plus raw backend-specific `recognizer` and `model` JSON subtrees.

Live `i18n/*.json` files are flat string maps. Model display text resolves through inline `title`/`description`, then `<model-id>.title` / `<model-id>.description` i18n keys, then `short_id` or full `id` fallback for the title.

The live adapter catalog is `registry/adapters.json` with `version` and `items`. Each item has a stable `adapter.<parent>.<leaf>` id, optional `short_id`, launch `command`, ordered `script_urls`, optional `readme_url`, and environment specifications. The managed relative path follows the legacy rule: the second id segment is the directory and all remaining segments form the filename, so `adapter.mtranserver.proxy` becomes `mtranserver/proxy`.

The provider catalog is `registry/providers.json` with the same shared fields plus `stream`. Provider ids use the `provider` prefix and `stream` must agree with the `.streaming` suffix because command ASR protocol selection remains id-based for legacy compatibility. The same path rule maps `provider.openai-compatible.streaming` to `openai-compatible/streaming` and `provider.vinput.remote.streaming` to `vinput/remote.streaming`.

Provider and adapter display text uses the same root-level flat `i18n/<locale>.json` map as model metadata. `<full-id>.title` resolves the display title and `<full-id>.description` resolves the optional description. Missing or blank titles fall back to `short_id`, then full id; missing descriptions remain absent. Localization never changes machine ids or selector behavior. CLI `--locale` chooses the mirror file, while `--i18n` injects a deterministic local map. Fetch/parse failure is reported in the i18n diagnostic object and falls back without failing the registry list.

`install_live_script` stages mirror responses through the shared asset boundary, publishes only a complete file, and adds executable bits. `materialize_llm_adapter` writes one script argument, creates blank values for newly declared environment keys, preserves existing environment values and forward-compatible fields, and refuses to overwrite an adapter whose arguments do not already point at the expected managed script. `materialize_asr_provider` creates a command provider with the legacy 60000 ms timeout, preserves existing positive timeout/model/environment values, adds newly declared environment keys, and refuses to overwrite non-command or non-managed providers.

## CLI diagnostics

`vinput-cli registry` prints the configured registry mirror URLs from the bundled config. File-backed diagnostics use explicit paths:

```sh
cargo run -q -p vinput-cli -- registry validate data/sample-registry-index.json
cargo run -q -p vinput-cli -- registry plan data/sample-registry-index.json --summary-only
```

These commands parse local JSON only. They do not download assets or touch install directories.

The live provider and adapter CLIs use the current upstream script registries:

```sh
cargo run -q -p vinput-cli -- provider list --available --json
cargo run -q -p vinput-cli -- provider list --available --locale zh_CN --json
cargo run -q -p vinput-cli -- provider install <id-or-short-id> --dry-run --json
cargo run -q -p vinput-cli -- adapter list --available --json
cargo run -q -p vinput-cli -- adapter list --available --locale zh_CN --json
cargo run -q -p vinput-cli -- adapter install <id-or-short-id> --dry-run --json
cargo run -q -p vinput-cli -- adapter remove <id-or-short-id> --registry registry/adapters.json --adapter-root <root> --dry-run --json
cargo run -q -p vinput-cli -- adapter start <id-or-short-id> --registry registry/adapters.json --config <config> --dry-run --json
cargo run -q -p vinput-cli -- adapter status <id-or-short-id> --registry registry/adapters.json --config <config> --dry-run --json
```

Omitting `--registry` for list/install fetches `registry/providers.json` or `registry/adapters.json` from configured mirrors. Real installs download into `$XDG_DATA_HOME/fcitx-vinput/providers` or `$XDG_DATA_HOME/fcitx-vinput/adapters`, mark the script executable, and write configuration through the same output/in-place/backup policy as other config mutations. Adapter removal accepts an exact configured id directly; resolving a short id requires an explicit local `--registry`. A configured adapter is considered managed only when its sole argument equals `<adapter-root>/<legacy-relative-path>`. In-place removal then deletes that regular file or symlink after writing the backed-up config. `--output` writes only the derived config and preserves the script because the input config still references it. Non-managed or user-defined adapter files are never deleted. Adapter start, stop, and filtered status use the same exact-id-first, explicit-registry-short-id resolution policy, validate that the resolved machine id is installed in the selected config, and reject before D-Bus otherwise. Unfiltered status does not require config resolution. Dry-run performs registry, path, protocol, overwrite, removal, selector, and config validation without downloading, writing, or contacting D-Bus.

The library exposes `fetch_registry_index_from_mirrors` as the shared mirror fallback boundary. It iterates mirror URLs through a `RegistryTextSource`, falls through on transport failures, stops on the first fetched-but-invalid registry body, and performs the same `RegistryIndex` validation as file-backed CLI diagnostics. `ReqwestRegistryTextSource` is the implemented concrete HTTP registry index text source behind that boundary; it fetches JSON text from mirror URLs with sanitized transport/status errors and no auth/header/body leakage. `RegistryTextCache` and `fetch_registry_index_with_cache` are implemented as a text-only stale-cache boundary: fresh successful fetches parse before writing cache, write cache through a temporary file plus rename, and fall back to stale cache only when fresh mirror fetch fails. `sha256_hex`, `verify_sha256_bytes`, `verify_sha256_reader`, and `verify_sha256_file` are implemented checksum helpers; they require lowercase 64-character expected checksums and report mismatch or read/open failures with typed sanitized errors. `stage_planned_asset` and `ReqwestRegistryAssetSource` are implemented as an asset download/staging boundary: candidate asset URLs are local/test-injectable through `RegistryAssetSource`, downloaded bytes are written to a temp file, declared SHA-256 checksums are verified before publication, missing checksums are reported as an explicit `AssetChecksumStatus::Missing`, and final staged output is published only after validation. `checked_archive_entry_target` remains the shared filesystem-free archive safety policy helper. `stage_tar_archive` handles already-staged local plain tar archives, `stage_tar_zst_archive` handles already-staged local zstd-compressed tar archives, and `stage_tar_bz2_archive` handles already-staged local bzip2-compressed tar archives; all use the same temporary-tree validation and publication flow. `materialize_staged_tree` first attempts a direct rename. On `CrossesDevices`, it copies only regular files/directories into a hidden sibling on the target filesystem and atomically renames that completed sibling into place. Existing targets are still moved to a same-directory backup before replacement, rollback remains active on publish failure, and hidden publish/backup trees are cleaned after success. `install_live_model` also resets a stale extraction directory before each retry, while retaining the checksum-verified archive cache. It downloads through an injectable asset source, verifies declared SHA-256, stages the archive, writes `vinput-model.json`, materializes the model directory, and deliberately does not mutate user config. The optional installed `display` metadata stores the full registry id, inline fallback title, and locale-keyed titles captured from the registry i18n file; older metadata without this object remains valid. `InstalledModelInfo::stable_model_id` and `display_title` expose that data without requiring the daemon or C++ frontend to reopen registry files. `scan_installed_models` is the shared local discovery boundary used by CLI and daemon/frontend model menus. It walks only `<root>/<managed-name>/vinput-model.json` and legacy `<root>/<engine>/<name>/vinput-model.json`, never recurses through model asset trees while discovering models, and emits a stable `model.<engine>.<name>` id for the legacy layout. Archive wrappers beyond plain tar, `.tar.zst`, and `.tar.bz2` remain future work. Active-model selection and guarded config mutation are implemented by the CLI and daemon layers on top of these registry primitives.

Dry-run install plans keep install roots explicit: an empty root keeps relative target paths, the filesystem root stays absolute, and non-root roots are joined with registry-relative asset paths without touching the filesystem.

## Fixture

`data/sample-registry-index.json` is the stable smoke fixture for registry validation and planning. Integration tests also consume it directly so changes to registry parsing, planning, or fixture format fail before smoke output drifts.

The committed sample intentionally fixes these contract ids:

- model `sherpa-zh-small` with provider `sherpa-onnx` and asset `models/sherpa-zh-small.tar.zst`.
- adapter `mock-adapter` with kind `command` and no bundled assets yet.

Treat these as smoke-test fixtures rather than a real downloadable registry catalog.
