# Function gap audit

Reviewed: 2026-08-06

This document is the current implementation/readiness summary. The generated source/callable baseline is tracked under [`../legacy/`](../legacy/README.md), user-task mappings live in [`user-capability-audit.md`](user-capability-audit.md), detailed evidence lives in [`e2e-capability-matrix.md`](e2e-capability-matrix.md), and priorities live in [`e2e-replication-plan.md`](e2e-replication-plan.md).

## Review baseline

- Vinpst implementation: `main` at `102413d5` when this review started.
- Upstream reference: `xifan2333/fcitx5-vinput` at `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9` (`v2.3.5-1-g6cdcac8`).
- Generated upstream scope: 164 production C/C++ files, 28,168 lines, and 1,559 function/prototype/signal/slot occurrences.
- Product target: practical user-capability parity under independent Vinpst identities and paths.

## Executive conclusion

Vinpst already provides the core product experience expected for a voice-input system:

- normal dictation with streaming partials and final Fcitx commits;
- selected-text command editing with candidates and failure-safe replacement;
- local, command, and OpenAI-compatible remote ASR;
- PipeWire capture, device selection, gain/normalization, Silero VAD, hotwords, and output ducking;
- scene, LLM provider, adapter, model, and provider management;
- Fcitx keys, Tap/Hold/Both behavior, Scene/ASR menus, notifications, localization, and owner recovery;
- CLI diagnostics and a Rust/Iced management GUI;
- checked Arch, Debian, RPM, Nix, Flatpak, source-archive, manifest, and signing boundaries at different publication-readiness levels.

The project is no longer blocked on a missing core dictation or command-editing implementation. The active 0.1.0 work is exhaustive user-capability review, accessibility/result-path closure in the GUI, user documentation, selection of the public artifact matrix, release-workflow integration, and unrelated-environment validation.

Vinpst is not an in-place replacement for the upstream package. Its package, executable, addon, D-Bus, service, environment-variable, and XDG identities remain Vinpst-only, and no upstream migration or pre-0.1.0 internal compatibility is required.

## Readiness summary

| Area | State | Release-relevant remainder |
| --- | --- | --- |
| Normal desktop dictation | `live-proven` | Broader application/device breadth is useful but no core task is missing. |
| Command editing | `live-proven` | Broader application and real hosted-provider operations. |
| Trigger modes, keys, menus, candidates | `live-proven` | No practical parity gap currently identified. |
| Local ASR | `deterministic`; representative `live-proven` | Add model layouts only for real registry/user demand. |
| Command ASR | `deterministic`; independent Whisper `live-proven` | No known ordinary workflow gap. |
| Remote ASR | `deterministic`; loopback `live-proven` | Hosted-provider operational and credential evidence. |
| Audio/VAD/device/output ducking | `deterministic`; representative `live-proven` | Additional physical-device and audible-output breadth. |
| Scenes/LLM/adapters | `deterministic`; command replacement `live-proven` | Hosted-provider evidence and broader GUI error categories. |
| Registry/resource lifecycle | `deterministic` | Representative live install/recovery/removal result paths. |
| Fcitx localization/notifications | `live-proven` for English and zh_CN | Additional locales are optional expansion. |
| Remote text HTTP/WebSocket | `deterministic`; same-host browser `live-proven` | A separately confirmed physical-device collector run. |
| CLI management and diagnostics | `deterministic` | UX polish from concrete audit findings. |
| Rust management GUI | packaged interactive baseline; desktop keyboard/IME paths `live-proven` | Assistive-technology semantic tree/policy, broader error taxonomy, and remaining live result paths. |
| Arch package/repository/signature/candidate | `deterministic`; explicit package smoke; tag job consumes the byte-identical source-job archive | Production package/repository signing and unrelated-environment validation. |
| Debian 12 / Ubuntu 24.04 | Docker install/upgrade/remove transactions complete; tag jobs build from the one source-job archive | Production publication and unrelated-environment validation. |
| RPM family | build and isolated transaction baseline | Fedora/openSUSE support claims require distro/repository/signing/SELinux/live-scriptlet evidence. |
| Nix | locked closure build baseline | Binary-cache publication policy if selected. |
| Flatpak | checked extension transaction baseline; tag job consumes the byte-identical source-job archive | Live host desktop/Fcitx/PipeWire/systemd and publication/signing policy. |
| User documentation | initial MkDocs user guide implemented | Keep strict build green and validate it against the release artifacts. |
| Exhaustive upstream review | generated file/callable inventory implemented | Review every delta through the user-capability table before the release candidate. |

## Highest-risk gaps

1. **Audit completeness:** the generated inventory detects source/callable drift, but every current and future delta still needs a human user-capability classification.
2. **GUI accessibility:** keyboard operation is proven, but a real assistive-technology semantic tree is not yet established. This needs an explicit 0.1.0 support decision rather than an implicit claim.
3. **GUI result-path breadth:** common configuration and interaction paths are proven; remaining install/recovery/resource mutation outcomes need representative real-session evidence.
4. **Release assembly:** the current Arch, Debian, and Flatpak publication matrix now builds from one exact source-job archive, but production package/manifest signing, required-check policy, and unrelated-environment validation remain.
5. **Operational external evidence:** hosted providers, an unrelated machine/user, production signing/key custody, and some long-duration/device/application breadth remain.

These are release and evidence risks. They do not justify changing Vinpst identities to upstream names.

## Improvements beyond the upstream implementation

Vinpst intentionally changes implementation and management design where that produces a clearer or safer product:

- Rust-owned typed runtime, configuration, registry, frontend-policy, and GUI boundaries;
- deterministic file-input, private-session-bus, temporary-HOME, package-transaction, and display-independent GUI paths;
- checksum-verified downloads, safe extraction, staged publication, managed-root guards, and conflict-aware atomic config writes;
- bounded process groups, deadlines, descendant cleanup, and independent stdout/stderr limits for helper providers;
- redacted typed diagnostics, owner/runtime visibility, prepare-before-swap provider reload, and failure preservation;
- a standalone Rust management GUI rather than a Qt source-level port;
- generated upstream drift inventory plus a user-task review layer.

## Completion gate

Before 0.1.0, the following path must work from a produced release artifact without manual JSON editing:

```sh
vinpst init
vinpst model list --available
vinpst model install <id-or-short-id>
vinpst model use <id-or-short-id> --in-place --reload-daemon
vinpst doctor
vinpst daemon status
```

The same installation must then pass live normal dictation, command replacement, scene/ASR selection, restart/reload/owner recovery, required GUI management tasks, and removal with Vinpst user state preserved.

The final review must refresh the upstream inventory, resolve every meaningful `missing` user task, document all evidence-only limitations, build the MkDocs site strictly, and validate the selected release artifacts on an unrelated environment.
