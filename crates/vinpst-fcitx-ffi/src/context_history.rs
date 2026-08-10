//! Recent-input context history ownership for the retained Fcitx adapter.

use std::{
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

use vinpst_config::{VinpstConfig, user_paths};
use vinpst_text::{
    append_recent_input_context_buffer, append_recent_input_context_entry,
    default_context_cache_path_for_current_user, truncate_recent_input_context_cache,
};

use crate::ffi_string::text_input;

const USER_SOURCE: &str = "user";
const MAINTENANCE_INTERVAL: u32 = 100;

#[derive(Debug)]
struct ContextHistory {
    path: PathBuf,
    max_context_lines: u8,
    buffered_text: String,
    buffered_context: Option<usize>,
    suppress_next_commit: Option<String>,
    write_count: u32,
}

impl Drop for ContextHistory {
    fn drop(&mut self) {
        self.flush();
    }
}

impl ContextHistory {
    fn current() -> Self {
        Self {
            path: default_context_cache_path_for_current_user(),
            max_context_lines: load_max_context_lines(),
            buffered_text: String::new(),
            buffered_context: None,
            suppress_next_commit: None,
            write_count: 0,
        }
    }

    fn reload(&mut self) {
        self.max_context_lines = load_max_context_lines();
    }

    fn append_entry(&mut self, text: &str, source: &str) {
        if text.is_empty() {
            return;
        }
        self.flush();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        if append_recent_input_context_entry(&self.path, text, source, timestamp).unwrap_or(false) {
            self.write_count = self.write_count.saturating_add(1);
            self.maybe_truncate();
        }
    }

    fn on_user_commit(&mut self, context: usize, text: &str) -> bool {
        if text.is_empty() {
            return false;
        }
        if let Some(suppressed) = self.suppress_next_commit.take()
            && suppressed == text
        {
            return false;
        }
        if self
            .buffered_context
            .is_some_and(|current| current != context)
        {
            self.flush();
        }
        self.buffered_context = Some(context);
        let should_flush = append_recent_input_context_buffer(&mut self.buffered_text, text);
        if should_flush {
            self.flush();
            false
        } else {
            true
        }
    }

    fn suppress_next(&mut self, text: &str) {
        self.suppress_next_commit = (!text.is_empty()).then(|| text.to_owned());
    }

    fn context_destroyed(&mut self, context: usize) {
        if self.buffered_context == Some(context) {
            self.flush();
        }
    }

    fn flush(&mut self) {
        if self.buffered_text.is_empty() {
            self.buffered_context = None;
            return;
        }
        let text = std::mem::take(&mut self.buffered_text);
        self.buffered_context = None;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_secs());
        if append_recent_input_context_entry(&self.path, &text, USER_SOURCE, timestamp)
            .unwrap_or(false)
        {
            self.write_count = self.write_count.saturating_add(1);
            self.maybe_truncate();
        }
    }

    fn maybe_truncate(&mut self) {
        if self.max_context_lines == 0 || self.write_count < MAINTENANCE_INTERVAL {
            return;
        }
        self.write_count = 0;
        let _ = truncate_recent_input_context_cache(&self.path, self.max_context_lines);
    }
}

fn load_max_context_lines() -> u8 {
    let Some(path) = user_paths::default_config_path() else {
        return 0;
    };
    let config = if path.is_file() {
        VinpstConfig::from_json_file(path).ok()
    } else {
        VinpstConfig::bundled_default().ok()
    };
    config
        .filter(|config| config.validate().is_ok())
        .and_then(|config| {
            config
                .scenes
                .definitions
                .into_iter()
                .map(|scene| scene.context_lines)
                .max()
        })
        .unwrap_or(0)
}

/// Opaque recent-input history state owned by Rust.
pub struct VinpstFcitxContextHistory {
    history: ContextHistory,
}

/// Allocates the current-user recent-input context history state.
#[unsafe(no_mangle)]
pub extern "C" fn vinpst_fcitx_context_history_new() -> *mut VinpstFcitxContextHistory {
    Box::into_raw(Box::new(VinpstFcitxContextHistory {
        history: ContextHistory::current(),
    }))
}

/// Frees one recent-input context history state.
///
/// # Safety
///
/// `history` must be null or returned by `vinpst_fcitx_context_history_new` and not freed before.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_free(
    history: *mut VinpstFcitxContextHistory,
) {
    if !history.is_null() {
        // SAFETY: Forwarded from the caller contract.
        drop(unsafe { Box::from_raw(history) });
    }
}

/// Re-reads scene retention settings from the canonical user config.
///
/// # Safety
///
/// `history` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_reload(
    history: *mut VinpstFcitxContextHistory,
) {
    crate::ffi_catch((), || {
        // SAFETY: Forwarded from the caller contract.
        if let Some(history) = unsafe { history.as_mut() } {
            history.history.reload();
        }
    });
}

/// Records a user commit and returns whether the caller should arm the inactivity timer.
///
/// # Safety
///
/// `history` must be live and `text_data` must reference `text_len` readable bytes.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_user_commit(
    history: *mut VinpstFcitxContextHistory,
    context: usize,
    text_data: *const u8,
    text_len: usize,
) -> u8 {
    crate::ffi_catch(0, || {
        // SAFETY: Forwarded from the caller contract.
        let Some(history) = (unsafe { history.as_mut() }) else {
            return 0;
        };
        // SAFETY: Forwarded from the caller contract.
        let Some(text) = (unsafe { text_input(text_data, text_len) }) else {
            return 0;
        };
        u8::from(history.history.on_user_commit(context, text))
    })
}

/// Appends an explicit ASR/LLM context entry after flushing buffered user input.
///
/// # Safety
///
/// `history` must be live and both byte ranges valid UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_append_entry(
    history: *mut VinpstFcitxContextHistory,
    text_data: *const u8,
    text_len: usize,
    source_data: *const u8,
    source_len: usize,
) {
    crate::ffi_catch((), || {
        // SAFETY: Forwarded from the caller contract.
        let Some(history) = (unsafe { history.as_mut() }) else {
            return;
        };
        // SAFETY: Forwarded from the caller contract.
        let Some(text) = (unsafe { text_input(text_data, text_len) }) else {
            return;
        };
        // SAFETY: Forwarded from the caller contract.
        let Some(source) = (unsafe { text_input(source_data, source_len) }) else {
            return;
        };
        history.history.append_entry(text, source);
    });
}

/// Suppresses the next identical Fcitx commit event from being recorded as user text.
///
/// # Safety
///
/// `history` must be live and the byte range valid UTF-8.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_suppress_next(
    history: *mut VinpstFcitxContextHistory,
    text_data: *const u8,
    text_len: usize,
) {
    crate::ffi_catch((), || {
        // SAFETY: Forwarded from the caller contract.
        let Some(history) = (unsafe { history.as_mut() }) else {
            return;
        };
        // SAFETY: Forwarded from the caller contract.
        let Some(text) = (unsafe { text_input(text_data, text_len) }) else {
            return;
        };
        history.history.suppress_next(text);
    });
}

/// Flushes buffered user input when its owning Fcitx input context is destroyed.
///
/// # Safety
///
/// `history` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_context_destroyed(
    history: *mut VinpstFcitxContextHistory,
    context: usize,
) {
    crate::ffi_catch((), || {
        // SAFETY: Forwarded from the caller contract.
        if let Some(history) = unsafe { history.as_mut() } {
            history.history.context_destroyed(context);
        }
    });
}

/// Flushes any buffered user input immediately.
///
/// # Safety
///
/// `history` must be live.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn vinpst_fcitx_context_history_flush(
    history: *mut VinpstFcitxContextHistory,
) {
    crate::ffi_catch((), || {
        // SAFETY: Forwarded from the caller contract.
        if let Some(history) = unsafe { history.as_mut() } {
            history.history.flush();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::ContextHistory;

    #[test]
    fn suppression_and_context_switch_match_legacy_buffering() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut history = ContextHistory {
            path: dir.path().join("context.jsonl"),
            max_context_lines: 32,
            buffered_text: String::new(),
            buffered_context: None,
            suppress_next_commit: None,
            write_count: 0,
        };
        assert!(history.on_user_commit(1, "hello"));
        assert!(history.on_user_commit(1, "world"));
        assert!(history.on_user_commit(2, "你好"));
        history.suppress_next("voice");
        assert!(!history.on_user_commit(2, "voice"));
        history.append_entry("voice", "asr");
        history.flush();

        let lines = std::fs::read_to_string(&history.path).expect("context cache");
        assert!(lines.contains("\"text\":\"hello world\""));
        assert!(lines.contains("\"text\":\"你好\""));
        assert!(lines.contains("\"text\":\"voice\""));
        assert!(lines.contains("\"source\":\"asr\""));
    }

    #[test]
    fn mismatched_commit_consumes_one_shot_suppression() {
        let dir = tempfile::tempdir().expect("tempdir");
        let mut history = ContextHistory {
            path: dir.path().join("context.jsonl"),
            max_context_lines: 32,
            buffered_text: String::new(),
            buffered_context: None,
            suppress_next_commit: None,
            write_count: 0,
        };
        history.suppress_next("generated");
        assert!(history.on_user_commit(1, "other"));
        assert!(history.on_user_commit(1, "generated"));
        history.flush();

        let lines = std::fs::read_to_string(&history.path).expect("context cache");
        assert!(lines.contains("\"text\":\"other generated\""));
    }
}
