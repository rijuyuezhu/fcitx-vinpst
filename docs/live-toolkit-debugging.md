# Live toolkit debugging runbook

This runbook documents the reusable debugging method established while validating the real GTK3, Qt6, and Chromium/Ozone application paths on Wayland. It is not a chat transcript or a replacement for the migration status documents. Use it when a live probe appears to fail even though dictation works in ordinary applications.

## Scope

The validated path is:

```text
real desktop F9/F10 event
  -> retained Fcitx addon
  -> org.fcitx.Vinput
  -> native streaming ASR
  -> RecognitionPartial signals
  -> real application widget commit or command replacement
```

The retained evidence profile is `sherpa-native-command-live`. Normal mode uses F9. Command mode uses F10 and the deterministic `native-command-live-adapter` so that selected-text provenance can be checked without depending on a remote provider.

Live recipes are intentionally outside `just ci`. A compiled probe is only implemented evidence. A successful JSONL summary from a real desktop key event is live evidence.

## Evidence model

A toolkit run is accepted only when it proves both sides of the boundary:

1. the daemon emitted at least one same-run non-empty `RecognitionPartial` value;
2. the real application widget observed the final text change.

Do not require client-side toolkit preedit as the only partial signal. Fcitx may render input-panel preedit without exposing it through GTK `preedit-changed` or a non-empty Qt `QInputMethodEvent::preeditString()`.

Command runs additionally require:

- `selection-ready` after the application field has real focus;
- the selected text to equal the expected fixture (`selected text` by default);
- the final adapter result to contain the expected selected text;
- surrounding-text runs to perform replacement rather than accept an unrelated primary-selection value.

Retained summaries are under:

```text
target/tmp/ime-gtk3-native-live/{normal,command}.jsonl
target/tmp/ime-qt6-native-live/{normal,command}.jsonl
target/tmp/ime-chromium-native-live/{normal,command}.jsonl
```

## Fast triage

Before changing code, check the session and installed profile:

```sh
fcitx5-remote --check
gdbus call --session \
  --dest org.fcitx.Vinput \
  --object-path /org/fcitx/Vinput \
  --method org.fcitx.Vinput.Service.GetStatus
niri msg windows
```

The daemon should normally report `idle`. Confirm that the probe window exists and has focus before deciding that the shortcut or addon failed.

Inspect the latest evidence rather than an older background job:

```sh
tail -n 30 target/tmp/ime-gtk3-native-live/normal.jsonl
tail -n 30 target/tmp/ime-qt6-native-live/normal.jsonl
tail -n 30 target/tmp/ime-chromium-native-live/normal.jsonl
```

A useful summary must name the toolkit and mode and end with `"ok":true`.

## Failure patterns and fixes

### Speaker playback is not microphone input

**Symptom:** the probe reaches `ready`, `pw-play` runs, but no partial arrives.

**Cause:** the configured microphone does not capture the desktop output. Playing a WAV through the speakers is not a reliable audio-injection mechanism.

**Action:**

- use real speech for an interactive application probe; or
- use `ime-fcitx-virtual-source-live` for repeatable injected-audio Fcitx-client evidence.

Never label a speaker-to-microphone pickup attempt as retained audio proof.

### Probe window is hidden or unfocused

**Symptom:** the JSONL contains `ready`, but F9/F10 is not consumed.

**Action:** locate and focus the window explicitly:

```sh
niri msg windows
niri msg action focus-window --id <window-id>
```

A timeout with no key or partial event is usually a focus/readiness failure, not an ASR failure.

### GTK object lifetime failure after a successful commit

**Symptom:** GTK receives final text, the window closes, and the timeout callback later reads a destroyed `GtkEntry` or marks the run as failed.

**Root cause:** GLib sources outlived the widget, and the final text was read directly from the widget after destruction.

**Fix used:**

- retain `last_text` independently of the widget;
- retain and remove timeout/finish/selection source IDs;
- clear widget pointers in the destroy callback;
- call `gtk_main_quit()` only while a GTK main loop exists;
- determine completion from retained state rather than dereferencing a destroyed object.

Regression commit: `5ce94b9 fix(toolkit): stabilize GTK3 live evidence`.

### GTK/Qt client-side preedit is empty

**Symptom:** the application receives the final text, but GTK emits no useful `preedit-changed` value or Qt emits only an empty preedit.

**Cause:** the retained addon updates the Fcitx input panel; every frontend toolkit does not necessarily surface that text as client-side preedit.

**Fix used:** subscribe to daemon `RecognitionPartial` for streaming evidence and use the real widget's `changed`/`textChanged` event for final application evidence.

Regression commits:

```text
5ce94b9 fix(toolkit): stabilize GTK3 live evidence
7d975bf fix(toolkit): stabilize Qt6 live evidence
```

### Stale primary selection creates a false command pass

**Symptom:** command replacement succeeds, but the adapter input contains text selected earlier in another application instead of the probe field's `selected text`.

**Cause:** the probe selected its text before the field had real desktop focus. Fcitx therefore had no valid surrounding selection and fell back to the existing primary selection.

**Fix used:**

- wait until the field reports real focus;
- select the fixture after focus;
- emit `selection-ready` only after reading the exact selected range back;
- optionally set `VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING`;
- require the final adapter result to contain the selected fixture.

Regression commits:

```text
34a0ef1 fix(toolkit): prove GTK surrounding selection
5f06766 fix(toolkit): prove Qt surrounding selection
```

This distinction is important: surrounding-text replacement and primary-selection fallback are separate live cases and must not satisfy each other's gate.

### English abbreviations become `<unk>`

**Symptom:** Chinese text is committed correctly, while terms such as `GTK` are emitted as `<unk>`.

**Cause:** the current Chinese Zipformer model's language/token coverage.

**Action:** treat this as a model-quality issue unless partials or final commits are missing. Use pure Chinese utterances for toolkit transport validation, and use a bilingual model for a separate multilingual-quality test.

### Concurrent work contaminates a small commit

**Symptom:** `git status` contains unrelated live-test or documentation changes while a focused fix is being prepared.

**Action:**

- stage only explicit paths;
- use an index-only patch generated from `git show HEAD:<path>` when one file contains unrelated hunks;
- inspect `git diff --cached --stat` and `git diff --cached` before committing;
- never reset or discard another worker's uncommitted changes.

## Real application recipes

Run with a real desktop key event:

```sh
just ime-gtk3-native-live normal
just ime-gtk3-native-live command
just ime-qt6-native-live normal
just ime-qt6-native-live command
just ime-chromium-native-live normal
just ime-chromium-native-live command
```

For a strict command provenance check:

```sh
VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING='selected text' \
  just ime-gtk3-native-live command
VINPUT_TOOLKIT_EXPECTED_COMMIT_SUBSTRING='selected text' \
  just ime-qt6-native-live command
```

Chromium command mode applies the same selected-text assertion automatically.

## Expected JSONL sequence

Normal mode should resemble:

```json
{"event":"ready","toolkit":"gtk3","mode":"normal","manual_trigger":true}
{"event":"daemon-partial","text":"你好"}
{"event":"changed","text":"你好"}
{"event":"summary","toolkit":"gtk3","mode":"normal","partial":true,"commit":true,"ok":true}
```

Command mode should additionally include:

```json
{"event":"selection-ready","text":"selected text"}
{"event":"daemon-partial","text":"改得更简洁"}
{"event":"changed","text":"adapter-backed: selected text | command: 改得更简洁"}
{"event":"summary","selection_ready":true,"expected_commit":true,"replacement":true,"ok":true}
```

Exact recognized words are not the toolkit contract. The required contract is same-run partial evidence, one final application change, and correct selected-text provenance.

## Validated result set

The 2026-07-30 real-key validation produced `ok: true` for:

- GTK3 normal and command;
- Qt6 normal and command;
- Chromium/Ozone normal and command.

Chromium support was added by `7f270fc test(toolkit): add Chromium live probe`. The retained evidence was recorded by `6d24f39 docs(migration): record toolkit live evidence`.

## Extending the matrix

When adding another application or failure mode:

1. emit `ready` only after the target input is focused and prepared;
2. never synthesize the shortcut inside the toolkit;
3. isolate same-run daemon signals from unrelated sessions;
4. assert the final text in the real application surface;
5. keep surrounding-text and primary-selection fallback as distinct cases;
6. write JSONL to a stable `target/tmp` directory;
7. add a deterministic architecture/contract test for the probe;
8. run the narrow validation tier, then the full relevant gate before handoff.
