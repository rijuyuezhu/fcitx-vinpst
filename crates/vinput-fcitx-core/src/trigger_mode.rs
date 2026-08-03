//! Pure Tap/Hold/Both trigger state machine.

/// Debounce interval for repeated trigger presses, in nanoseconds.
pub const TRIGGER_DEBOUNCE_NS: i64 = 80_000_000;
/// Hold duration before release schedules a stop, in nanoseconds.
pub const TRIGGER_HOLD_THRESHOLD_NS: i64 = 300_000_000;

/// User-selected trigger behavior.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TriggerMode {
    /// Press starts or stops recording; release is consumed.
    Tap = 0,
    /// Recording starts after the hold timer and stops after release.
    Hold = 1,
    /// A short press behaves as tap while a held press stops on release.
    #[default]
    Both = 2,
}

/// Recording kind selected by a trigger key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TriggerKind {
    /// Normal dictation.
    Normal = 0,
    /// Command-mode dictation.
    Command = 1,
}

/// Effect requested by the trigger state machine.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TriggerAction {
    /// No work is pending.
    #[default]
    None = 0,
    /// Consume the key event without another effect.
    Consume = 1,
    /// Start normal dictation immediately.
    StartNormal = 2,
    /// Start command mode immediately.
    StartCommand = 3,
    /// Stop the active recording.
    StopActive = 4,
    /// Schedule the hold timer for normal dictation.
    ScheduleNormalStart = 5,
    /// Schedule the hold timer for command mode.
    ScheduleCommandStart = 6,
    /// Cancel a hold timer that has not fired.
    CancelPendingStart = 7,
    /// Schedule the release-tail stop timer.
    ScheduleStop = 8,
}

/// One external event accepted by the trigger controller.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerEvent {
    /// Change the configured trigger mode.
    SetMode(TriggerMode),
    /// Handle a trigger press.
    Press {
        /// Normal or command trigger.
        kind: TriggerKind,
        /// Monotonic press timestamp.
        now_ns: i64,
        /// Whether recording was already active.
        recording: bool,
    },
    /// Handle a trigger release.
    Release {
        /// Monotonic release timestamp.
        now_ns: i64,
        /// Whether this release matches the active trigger key.
        active_release: bool,
    },
    /// Fire the delayed hold-start timer.
    FirePendingStart,
    /// Fire the delayed release-tail stop timer.
    FirePendingStop,
    /// Reconcile a recording-start attempt.
    ConfirmStart {
        /// Whether recording actually started.
        recording_started: bool,
    },
    /// Clear trigger ownership after recording stops.
    RecordingStopped,
}

/// Stable summary of trigger controller state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TriggerStateView {
    /// Configured trigger mode.
    pub mode: TriggerMode,
    /// Whether a delayed hold start is pending.
    pub has_pending_start: bool,
    /// Whether a trigger owns the active recording.
    pub has_active_trigger: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TriggerPress {
    kind: TriggerKind,
    pressed_at_ns: i64,
    released: bool,
}

/// Mutable trigger mode state independent of Fcitx key objects and timers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerModeState {
    mode: TriggerMode,
    last_press_ns: Option<i64>,
    pending_start: Option<TriggerPress>,
    active_trigger: Option<TriggerPress>,
    stop_pending: bool,
}

impl TriggerModeState {
    /// Creates an idle trigger state machine.
    #[must_use]
    pub const fn new(mode: TriggerMode) -> Self {
        Self {
            mode,
            last_press_ns: None,
            pending_start: None,
            active_trigger: None,
            stop_pending: false,
        }
    }

    /// Returns the configured trigger mode.
    #[must_use]
    pub const fn mode(&self) -> TriggerMode {
        self.mode
    }

    /// Changes mode and cancels only a not-yet-fired start, matching the legacy
    /// frontend behavior for an already active recording.
    pub fn set_mode(&mut self, mode: TriggerMode) {
        self.mode = mode;
        self.pending_start = None;
        self.last_press_ns = None;
    }

    /// Handles a trigger press at a monotonic timestamp.
    pub fn on_press(&mut self, kind: TriggerKind, now_ns: i64, recording: bool) -> TriggerAction {
        if self
            .last_press_ns
            .is_some_and(|last| now_ns.saturating_sub(last) < TRIGGER_DEBOUNCE_NS)
        {
            return TriggerAction::Consume;
        }
        self.last_press_ns = Some(now_ns);
        self.stop_pending = false;

        if let Some(active) = self.active_trigger {
            return if active.released {
                TriggerAction::StopActive
            } else {
                TriggerAction::Consume
            };
        }
        if recording {
            return TriggerAction::StopActive;
        }
        if self.pending_start.is_some() {
            return TriggerAction::Consume;
        }

        let press = TriggerPress {
            kind,
            pressed_at_ns: now_ns,
            released: false,
        };
        if self.mode == TriggerMode::Hold {
            self.pending_start = Some(press);
            return schedule_start_action(kind);
        }
        self.active_trigger = Some(press);
        start_action(kind)
    }

    /// Handles a key release. Fcitx-specific key matching is supplied by the
    /// adapter as `active_release`.
    pub fn on_release(&mut self, now_ns: i64, active_release: bool) -> TriggerAction {
        if self.mode == TriggerMode::Hold && self.pending_start.is_some() {
            self.pending_start = None;
            return TriggerAction::CancelPendingStart;
        }
        let Some(active) = self.active_trigger.as_mut() else {
            return TriggerAction::Consume;
        };

        if self.mode == TriggerMode::Tap {
            active.released = true;
            return TriggerAction::Consume;
        }

        if active_release {
            active.released = true;
            if now_ns.saturating_sub(active.pressed_at_ns) >= TRIGGER_HOLD_THRESHOLD_NS {
                self.stop_pending = true;
                return TriggerAction::ScheduleStop;
            }
            return TriggerAction::Consume;
        }

        if self.mode == TriggerMode::Both {
            active.released = true;
        }
        TriggerAction::Consume
    }

    /// Fires the pending hold-start timer.
    pub fn fire_pending_start(&mut self) -> TriggerAction {
        let Some(pending) = self.pending_start.take() else {
            return TriggerAction::None;
        };
        self.active_trigger = Some(pending);
        start_action(pending.kind)
    }

    /// Fires the release-tail stop timer.
    pub fn fire_pending_stop(&mut self) -> TriggerAction {
        if !self.stop_pending || !self.active_trigger.is_some_and(|active| active.released) {
            return TriggerAction::None;
        }
        self.stop_pending = false;
        TriggerAction::StopActive
    }

    /// Reconciles the state after the frontend attempted to start recording.
    pub fn confirm_start(&mut self, recording_started: bool) {
        if !recording_started {
            self.pending_start = None;
            self.active_trigger = None;
        }
    }

    /// Clears trigger state after recording stops.
    pub fn recording_stopped(&mut self) {
        self.pending_start = None;
        self.active_trigger = None;
        self.stop_pending = false;
    }

    /// Returns whether a delayed hold start is pending.
    #[must_use]
    pub const fn has_pending_start(&self) -> bool {
        self.pending_start.is_some()
    }

    /// Returns whether a trigger owns the active recording.
    #[must_use]
    pub const fn has_active_trigger(&self) -> bool {
        self.active_trigger.is_some()
    }

    /// Applies one external event and returns the requested effect.
    pub fn dispatch(&mut self, event: TriggerEvent) -> TriggerAction {
        match event {
            TriggerEvent::SetMode(mode) => {
                self.set_mode(mode);
                TriggerAction::None
            }
            TriggerEvent::Press {
                kind,
                now_ns,
                recording,
            } => self.on_press(kind, now_ns, recording),
            TriggerEvent::Release {
                now_ns,
                active_release,
            } => self.on_release(now_ns, active_release),
            TriggerEvent::FirePendingStart => self.fire_pending_start(),
            TriggerEvent::FirePendingStop => self.fire_pending_stop(),
            TriggerEvent::ConfirmStart { recording_started } => {
                self.confirm_start(recording_started);
                TriggerAction::None
            }
            TriggerEvent::RecordingStopped => {
                self.recording_stopped();
                TriggerAction::None
            }
        }
    }

    /// Returns a compact stable state summary.
    #[must_use]
    pub const fn view(&self) -> TriggerStateView {
        TriggerStateView {
            mode: self.mode(),
            has_pending_start: self.has_pending_start(),
            has_active_trigger: self.has_active_trigger(),
        }
    }
}

impl Default for TriggerModeState {
    fn default() -> Self {
        Self::new(TriggerMode::default())
    }
}

const fn start_action(kind: TriggerKind) -> TriggerAction {
    match kind {
        TriggerKind::Normal => TriggerAction::StartNormal,
        TriggerKind::Command => TriggerAction::StartCommand,
    }
}

const fn schedule_start_action(kind: TriggerKind) -> TriggerAction {
    match kind {
        TriggerKind::Normal => TriggerAction::ScheduleNormalStart,
        TriggerKind::Command => TriggerAction::ScheduleCommandStart,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        TRIGGER_HOLD_THRESHOLD_NS, TriggerAction, TriggerEvent, TriggerKind, TriggerMode,
        TriggerModeState, TriggerStateView,
    };

    const MS: i64 = 1_000_000;

    #[test]
    fn hold_mode_cancels_short_press_and_starts_after_timer() {
        let mut state = TriggerModeState::new(TriggerMode::Hold);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 0, false),
            TriggerAction::ScheduleNormalStart
        );
        assert!(state.has_pending_start());
        assert_eq!(
            state.on_release(100 * MS, true),
            TriggerAction::CancelPendingStart
        );
        assert_eq!(state.fire_pending_start(), TriggerAction::None);

        assert_eq!(
            state.on_press(TriggerKind::Command, 1_000 * MS, false),
            TriggerAction::ScheduleCommandStart
        );
        assert_eq!(state.fire_pending_start(), TriggerAction::StartCommand);
        state.confirm_start(true);
        assert_eq!(
            state.on_release(1_400 * MS, true),
            TriggerAction::ScheduleStop
        );
        assert_eq!(state.fire_pending_stop(), TriggerAction::StopActive);
    }

    #[test]
    fn tap_mode_stops_on_next_press() {
        let mut state = TriggerModeState::new(TriggerMode::Tap);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 0, false),
            TriggerAction::StartNormal
        );
        state.confirm_start(true);
        assert_eq!(state.on_release(50 * MS, true), TriggerAction::Consume);
        assert_eq!(
            state.on_press(TriggerKind::Command, 200 * MS, true),
            TriggerAction::StopActive
        );
    }

    #[test]
    fn both_mode_distinguishes_short_and_held_release() {
        let mut state = TriggerModeState::new(TriggerMode::Both);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 0, false),
            TriggerAction::StartNormal
        );
        state.confirm_start(true);
        assert_eq!(state.on_release(100 * MS, true), TriggerAction::Consume);
        assert_eq!(state.fire_pending_stop(), TriggerAction::None);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 200 * MS, true),
            TriggerAction::StopActive
        );

        state.recording_stopped();
        assert_eq!(
            state.on_press(TriggerKind::Normal, 1_000 * MS, false),
            TriggerAction::StartNormal
        );
        state.confirm_start(true);
        assert_eq!(
            state.on_release(1_000 * MS + TRIGGER_HOLD_THRESHOLD_NS, true),
            TriggerAction::ScheduleStop
        );
        assert_eq!(state.fire_pending_stop(), TriggerAction::StopActive);
    }

    #[test]
    fn debounces_and_clears_failed_start() {
        let mut state = TriggerModeState::new(TriggerMode::Tap);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 0, false),
            TriggerAction::StartNormal
        );
        state.confirm_start(false);
        assert!(!state.has_active_trigger());
        assert_eq!(
            state.on_press(TriggerKind::Normal, 50 * MS, false),
            TriggerAction::Consume
        );
        assert_eq!(
            state.on_press(TriggerKind::Normal, 100 * MS, false),
            TriggerAction::StartNormal
        );
    }

    #[test]
    fn unrelated_release_matches_legacy_both_behavior() {
        let mut state = TriggerModeState::new(TriggerMode::Both);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 0, false),
            TriggerAction::StartNormal
        );
        state.confirm_start(true);
        assert_eq!(state.on_release(100 * MS, false), TriggerAction::Consume);
        assert_eq!(
            state.on_press(TriggerKind::Normal, 200 * MS, true),
            TriggerAction::StopActive
        );
    }

    #[test]
    fn dispatches_external_events_and_exposes_compact_view() {
        let mut state = TriggerModeState::new(TriggerMode::Hold);
        assert_eq!(
            state.dispatch(TriggerEvent::Press {
                kind: TriggerKind::Command,
                now_ns: 0,
                recording: false,
            }),
            TriggerAction::ScheduleCommandStart
        );
        assert_eq!(
            state.view(),
            TriggerStateView {
                mode: TriggerMode::Hold,
                has_pending_start: true,
                has_active_trigger: false,
            }
        );
        assert_eq!(
            state.dispatch(TriggerEvent::FirePendingStart),
            TriggerAction::StartCommand
        );
        assert_eq!(
            state.dispatch(TriggerEvent::ConfirmStart {
                recording_started: true,
            }),
            TriggerAction::None
        );
        assert!(state.view().has_active_trigger);
        assert_eq!(
            state.dispatch(TriggerEvent::SetMode(TriggerMode::Both)),
            TriggerAction::None
        );
        assert_eq!(state.view().mode, TriggerMode::Both);
        assert_eq!(
            state.dispatch(TriggerEvent::RecordingStopped),
            TriggerAction::None
        );
        assert!(!state.view().has_active_trigger);
    }
}
