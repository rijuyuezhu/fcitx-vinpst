//! Typed English and Simplified Chinese presentation strings for the Rust GUI.

use std::env;

mod en;
mod keys;
mod zh_cn;

pub(crate) use keys::GuiText;

/// Locale set required for legacy management-GUI parity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuiLocale {
    /// English fallback.
    EnUs,
    /// Simplified Chinese.
    ZhCn,
}

impl GuiLocale {
    /// Detects the preferred GUI locale using the legacy environment priority.
    #[must_use]
    pub fn detect() -> Self {
        Self::from_candidates(
            ["LANGUAGE", "LC_ALL", "LC_MESSAGES", "LANG"]
                .into_iter()
                .filter_map(|name| env::var(name).ok()),
        )
    }

    /// Resolves one locale name without reading process-global state.
    #[must_use]
    pub fn from_name(value: &str) -> Self {
        Self::from_candidates([value])
    }

    /// Stable locale identifier used by diagnostics.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::EnUs => "en_US",
            Self::ZhCn => "zh_CN",
        }
    }

    /// Resolves one typed static presentation string.
    #[must_use]
    pub(crate) const fn text(self, key: GuiText) -> &'static str {
        match self {
            Self::EnUs => en::text(key),
            Self::ZhCn => zh_cn::text(key),
        }
    }

    fn from_candidates(values: impl IntoIterator<Item = impl AsRef<str>>) -> Self {
        values
            .into_iter()
            .flat_map(|value| {
                value
                    .as_ref()
                    .split(':')
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .find_map(|value| normalized_locale(&value))
            .unwrap_or(Self::EnUs)
    }

    pub(crate) fn config_path(self, path: impl std::fmt::Display) -> String {
        match self {
            Self::EnUs => format!("Config: {path}"),
            Self::ZhCn => format!("配置：{path}"),
        }
    }

    pub(crate) fn config_error(self, error: &str) -> String {
        match self {
            Self::EnUs => format!("Config error: {error}"),
            Self::ZhCn => format!("配置错误：{error}"),
        }
    }

    pub(crate) fn daemon_status(self, status: &str) -> String {
        match self {
            Self::EnUs => format!("Daemon: {status}"),
            Self::ZhCn => format!("守护进程：{status}"),
        }
    }

    pub(crate) fn daemon_unavailable(self, error: &str) -> String {
        match self {
            Self::EnUs => format!("Daemon unavailable: {error}"),
            Self::ZhCn => format!("守护进程不可用：{error}"),
        }
    }

    pub(crate) fn owner_monitor_degraded(self, error: &str) -> String {
        match self {
            Self::EnUs => format!(
                "Owner monitoring degraded; using a 30-second non-activating fallback: {error}"
            ),
            Self::ZhCn => {
                format!("所有者监控已降级；正在使用 30 秒一次且不会激活服务的回退查询：{error}")
            }
        }
    }

    pub(crate) fn duck_volume(self, percent: f32) -> String {
        match self {
            Self::EnUs => format!("Duck volume: {percent:.0}%"),
            Self::ZhCn => format!("录音时输出音量：{percent:.0}%"),
        }
    }

    pub(crate) fn vad_threshold(self, threshold: f32) -> String {
        match self {
            Self::EnUs => format!("VAD threshold: {threshold:.2}"),
            Self::ZhCn => format!("VAD 阈值：{threshold:.2}"),
        }
    }

    pub(crate) fn operation_success(self, message: &str) -> String {
        match self {
            Self::EnUs => format!("Success: {message}"),
            Self::ZhCn => format!("成功：{message}"),
        }
    }

    pub(crate) fn operation_error(self, message: &str) -> String {
        match self {
            Self::EnUs => format!("Error: {message}"),
            Self::ZhCn => format!("错误：{message}"),
        }
    }

    pub(crate) fn daemon_action_failure(self, action: DaemonActionName) -> String {
        match (self, action) {
            (Self::EnUs, DaemonActionName::Start) => {
                "Cannot start daemon: D-Bus activation did not return a valid daemon snapshot."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Start) => {
                "无法启动守护进程：D-Bus 激活未返回有效的守护进程状态。".to_owned()
            }
            (Self::EnUs, DaemonActionName::Stop) => {
                "Cannot stop daemon: the user-service command was rejected or could not be executed."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Stop) => {
                "无法停止守护进程：用户服务命令被拒绝或无法执行。".to_owned()
            }
            (Self::EnUs, DaemonActionName::Restart) => {
                "Cannot restart daemon: the user-service command was rejected or could not be executed."
                    .to_owned()
            }
            (Self::ZhCn, DaemonActionName::Restart) => {
                "无法重启守护进程：用户服务命令被拒绝或无法执行。".to_owned()
            }
        }
    }

    pub(crate) fn daemon_state_confirmed(self, running: bool) -> String {
        match (self, running) {
            (Self::EnUs, true) => "Daemon running state confirmed.".to_owned(),
            (Self::EnUs, false) => "Daemon stopped state confirmed.".to_owned(),
            (Self::ZhCn, true) => "已确认守护进程正在运行。".to_owned(),
            (Self::ZhCn, false) => "已确认守护进程已停止。".to_owned(),
        }
    }

    pub(crate) fn daemon_action_unconfirmed(
        self,
        action: DaemonActionName,
        unavailable: bool,
    ) -> String {
        let action = match (self, action) {
            (Self::EnUs, DaemonActionName::Start) => "start",
            (Self::EnUs, DaemonActionName::Stop) => "stop",
            (Self::EnUs, DaemonActionName::Restart) => "restart",
            (Self::ZhCn, DaemonActionName::Start) => "启动",
            (Self::ZhCn, DaemonActionName::Stop) => "停止",
            (Self::ZhCn, DaemonActionName::Restart) => "重启",
        };
        match (self, unavailable) {
            (Self::EnUs, false) => format!(
                "Daemon {action} request was accepted, but the observed owner state did not confirm it."
            ),
            (Self::EnUs, true) => format!(
                "Daemon {action} request was accepted; current owner state is unavailable and will be reconciled by D-Bus monitoring."
            ),
            (Self::ZhCn, false) => {
                format!("守护进程{action}请求已接受，但观察到的所有者状态未能确认结果。")
            }
            (Self::ZhCn, true) => format!(
                "守护进程{action}请求已接受；当前所有者状态不可用，将由 D-Bus 监控进行协调。"
            ),
        }
    }

    pub(crate) fn installed_model_scan_failed(self, error: &str) -> String {
        match self {
            Self::EnUs => format!("Installed model scan failed: {error}"),
            Self::ZhCn => format!("已安装模型扫描失败：{error}"),
        }
    }

    pub(crate) fn installed_model_row(
        self,
        title: &str,
        directory: &str,
        file_count: usize,
        active: bool,
    ) -> String {
        let state = self.text(if active {
            GuiText::Active
        } else {
            GuiText::Inactive
        });
        match self {
            Self::EnUs => format!("{title} · {directory} · {file_count} files · {state}"),
            Self::ZhCn => format!("{title} · {directory} · {file_count} 个文件 · {state}"),
        }
    }

    pub(crate) fn adapter_row(self, adapter_id: &str, runtime: &str) -> String {
        format!(
            "{adapter_id} · {} · {runtime}",
            self.text(GuiText::CommandAdapter)
        )
    }

    pub(crate) fn runtime_running_pid(self, pid: u32) -> String {
        format!("{} · pid {pid}", self.text(GuiText::Running))
    }

    pub(crate) fn model_detail_title(self, title: &str) -> String {
        format!("{} · {title}", self.text(GuiText::Model))
    }

    pub(crate) fn asr_provider_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::AsrProvider))
    }

    pub(crate) fn llm_provider_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::LlmProvider))
    }

    pub(crate) fn text_adapter_detail_title(self, id: &str) -> String {
        format!("{} · {id}", self.text(GuiText::TextAdapter))
    }

    pub(crate) fn configured_count(self, count: usize) -> String {
        match self {
            Self::EnUs => format!("{count} configured"),
            Self::ZhCn => format!("已配置 {count} 项"),
        }
    }
}

fn normalized_locale(value: &str) -> Option<GuiLocale> {
    let value = value
        .trim()
        .split(['.', '@'])
        .next()
        .unwrap_or_default()
        .replace('-', "_");
    if value.is_empty() || matches!(value.as_str(), "C" | "POSIX") {
        return None;
    }
    value
        .to_ascii_lowercase()
        .starts_with("zh")
        .then_some(GuiLocale::ZhCn)
        .or(Some(GuiLocale::EnUs))
}

/// Stable daemon action names used by localized result templates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DaemonActionName {
    Start,
    Stop,
    Restart,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_detection_normalizes_legacy_names_and_falls_back_to_english() {
        assert_eq!(GuiLocale::from_name("zh_CN.UTF-8"), GuiLocale::ZhCn);
        assert_eq!(GuiLocale::from_name("zh-Hans@variant"), GuiLocale::ZhCn);
        assert_eq!(GuiLocale::from_name("en_US.UTF-8"), GuiLocale::EnUs);
        assert_eq!(GuiLocale::from_name("C.UTF-8"), GuiLocale::EnUs);
    }

    #[test]
    fn translated_key_set_is_complete_and_nonempty() {
        for locale in [GuiLocale::EnUs, GuiLocale::ZhCn] {
            assert!(
                GuiText::ALL
                    .into_iter()
                    .all(|key| !locale.text(key).is_empty())
            );
        }
        assert!(
            GuiText::ALL
                .into_iter()
                .any(|key| GuiLocale::EnUs.text(key) != GuiLocale::ZhCn.text(key))
        );
    }
}
