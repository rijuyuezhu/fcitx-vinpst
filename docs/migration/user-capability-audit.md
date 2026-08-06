# User-capability parity audit

Reviewed: 2026-08-06

This audit maps user-visible capabilities from the upstream C++ project to Vinpst. It is the review layer above the generated [`../legacy/upstream-source-inventory.json`](../legacy/upstream-source-inventory.json), which records 164 production files and 1,559 callable occurrences at upstream commit `6cdcac8b4300ff347ad3157bf61cd09a5302f7a9`.

## Scope and non-goals

The target is practical feature parity: a Vinpst user should be able to complete substantially the same useful voice-input, command-editing, resource-management, configuration, and diagnostic tasks.

The following differences are intentional and are not parity gaps:

- package, executable, addon, D-Bus, systemd, environment-variable, and XDG names use `vinpst` or `fcitx-vinpst`;
- Vinpst does not provide, conflict with, replace, import, or migrate another package or installation;
- CLI command spelling and output structure may differ when the Vinpst interface is clear and complete;
- the management GUI is Rust/Iced rather than Qt and need not reproduce widget hierarchy or layout;
- low-level C++ functions may map to one Rust abstraction, and one upstream function may have no direct counterpart when its user-visible behavior is provided elsewhere;
- pre-0.1.0 Vinpst interfaces may change without compatibility aliases.

## Evidence labels

- `implemented`: the production path exists.
- `deterministic`: automated tests exercise the externally observable behavior without claiming a real desktop or hosted provider.
- `live-proven`: retained evidence crosses the actual desktop/application/device boundary described by the row.
- `partial`: the normal path exists but a meaningful user task or required management path remains incomplete.
- `not applicable`: the upstream mechanism is unnecessary because Vinpst provides the task through a different product design.
- `missing`: a meaningful upstream user task has no usable Vinpst path.

A row may list both implementation and evidence, for example `implemented; live-proven`.

## Generated review baseline

| Upstream area | Files | Lines | Review purpose |
| --- | ---: | ---: | --- |
| CLI | 44 | 4,327 | User commands, configuration/resource editing, daemon control, and diagnostics. |
| Daemon backend | 36 | 8,965 | Audio, ASR, text processing, runtime ownership, remote text, and recovery. |
| Fcitx addon | 6 | 3,503 | Keys, menus, preedit, candidates, commits, selected text, notifications, and localization. |
| Qt GUI | 23 | 4,031 | User management tasks to reproduce in the Rust GUI, not toolkit/layout details. |
| Shared common library | 55 | 7,342 | Config, D-Bus payloads, registry/resource safety, i18n, and shared models. |

Every file is present in [`../legacy/source-annotations.md`](../legacy/source-annotations.md). Function/prototype/signal/slot entries are reviewed through the user journeys below; source additions are detected by the scheduled inventory drift workflow.

## User journeys

| User journey | Upstream source areas | Vinpst implementation | State and evidence | Remaining functional work |
| --- | --- | --- | --- | --- |
| Install the product and discover the Fcitx addon | build/package metadata, addon registration, daemon service | checked Arch, Debian, RPM, Nix, and Flatpak packaging; `vinpst`, `vinpst-daemon`, `vinpst-gui`; `fcitx5-vinpst.so`; `org.fcitx.Vinpst`; `vinpst-daemon.service` | implemented; deterministic package transactions for current checked paths | Publish the selected 0.1.0 artifact matrix and verify one unrelated machine. No upstream-package migration is planned. |
| Initialize user configuration and managed directories | CLI init and config store | `vinpst init`, typed config defaults, XDG roots, guarded first-file creation | implemented; deterministic | Final release documentation and artifact-installed smoke. |
| Start, stop, restart, inspect, and recover the daemon | daemon CLI, D-Bus service, process lifecycle | D-Bus activation, systemd user service, `vinpst daemon *`, owner diagnostics, guarded handoff, removal preflight | implemented; deterministic; owner-loss/reload paths live-proven | Actual package-installed multi-user lifecycle proof remains release evidence work. |
| Dictate normally into applications | addon input path, recorder, native/command/remote ASR | Fcitx trigger, PipeWire capture, streaming partials, final commit | implemented; live-proven with isolated audio, physical microphone, GTK, Qt, Chromium, GNOME Text Editor, kitty, and VS Code/Electron | Broader application and physical-device breadth; no known core task is missing. |
| Edit selected text by voice | surrounding-text access, command ASR, scenes, post-processing, candidates | Fcitx selected text plus primary-selection fallback, command scene, adapter/LLM processing, guarded delete-and-replace | implemented; live-proven for local adapter and independent loopback HTTP provider, including failure preservation and no-selection refusal | More applications and real hosted-provider operations remain evidence breadth. |
| Choose Tap, Hold, or Both trigger behavior | addon key-event state machine and Fcitx config | Rust-owned trigger state machine with thin Fcitx timer/key adapter; persistent Fcitx settings | implemented; deterministic; live-proven | None identified for practical parity. |
| Configure dictation, command, scene-menu, ASR-menu, and paging keys | Fcitx configuration form | official Fcitx addon configuration with English/zh_CN labels and persisted key lists | implemented; live-proven | Upstream default key values and labels need not match; Vinpst documents its own defaults. |
| Browse and select scenes and ASR targets from Fcitx | addon menus, filtering, paging, candidate actions | Rust menu controllers/projections with thin Fcitx candidate rendering | implemented; live-proven | None identified for practical parity. |
| See partials, status, errors, and notifications | addon presentation and notification paths | preedit/status priority, final candidates, localized information/error notifications, owner reconciliation | implemented; live-proven for retained categories | Broaden notification categories only when a concrete user-visible case is found. |
| Use local native ASR models | model manager, sherpa backend families | typed registry metadata and sherpa-onnx backends for current supported families | implemented; deterministic real-WAV coverage; representative live proof | Add model layouts only when registry entries or user demand require them. |
| Use an external command ASR provider | command backend and process bridge | command providers, raw/WAV bridge, bounded process-group supervision, provider script management | implemented; deterministic; independent Whisper live-proven | No known core task missing. |
| Use an OpenAI-compatible remote ASR provider | remote provider client | multipart WAV transport, authentication, model/language/prompt fields, deadlines, bounded bodies, redacted failures | implemented; deterministic network semantics; loopback live-proven | Real hosted-service operational evidence and credential procedures. |
| Select microphones and tune capture behavior | PipeWire devices, gain, normalization, VAD | `vinpst device`, GUI Control page, typed PipeWire recorder, gain/normalization, Silero VAD | implemented; deterministic; two-source switching and physical microphone live-proven | Additional Bluetooth/USB/hot-plug/channel-layout breadth. |
| Duck output while recording and restore it | output-ducker files | Rust daemon/audio output-ducking boundary using bounded `wpctl` control and restoration | implemented; deterministic; isolated real-`wpctl` live proof | Audible physical-output breadth is evidence work, not a missing setting. |
| Configure hotwords | provider config and GUI/CLI editing | `vinpst hotword` plus GUI provider/path/content workflow with bounded safe writes | implemented; deterministic; portal and config mutation live-proven | Provider/model support remains capability-dependent. |
| Create, edit, select, and remove scenes | scene config and Qt scene page | CLI and Rust GUI typed scene lifecycle, provider selection, candidates, timeout, context | implemented; deterministic | None identified for the ordinary management workflow. |
| Configure and test LLM providers | LLM config and Qt LLM page | OpenAI-compatible provider add/edit/test/remove with secure inputs and redacted diagnostics | implemented; deterministic; loopback command replacement live-proven | Real hosted-provider operations and credential lifecycle evidence. |
| Install and manage text adapters | adapter registry, process manager, Qt LLM page | registry install/update/remove, custom adapters, process start/stop/status, guarded script editing | implemented; deterministic | Broader resource-specific error messages in the GUI. |
| Install, update, select, and remove models/providers/adapters | registry fetch/cache/download/extraction and Qt resource page | shared Rust registry with mirror fallback, checksum validation, safe extraction, staging, atomic publication, localized metadata, managed-root guards | implemented; deterministic; model install/rendered-row/inactive managed removal live-proven through loopback registry and private daemon | Provider/adapter install-recovery and remaining mutation result paths still need representative live proof. |
| Manage settings without editing JSON | Qt pages and Fcitx config | Rust GUI Control/Resources/LLM/Hotwords pages plus focused CLI commands and editor flows | implemented; deterministic; representative GUI desktop interaction live-proven | `0.1.0` explicitly supports keyboard operation but not a screen-reader semantic tree. Use `vinpst` for management/diagnostics and `fcitx5-configtool` or the guarded terminal configuration-file fallback for frontend-only settings; broader error taxonomy remains. |
| Open the management application from a desktop environment | Qt desktop entry and Fcitx external option | packaged `vinpst-gui.desktop`, desktop launcher, direct `vinpst-gui` command | implemented; deterministic package checks | The exact upstream Fcitx “open settings” option is not required because the same task has a normal desktop/CLI entry. |
| Use the remote text browser/WebSocket interface | remote text daemon and web assets | Axum HTTP/WebSocket runtime, browser UI, authentication, debounce, provider selection, daemon ownership | implemented; deterministic; same-host Chromium path live-proven | Successful proof from a separately confirmed physical device remains an evidence gap. |
| Use English or Simplified Chinese interfaces | common/addon/GUI i18n | English fallback and zh_CN Fcitx plus Rust GUI presentation | implemented; deterministic; desktop localization live-proven | Additional locales are optional expansion beyond the current parity target. |
| Diagnose configuration, ASR, audio, activation, addon, and owner failures | CLI status/diagnostic actions | `vinpst doctor`, `asr-state`, `audio-devices`, daemon status/log, redacted typed diagnostics | implemented; deterministic | Continue improving messages from concrete failures; arbitrary third-party response-secret detection is not claimed. |
| Preserve user state during Vinpst upgrades/removal | config store and package lifecycle | package-owned files remain separate from user XDG state; guarded service handoff/removal; future-schema refusal | implemented; deterministic package transactions | Actual host and multi-user release proof. This applies only to Vinpst-to-Vinpst lifecycle, not upstream migration. |

## Function-level review method

The generated inventory is exhaustive at the source/callable occurrence level, while this table is exhaustive at the user-task level. Review proceeds as follows:

1. refresh the clean upstream inventory;
2. inspect every added, removed, or changed file/callable occurrence;
3. decide whether it contributes to an existing user journey, creates a new one, or is implementation-only;
4. update the corresponding journey, implementation mapping, evidence, or explicit `not applicable` rationale;
5. add behavior tests or live evidence only where the user-visible contract is not already established.

A callable is not considered “ported” merely because a similarly named Rust function exists. Conversely, a callable does not require a direct port when its behavior is absorbed by a safer Rust abstraction or is specific to Qt/C++ mechanics with no independent user task.

## Current release blockers from this audit

1. Complete review of every inventory delta against the journey table before the release candidate.
2. Add representative live proof for managed script update/replacement and remaining GUI resource-mutation result paths; checksum-verified model install/rendered reconciliation/inactive removal plus command ASR provider and required-environment text-adapter published-script config-only recovery are complete.
3. Complete production manifest/package signing policy and artifact-installed smokes for the selected release matrix.
4. Run the release candidate on an unrelated user or machine and repeat normal dictation, command replacement, diagnostics, and removal.
5. Keep the user documentation and strict MkDocs build green, including the explicit keyboard-supported/screen-reader-unsupported GUI policy and fallback paths.

No current blocker requires changing Vinpst identities to upstream names or adding package/D-Bus/path compatibility.
