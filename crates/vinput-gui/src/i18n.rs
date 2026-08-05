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

    pub(crate) fn scene_provider_choice(self, provider_id: Option<&str>) -> String {
        match provider_id {
            None => self.text(GuiText::NoProviderClearBinding).to_owned(),
            Some(provider_id) => match self {
                Self::EnUs => format!("Provider: {provider_id}"),
                Self::ZhCn => format!("提供商：{provider_id}"),
            },
        }
    }

    pub(crate) fn scene_id_immutable(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Scene id: {scene_id} (immutable)"),
            Self::ZhCn => format!("场景 ID：{scene_id}（不可修改）"),
        }
    }

    pub(crate) fn provider_identity(self, provider_id: &str, kind: &str) -> String {
        match self {
            Self::EnUs => {
                format!("Provider id: {provider_id} (immutable) · type: {kind} (immutable)")
            }
            Self::ZhCn => {
                format!("提供商 ID：{provider_id}（不可修改）· 类型：{kind}（不可修改）")
            }
        }
    }

    pub(crate) fn selected_label(self, label: &str) -> String {
        match self {
            Self::EnUs => format!("{label} (selected)"),
            Self::ZhCn => format!("{label}（已选择）"),
        }
    }

    pub(crate) fn scene_added(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Added scene `{scene_id}`."),
            Self::ZhCn => format!("已添加场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_updated(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Updated scene `{scene_id}`."),
            Self::ZhCn => format!("已更新场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_selected(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Selected scene `{scene_id}`."),
            Self::ZhCn => format!("已选择场景“{scene_id}”。"),
        }
    }

    pub(crate) fn scene_removed(self, scene_id: &str) -> String {
        match self {
            Self::EnUs => format!("Removed scene `{scene_id}`."),
            Self::ZhCn => format!("已移除场景“{scene_id}”。"),
        }
    }

    pub(crate) fn asr_provider_changed(self, created: bool, provider_id: &str) -> String {
        match (self, created) {
            (Self::EnUs, true) => format!("Added ASR provider `{provider_id}`."),
            (Self::EnUs, false) => format!("Updated ASR provider `{provider_id}`."),
            (Self::ZhCn, true) => format!("已添加 ASR 提供商“{provider_id}”。"),
            (Self::ZhCn, false) => format!("已更新 ASR 提供商“{provider_id}”。"),
        }
    }

    pub(crate) fn asr_provider_removed(self, provider_id: &str) -> String {
        match self {
            Self::EnUs => format!("Removed custom ASR provider `{provider_id}`."),
            Self::ZhCn => format!("已移除自定义 ASR 提供商“{provider_id}”。"),
        }
    }

    pub(crate) fn save_receipt(
        self,
        summary: &str,
        path: &str,
        backup: Option<&str>,
        daemon_reload: &str,
    ) -> String {
        match (self, backup) {
            (Self::EnUs, Some(backup)) => {
                format!("{summary} Saved {path} (backup {backup}); {daemon_reload}")
            }
            (Self::EnUs, None) => {
                format!("{summary} Saved {path} (no previous file); {daemon_reload}")
            }
            (Self::ZhCn, Some(backup)) => {
                format!("{summary} 已保存 {path}（备份 {backup}）；{daemon_reload}")
            }
            (Self::ZhCn, None) => {
                format!("{summary} 已保存 {path}（此前无文件）；{daemon_reload}")
            }
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
    fn localized_form_templates_preserve_machine_ids_and_raw_details() {
        let scene_id = "scene-machine-id";
        let provider_id = "provider-machine-id";
        let path = "/tmp/config-machine-path";
        let reload = "raw daemon reload detail";

        let english_scene = GuiLocale::EnUs.scene_added(scene_id);
        let chinese_scene = GuiLocale::ZhCn.scene_added(scene_id);
        assert!(english_scene.contains(scene_id));
        assert!(chinese_scene.contains(scene_id));
        assert_ne!(english_scene, chinese_scene);

        let english_provider = GuiLocale::EnUs.asr_provider_changed(true, provider_id);
        let chinese_provider = GuiLocale::ZhCn.asr_provider_changed(true, provider_id);
        assert!(english_provider.contains(provider_id));
        assert!(chinese_provider.contains(provider_id));
        assert_ne!(english_provider, chinese_provider);

        for locale in [GuiLocale::EnUs, GuiLocale::ZhCn] {
            let receipt = locale.save_receipt(&english_provider, path, None, reload);
            assert!(receipt.contains(provider_id));
            assert!(receipt.contains(path));
            assert!(receipt.contains(reload));
        }
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
