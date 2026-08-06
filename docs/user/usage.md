# Dictation and command mode

## Normal dictation

Normal dictation records audio, sends it to the active ASR backend, and commits the final text through Fcitx.

When the backend supports streaming, Vinpst shows partial recognition as preedit before the final commit. A batch-only backend returns only the final result.

You can also control recording from a terminal:

```sh
vinpst recording status
vinpst recording start
vinpst recording stop
```

The Fcitx trigger is the normal desktop path; CLI recording commands are useful for diagnostics and automation.

## Trigger modes

The Fcitx addon supports three trigger modes:

- **Tap**: press once to start and press again to stop;
- **Hold**: recording starts after the hold threshold and stops on release;
- **Both**: short presses toggle, while a sustained press behaves as push-to-talk.

Change the mode and keys in the Fcitx addon configuration. The daemon JSON configuration does not own these frontend keys.

## Command editing

Command mode combines:

1. the selected text from the focused application;
2. speech recognized by the active ASR backend;
3. the built-in command scene;
4. a configured LLM provider or text adapter.

The result replaces the selected text only after processing succeeds. When the application does not expose selected surrounding text, Vinpst can fall back to the Wayland primary selection where available. If both are empty, command mode refuses to start rather than deleting unrelated text.

## Candidates

A scene can request multiple candidates. Vinpst shows them through the Fcitx candidate list; select a candidate to commit it. Escape closes filters and menus without committing.

## Scene and ASR menus

- Open the ASR menu with the configured ASR-menu key, currently `F8` by default.
- Open the scene menu with the configured scene-menu key, currently Right Shift by default.
- Use the configured previous/next-page keys when a menu spans multiple pages.

Provider/model switches are applied through the daemon. If a requested backend cannot be prepared, Vinpst keeps the previous working backend active and reports the failure.

## Application behavior

Vinpst relies on Fcitx application integration. Normal dictation works wherever ordinary Fcitx commits work. Selected-text replacement additionally depends on the application's surrounding-text support or a usable primary selection.

The project has live evidence for GTK, Qt, Chromium/Ozone, GNOME Text Editor, kitty, and VS Code/Electron paths. Application-specific behavior can still vary; see [Known limitations](limitations.md).
