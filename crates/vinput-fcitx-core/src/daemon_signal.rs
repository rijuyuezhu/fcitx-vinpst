//! Pure presentation and control decisions for daemon-originated events.

use vinput_protocol::dbus;

/// Semantic notification severity chosen by the frontend core.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonNotificationKind {
    /// Informational daemon message.
    Info,
    /// Structured daemon error.
    Error,
}

/// Borrowed notification presentation decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaemonNotificationPlan<'a> {
    /// Semantic severity.
    pub kind: DaemonNotificationKind,
    /// Preferred daemon-provided text, or `None` for the localized unknown fallback.
    pub text: Option<&'a str>,
}

/// Semantic status-preedit decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonStatusPreedit<'a> {
    /// Clear daemon status preedit.
    Clear,
    /// Display a live partial exactly as received.
    Partial(&'a str),
    /// Display normal recording status.
    Recording,
    /// Display command recording status.
    Commanding,
    /// Display recognition/inference status.
    Recognizing,
    /// Display postprocessing status.
    Postprocessing,
}

/// Daemon-side event whose frontend control effect is decided by Rust.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonControlEvent<'a> {
    /// The daemon acquired or lost its well-known bus name.
    AvailabilityChanged {
        /// Whether the daemon is currently available.
        available: bool,
    },
    /// The daemon emitted a new status string.
    StatusChanged {
        /// Stable daemon status value.
        status: &'a str,
    },
    /// A trigger start found a daemon session already in progress.
    ReconcileBeforeStart {
        /// Stable daemon status value.
        status: &'a str,
        /// Whether the requested trigger is command mode.
        requested_command_mode: bool,
    },
}

/// Minimal frontend state required to decide daemon event control effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DaemonControlContext {
    /// Whether the local frontend controller owns a recording.
    pub recording: bool,
    /// Whether a remote-status preedit is currently attached to an input context.
    pub remote_status_active: bool,
}

/// Control effect requested by a daemon-originated event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonControlPlan {
    /// Ignore the event.
    None,
    /// Tear down local/remote session state because the daemon disappeared.
    ResetUnavailable,
    /// Clear a remote-status preedit and live signal state.
    ClearRemoteStatus,
    /// Reset the locally owned recording after idle/error status.
    ResetLocalRecording,
    /// Recompute the locally owned recording preedit.
    UpdateLocalPreedit,
    /// Present or refresh remote daemon status.
    PresentRemoteStatus,
    /// Adopt an externally started normal recording and stop it.
    AdoptAndStopNormal,
    /// Clear stale live state after an explicit daemon error status.
    ClearDaemonError,
}

/// Decides the control effect of one daemon-originated event.
#[must_use]
pub fn plan_daemon_control(
    event: DaemonControlEvent<'_>,
    context: DaemonControlContext,
) -> DaemonControlPlan {
    match event {
        DaemonControlEvent::AvailabilityChanged { available } => {
            if available || (!context.recording && !context.remote_status_active) {
                DaemonControlPlan::None
            } else {
                DaemonControlPlan::ResetUnavailable
            }
        }
        DaemonControlEvent::StatusChanged { status } => {
            if status.is_empty() {
                return DaemonControlPlan::None;
            }
            if !context.recording {
                if !context.remote_status_active {
                    DaemonControlPlan::None
                } else if status == dbus::status::IDLE || status == dbus::status::ERROR {
                    DaemonControlPlan::ClearRemoteStatus
                } else {
                    DaemonControlPlan::PresentRemoteStatus
                }
            } else if status == dbus::status::IDLE || status == dbus::status::ERROR {
                DaemonControlPlan::ResetLocalRecording
            } else {
                DaemonControlPlan::UpdateLocalPreedit
            }
        }
        DaemonControlEvent::ReconcileBeforeStart {
            status,
            requested_command_mode,
        } => {
            if context.recording || status.is_empty() || status == dbus::status::IDLE {
                DaemonControlPlan::None
            } else if status == dbus::status::RECORDING && !requested_command_mode {
                DaemonControlPlan::AdoptAndStopNormal
            } else if status == dbus::status::RECORDING
                || status == dbus::status::INFERRING
                || status == dbus::status::POSTPROCESSING
            {
                DaemonControlPlan::PresentRemoteStatus
            } else if status == dbus::status::ERROR {
                DaemonControlPlan::ClearDaemonError
            } else {
                DaemonControlPlan::None
            }
        }
    }
}

/// Chooses notification severity and preferred text using the legacy frontend policy.
#[must_use]
pub fn plan_daemon_notification<'a>(
    code: &'a str,
    subject: &'a str,
    detail: &'a str,
    raw_message: &'a str,
) -> DaemonNotificationPlan<'a> {
    let kind =
        if (!code.is_empty() && code != "unknown") || !subject.is_empty() || !detail.is_empty() {
            DaemonNotificationKind::Error
        } else {
            DaemonNotificationKind::Info
        };
    let text = if !raw_message.is_empty() {
        Some(raw_message)
    } else if !detail.is_empty() {
        Some(detail)
    } else if !subject.is_empty() {
        Some(subject)
    } else if !code.is_empty() && code != "unknown" {
        Some(code)
    } else {
        None
    };
    DaemonNotificationPlan { kind, text }
}

/// Chooses the semantic preedit for a daemon status/partial update.
#[must_use]
pub fn plan_daemon_status_preedit<'a>(
    status: &str,
    command_mode: bool,
    partial_text: &'a str,
) -> DaemonStatusPreedit<'a> {
    if !partial_text.is_empty() {
        return DaemonStatusPreedit::Partial(partial_text);
    }
    match status {
        "recording" if command_mode => DaemonStatusPreedit::Commanding,
        "recording" => DaemonStatusPreedit::Recording,
        "inferring" => DaemonStatusPreedit::Recognizing,
        "postprocessing" => DaemonStatusPreedit::Postprocessing,
        _ => DaemonStatusPreedit::Clear,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DaemonControlContext, DaemonControlEvent, DaemonControlPlan, DaemonNotificationKind,
        DaemonStatusPreedit, plan_daemon_control, plan_daemon_notification,
        plan_daemon_status_preedit,
    };

    #[test]
    fn notification_policy_preserves_legacy_priority_and_severity() {
        let plan = plan_daemon_notification("code", "subject", "detail", "raw");
        assert_eq!(plan.kind, DaemonNotificationKind::Error);
        assert_eq!(plan.text, Some("raw"));

        let plan = plan_daemon_notification("unknown", "", "", "message");
        assert_eq!(plan.kind, DaemonNotificationKind::Info);
        assert_eq!(plan.text, Some("message"));

        let plan = plan_daemon_notification("unknown", "", "", "");
        assert_eq!(plan.kind, DaemonNotificationKind::Info);
        assert_eq!(plan.text, None);
    }

    #[test]
    fn status_policy_prioritizes_live_partial() {
        assert_eq!(
            plan_daemon_status_preedit("recording", true, "partial"),
            DaemonStatusPreedit::Partial("partial")
        );
        assert_eq!(
            plan_daemon_status_preedit("recording", false, ""),
            DaemonStatusPreedit::Recording
        );
        assert_eq!(
            plan_daemon_status_preedit("recording", true, ""),
            DaemonStatusPreedit::Commanding
        );
        assert_eq!(
            plan_daemon_status_preedit("inferring", false, ""),
            DaemonStatusPreedit::Recognizing
        );
        assert_eq!(
            plan_daemon_status_preedit("postprocessing", false, ""),
            DaemonStatusPreedit::Postprocessing
        );
        assert_eq!(
            plan_daemon_status_preedit("idle", false, ""),
            DaemonStatusPreedit::Clear
        );
    }
    #[test]
    fn plans_daemon_availability_and_status_control() {
        let idle = DaemonControlContext {
            recording: false,
            remote_status_active: false,
        };
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::AvailabilityChanged { available: false },
                idle,
            ),
            DaemonControlPlan::None
        );
        let remote = DaemonControlContext {
            recording: false,
            remote_status_active: true,
        };
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::AvailabilityChanged { available: false },
                remote,
            ),
            DaemonControlPlan::ResetUnavailable
        );
        assert_eq!(
            plan_daemon_control(DaemonControlEvent::StatusChanged { status: "idle" }, remote,),
            DaemonControlPlan::ClearRemoteStatus
        );
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::StatusChanged {
                    status: "inferring"
                },
                remote,
            ),
            DaemonControlPlan::PresentRemoteStatus
        );
        let local = DaemonControlContext {
            recording: true,
            remote_status_active: false,
        };
        assert_eq!(
            plan_daemon_control(DaemonControlEvent::StatusChanged { status: "error" }, local,),
            DaemonControlPlan::ResetLocalRecording
        );
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::StatusChanged {
                    status: "postprocessing"
                },
                local,
            ),
            DaemonControlPlan::UpdateLocalPreedit
        );
    }

    #[test]
    fn plans_cross_client_start_reconciliation() {
        let idle = DaemonControlContext {
            recording: false,
            remote_status_active: false,
        };
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::ReconcileBeforeStart {
                    status: "recording",
                    requested_command_mode: false,
                },
                idle,
            ),
            DaemonControlPlan::AdoptAndStopNormal
        );
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::ReconcileBeforeStart {
                    status: "recording",
                    requested_command_mode: true,
                },
                idle,
            ),
            DaemonControlPlan::PresentRemoteStatus
        );
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::ReconcileBeforeStart {
                    status: "error",
                    requested_command_mode: false,
                },
                idle,
            ),
            DaemonControlPlan::ClearDaemonError
        );
        assert_eq!(
            plan_daemon_control(
                DaemonControlEvent::ReconcileBeforeStart {
                    status: "idle",
                    requested_command_mode: false,
                },
                idle,
            ),
            DaemonControlPlan::None
        );
    }
}
