# Arch Linux installation and lifecycle guide

This guide describes the currently supported external-user path for the Rust rewrite.

## Support boundary

The checked release target is currently **Arch Linux x86_64**. The project can build and verify a publication-shaped release candidate containing one package, repository metadata, detached signatures, a strict manifest, and checksums.

A production repository, production signing key, and independently published production fingerprint do not exist yet. Do not treat artifacts under `target/tmp`, the release-gate bundle, its synthetic `pkgrel=2` package, or its ephemeral test key as a public release.

The Rust package conflicts with and provides `fcitx5-vinput` because the legacy and Rust implementations own the same Fcitx addon, D-Bus service, and user service. Keep only one implementation installed.

## 1. Verify a release candidate

Obtain all three items from appropriate channels:

- the release-candidate directory;
- the release public key from outside that directory;
- the expected primary-key fingerprint from an independently trusted channel.

Use the verifier from the matching audited source release:

```sh
candidate=/path/to/fcitx-vinput-rs-release-candidate
public_key=/path/from/trusted-channel/fcitx-vinput-rs-signing-key.asc
fingerprint=EXPECTED_PRIMARY_FINGERPRINT

scripts/release/verify-arch-release-candidate.sh \
  "$candidate" "$public_key" "$fingerprint"
```

The verifier checks the detached manifest signature, exact artifact inventory, checksums and sizes, package/repository signatures, package identity, architecture, version, and the single-package repository indexes. It rejects a public key located inside the candidate directory; the trust root must come from outside the bundle.

Do not continue after any verification error.

## 2. Install the package

Extract the verified package filename from the manifest and install it with pacman:

```sh
package_name="$(
  jq -r '.artifacts[] | select(.role == "package") | .name' \
    "$candidate/manifest.json"
)"
sudo pacman -U "$candidate/$package_name"
```

Finish any active recording before installation, upgrade, or removal. Pacman may require removal of the conflicting legacy `fcitx5-vinput` package; do not keep both addon implementations installed.

The package installs:

- `vinput`, `vinput-daemon`, and the Rust `vinput-gui` management application;
- the retained Fcitx5 addon and metadata;
- D-Bus activation metadata, a systemd user service, and the GUI desktop entry/icons;
- English fallback and zh_CN translations;
- the checked native sherpa-onnx and ONNX Runtime libraries under `/usr/lib/fcitx-vinput`;
- the packaged reference configuration and VAD asset.

It does not create or overwrite user configuration during a package transaction.

## 3. Initialize user state

Preview the paths first:

```sh
vinput init --dry-run --json
```

Then create the default user config and managed data/cache directories:

```sh
vinput init
config="${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinput/config.json"
vinput config validate "$config" --summary-only
```

`vinput init` is idempotent and does not overwrite an existing config unless `--force` is passed. Do not use `--force` during ordinary upgrades.

The packaged management GUI can be started from the desktop application menu as **Vinput Configuration** or from a terminal:

```sh
vinput-gui
```

The current GUI displays daemon/config state and the configured ASR, scene, LLM, adapter, and hotword resources. Its Control page edits the default language, capture target, active ASR provider, active scene, output ducking, and VAD settings, then validates and atomically saves the user config. Resources and LLM provide typed scene and LLM-provider lifecycle, managed model/provider/adapter install, update, retry, cancellation, details, guarded removal, and provider connectivity testing. Hotwords selects configured local or command ASR providers, sets or clears `hotwords_file`, resolves a relative Local hotword path only when the provider model path is absolute, rejects relative model-plus-hotword combinations whose target depends on the daemon process environment, requires an absolute path to edit command-provider content in the GUI, and loads, edits, and atomically saves UTF-8 content with config-target and content external-change detection, missing-local-file preparation before reload with fail-safe preservation after failure, version-checked no-replace claim-and-publish for existing content, adjacent recovery preservation of the previous file version, fail-closed refusal when the target is externally rewritten or recreated, a no-rollback policy after content publication, bounded final-state confirmation for active-provider reloads, and post-reload config-target revalidation so background preparation failures or superseding config changes preserve the file but are reported as not applied. Existing config files receive an adjacent `.bak`; a file changed after the GUI loaded is not overwritten. Saving is refused while a reachable daemon is recording or reports an active session. The same page starts and stops normal recording through D-Bus. A display-independent package self-check is available as `vinput-gui --check --offline`; it validates typed config loading without contacting D-Bus.

## 4. Install and select an ASR model

List available models through the configured registry mirrors:

```sh
vinput model list --available --config "$config"
```

Install a supported model and select it in the active local provider:

```sh
vinput model install <model-id-or-short-id> --config "$config"
vinput model use <model-id-or-short-id> \
  --config "$config" --in-place
```

The managed model root defaults to:

```text
${XDG_DATA_HOME:-$HOME/.local/share}/fcitx-vinput/models
```

Before downloading, `vinput model install ... --dry-run --json` prints the resolved archive, target directory, and staging plan. Model installation rejects unsafe archives and writes through a staging directory. When changing models after the daemon is running, add `--reload-daemon` to `model use` or restart the user service afterward.

## 5. Start the user service and reload Fcitx5

```sh
systemctl --user enable --now vinput-daemon.service
fcitx5 -r
```

Check the installation:

```sh
vinput doctor --config "$config"
vinput daemon status
vinput device list --config "$config"
```

`doctor` reports configuration, ASR construction, VAD, audio, activation, addon, and stale-owner diagnostics. Resolve reported ASR/model errors before testing dictation.

The default capture target is `default`. To select another enumerated PipeWire source:

```sh
vinput device use <target> --config "$config" --in-place
vinput daemon restart
```

### Custom provider CA and proxy environment

Remote ASR and OpenAI-compatible text providers use the standard proxy environment supported by reqwest. `HTTP_PROXY`, `HTTPS_PROXY`, and `NO_PROXY` may be set for the daemon process or for a one-shot `vinput llm test` command. A proxy URL may contain Basic credentials, but treat that URL as a secret and do not include it in logs or bug reports.

Set `SSL_CERT_FILE` to an absolute path containing a PEM certificate bundle when a provider must trust an additional private CA. The bundle must be a regular file no larger than 4 MiB. Its certificates are added to the built-in `WebPKI` roots; they do not replace public roots, and certificate verification is not disabled.

For the systemd user service, create a private environment file and reference it from a drop-in:

```sh
install -d -m 700 "${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinput"
cat >"${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinput/provider.env" <<'EOF'
SSL_CERT_FILE=/absolute/path/to/private-ca-bundle.pem
HTTPS_PROXY=http://proxy.example.test:3128
NO_PROXY=localhost,127.0.0.1
EOF
chmod 600 "${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinput/provider.env"

systemctl --user edit vinput-daemon.service
```

Use this drop-in, replacing the path when `XDG_CONFIG_HOME` is customized:

```ini
[Service]
EnvironmentFile=%h/.config/fcitx-vinput/provider.env
```

Then reload and restart the service:

```sh
systemctl --user daemon-reload
systemctl --user restart vinput-daemon.service
```

For an isolated CLI check, pass the environment only to that command:

```sh
SSL_CERT_FILE=/absolute/path/to/private-ca-bundle.pem \
HTTPS_PROXY=http://proxy.example.test:3128 \
vinput llm test <provider-id> --config "$config"
```

The deterministic tests cover Basic-authenticated CONNECT tunnels through both plain-HTTP and TLS-protected HTTPS proxy endpoints to CA-signed HTTPS fixtures. Provider success and error response bodies are capped at 1 MiB; a larger body is rejected before JSON parsing or error-body display. Provider redirects are disabled: a 3xx response is reported as an HTTP failure, and the advertised `Location` is not contacted. Text scenes that omit `timeout_ms` retain the legacy 4000 ms request deadline; `vinput llm test --dry-run --json` reports that effective value. They also cover one local interception relay that terminates and re-establishes verified TLS while retaining only TLS versions and byte counts, plus same-daemon atomic replacement of the certificate bundle at one unchanged `SSL_CERT_FILE` path with mismatch rejection and recovery. They do not establish support for PAC, NTLM/Kerberos, real enterprise proxy products, production certificate distribution, revocation, custody, interception audit policy, provider credential rotation, or compliance operations. Changing the `SSL_CERT_FILE` path itself still requires updating the process environment; the deterministic gate proves replacement of the file at an already configured path.

## 6. Upgrade

Verify the new candidate exactly as in step 1, then install the new package with `pacman -U`.

The package hook automatically scans ownership-verified live session buses. It skips sessions without an existing daemon owner, so an upgrade never starts vinput for an inactive user. For each existing owner it runs the guarded handoff as that user: current owners remain unchanged, a stale systemd owner reloads metadata and restarts the unit, and a stale direct D-Bus owner is signalled only after the same-user, non-systemd, idle, no-active-session checks pass and a new current owner can be verified.

After the transaction, check the current desktop session:

```sh
vinput daemon status
```

Run the manual fallback only when pacman reported an automatic handoff failure or status still reports an old path/deleted executable:

```sh
vinput daemon handoff
vinput daemon status
```

Reload Fcitx5 when the addon changed:

```sh
fcitx5 -r
```

Package upgrades preserve user config, models, and cache. Config schemas newer than the installed binary supports are rejected rather than rewritten.

## 7. Troubleshooting

Start with these non-destructive commands:

```sh
vinput doctor --config "$config"
vinput daemon status
vinput daemon log --lines 100
vinput activation-service --user-status
systemctl --user status vinput-daemon.service
```

Common boundaries:

- **No usable ASR backend:** inspect `vinput model list --installed`, install/select a supported model, and rerun `doctor`.
- **Stale daemon after upgrade:** inspect `daemon status`, then use `vinput daemon handoff` only when stale-owner diagnostics are present.
- **Addon not visible:** verify the package is installed, restart Fcitx5 with `fcitx5 -r`, and inspect the addon section of `doctor`.
- **No audio source:** run `vinput device list --config "$config"`; verify the user PipeWire session and select a valid target.
- **Private provider certificate is rejected:** verify that `SSL_CERT_FILE` names a readable PEM bundle, restart the daemon after changing its environment, and confirm the service drop-in points to the intended environment file. Client setup errors intentionally omit the local certificate path and file contents.
- **Removal is refused:** finish or stop the active recording and retry. The package intentionally refuses to terminate an active session.

For a reproducible bug report, retain the package version, architecture, `doctor` output, `daemon status`, the last relevant daemon log lines, and whether the problem occurs before capture, during ASR, in the Fcitx frontend, or only in one application.

Do not include recognized private text, audio samples, API keys, Bearer tokens, or complete private configuration files unless explicitly sanitized.

## 8. Remove the package

Finish any active recording, then remove the package normally:

```sh
sudo pacman -Rns fcitx-vinput-rs
fcitx5 -r
```

The package pre-remove hook performs a guarded all-session preflight before stopping eligible daemon owners. If any live user session is busy or cannot be verified safely, removal fails and activation metadata is restored.

User state is intentionally retained:

```text
${XDG_CONFIG_HOME:-$HOME/.config}/fcitx-vinput/
${XDG_DATA_HOME:-$HOME/.local/share}/fcitx-vinput/
${XDG_CACHE_HOME:-$HOME/.cache}/fcitx-vinput/
```

Review those directories before deleting them manually. Keeping them permits later reinstallation without redownloading models or recreating configuration.

## Evidence boundary

Repository tests prove candidate construction and verification, clean package build, package/repository signatures, isolated install/upgrade/rollback/removal, user-config preservation, direct and systemd handoff behavior, and guarded removal behavior.

They do not yet prove a production-hosted repository, production key operations, an actual package upgrade on an unrelated external user's machine, or a live production multi-user upgrade/removal. Those remain release-readiness work.
