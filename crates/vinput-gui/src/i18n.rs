//! Typed English and Simplified Chinese presentation strings for the Rust GUI.

use std::env;

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
            Self::EnUs => english(key),
            Self::ZhCn => simplified_chinese(key),
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

/// Typed static presentation keys used by the first localized GUI surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum GuiText {
    ApplicationTitle,
    Control,
    Resources,
    Llm,
    Hotwords,
    OpenConfig,
    DaemonService,
    Recording,
    ReloadConfig,
    StartRecording,
    StopRecording,
    SavingConfiguration,
    StartingRecording,
    StoppingRecording,
    RecordingStarted,
    RecordingStopped,
    DaemonLoading,
    OwnerMonitorConnecting,
    OwnerMonitorReady,
    RefreshDaemon,
    StartDaemon,
    StopDaemon,
    RestartDaemon,
    StartingDaemon,
    StoppingDaemon,
    RestartingDaemon,
    General,
    AudioAndVad,
    DefaultLanguage,
    DefaultLanguagePlaceholder,
    CaptureDevice,
    PipeWireTarget,
    ActiveAsrProvider,
    ActiveScene,
    LockedWhileFinishing,
    DuckOutput,
    EnableVad,
    SaveConfiguration,
    ResetChanges,
    UnsavedChanges,
    ConfigurationUpToDate,
    ConfigDraftUnavailable,
    SourceUserFile,
    SourceBundledDefault,
    Details,
    Ok,
    NoValidConfig,
    NoHotwordProvider,
    NoHotwordProviderSelected,
    SaveOrResetHotwordBeforeSelecting,
    SaveOrResetHotwordBeforeProvider,
    SelectedHotwordProviderUnavailable,
    SaveHotwordBeforePathChange,
    HotwordPathCannotBeEmpty,
    SaveHotwordBeforePathClear,
    SaveHotwordBeforeReload,
    SetOrResetPathBeforeLoad,
    SetPathBeforeLoad,
    LoadingHotwordContent,
    DiscardedStaleHotwordContent,
    LoadedHotwordContent,
    MissingHotwordFileEmptyEditor,
    SetOrResetPathBeforeSave,
    LoadHotwordBeforeSave,
    SetPathBeforeSave,
    SavingHotwordContent,
    NoPendingHotwordActivation,
    SelectPendingHotwordProvider,
    RetryingHotwordActivation,
    SettingHotwordPath,
    ClearingHotwordPath,
    NoProviderSelected,
    AsrProvider,
    HotwordFile,
    HotwordPathPlaceholder,
    Browse,
    SetPath,
    ClearPath,
    LoadContent,
    SaveContent,
    RetryActivation,
    OneHotwordPerLine,
    HotwordActivationRetryable,
    UnsavedHotwordContent,
    HotwordContentUnchanged,
    LoadConfiguredHotwordFile,
    SelectHotwordsFile,
    TextFiles,
    AllFiles,
    SelectingHotwordFile,
    SelectedHotwordFile,
    InvalidUtf8HotwordPath,
    OpeningConfig,
    OpeningNotificationDetails,
    NoValidConfigLoaded,
    ConfigOpenLaunchFailed,
    ConfigOpenReaperFailed,
    DetailsOpenLaunchFailed,
    DetailsOpenReaperFailed,
    ConfigOpened,
    ConfigOpenedOnHost,
    DetailsOpened,
    DetailsOpenedOnHost,
}
#[cfg(test)]
impl GuiText {
    const ALL: [Self; 103] = [
        Self::ApplicationTitle,
        Self::Control,
        Self::Resources,
        Self::Llm,
        Self::Hotwords,
        Self::OpenConfig,
        Self::DaemonService,
        Self::Recording,
        Self::ReloadConfig,
        Self::StartRecording,
        Self::StopRecording,
        Self::SavingConfiguration,
        Self::StartingRecording,
        Self::StoppingRecording,
        Self::RecordingStarted,
        Self::RecordingStopped,
        Self::DaemonLoading,
        Self::OwnerMonitorConnecting,
        Self::OwnerMonitorReady,
        Self::RefreshDaemon,
        Self::StartDaemon,
        Self::StopDaemon,
        Self::RestartDaemon,
        Self::StartingDaemon,
        Self::StoppingDaemon,
        Self::RestartingDaemon,
        Self::General,
        Self::AudioAndVad,
        Self::DefaultLanguage,
        Self::DefaultLanguagePlaceholder,
        Self::CaptureDevice,
        Self::PipeWireTarget,
        Self::ActiveAsrProvider,
        Self::ActiveScene,
        Self::LockedWhileFinishing,
        Self::DuckOutput,
        Self::EnableVad,
        Self::SaveConfiguration,
        Self::ResetChanges,
        Self::UnsavedChanges,
        Self::ConfigurationUpToDate,
        Self::ConfigDraftUnavailable,
        Self::SourceUserFile,
        Self::SourceBundledDefault,
        Self::Details,
        Self::Ok,
        Self::NoValidConfig,
        Self::NoHotwordProvider,
        Self::NoHotwordProviderSelected,
        Self::SaveOrResetHotwordBeforeSelecting,
        Self::SaveOrResetHotwordBeforeProvider,
        Self::SelectedHotwordProviderUnavailable,
        Self::SaveHotwordBeforePathChange,
        Self::HotwordPathCannotBeEmpty,
        Self::SaveHotwordBeforePathClear,
        Self::SaveHotwordBeforeReload,
        Self::SetOrResetPathBeforeLoad,
        Self::SetPathBeforeLoad,
        Self::LoadingHotwordContent,
        Self::DiscardedStaleHotwordContent,
        Self::LoadedHotwordContent,
        Self::MissingHotwordFileEmptyEditor,
        Self::SetOrResetPathBeforeSave,
        Self::LoadHotwordBeforeSave,
        Self::SetPathBeforeSave,
        Self::SavingHotwordContent,
        Self::NoPendingHotwordActivation,
        Self::SelectPendingHotwordProvider,
        Self::RetryingHotwordActivation,
        Self::SettingHotwordPath,
        Self::ClearingHotwordPath,
        Self::NoProviderSelected,
        Self::AsrProvider,
        Self::HotwordFile,
        Self::HotwordPathPlaceholder,
        Self::Browse,
        Self::SetPath,
        Self::ClearPath,
        Self::LoadContent,
        Self::SaveContent,
        Self::RetryActivation,
        Self::OneHotwordPerLine,
        Self::HotwordActivationRetryable,
        Self::UnsavedHotwordContent,
        Self::HotwordContentUnchanged,
        Self::LoadConfiguredHotwordFile,
        Self::SelectHotwordsFile,
        Self::TextFiles,
        Self::AllFiles,
        Self::SelectingHotwordFile,
        Self::SelectedHotwordFile,
        Self::InvalidUtf8HotwordPath,
        Self::OpeningConfig,
        Self::OpeningNotificationDetails,
        Self::NoValidConfigLoaded,
        Self::ConfigOpenLaunchFailed,
        Self::ConfigOpenReaperFailed,
        Self::DetailsOpenLaunchFailed,
        Self::DetailsOpenReaperFailed,
        Self::ConfigOpened,
        Self::ConfigOpenedOnHost,
        Self::DetailsOpened,
        Self::DetailsOpenedOnHost,
    ];
}

const fn english(key: GuiText) -> &'static str {
    if (key as u8) <= GuiText::SourceBundledDefault as u8 {
        english_core(key)
    } else if (key as u8) <= GuiText::InvalidUtf8HotwordPath as u8 {
        english_hotwords(key)
    } else {
        english_desktop(key)
    }
}

const fn english_core(key: GuiText) -> &'static str {
    match key {
        GuiText::ApplicationTitle => "Vinput Configuration",
        GuiText::Control => "Control",
        GuiText::Resources => "Resources",
        GuiText::Llm => "LLM",
        GuiText::Hotwords => "Hotwords",
        GuiText::OpenConfig => "Open config",
        GuiText::DaemonService => "Daemon service",
        GuiText::Recording => "Recording",
        GuiText::ReloadConfig => "Reload config",
        GuiText::StartRecording => "Start recording",
        GuiText::StopRecording => "Stop recording",
        GuiText::SavingConfiguration => "Saving configuration…",
        GuiText::StartingRecording => "Starting recording…",
        GuiText::StoppingRecording => "Stopping recording…",
        GuiText::RecordingStarted => "Recording started.",
        GuiText::RecordingStopped => {
            "Recording stopped; the recognition result was delivered to the frontend."
        }
        GuiText::DaemonLoading => "Daemon: loading…",
        GuiText::OwnerMonitorConnecting => "Owner monitoring: connecting to D-Bus signals…",
        GuiText::OwnerMonitorReady => "Owner monitoring: signal-driven reconciliation active.",
        GuiText::RefreshDaemon => "Refresh daemon",
        GuiText::StartDaemon => "Start daemon",
        GuiText::StopDaemon => "Stop daemon",
        GuiText::RestartDaemon => "Restart daemon",
        GuiText::StartingDaemon => "Starting daemon…",
        GuiText::StoppingDaemon => "Stopping daemon…",
        GuiText::RestartingDaemon => "Restarting daemon…",
        GuiText::General => "General",
        GuiText::AudioAndVad => "Audio and VAD",
        GuiText::DefaultLanguage => "Default language",
        GuiText::DefaultLanguagePlaceholder => "for example en-US or zh-CN",
        GuiText::CaptureDevice => "Capture device",
        GuiText::PipeWireTarget => "PipeWire target",
        GuiText::ActiveAsrProvider => "Active ASR provider",
        GuiText::ActiveScene => "Active scene",
        GuiText::LockedWhileFinishing => "Locked while operation finishes",
        GuiText::DuckOutput => "Duck output while recording",
        GuiText::EnableVad => "Enable voice activity detection",
        GuiText::SaveConfiguration => "Save configuration",
        GuiText::ResetChanges => "Reset changes",
        GuiText::UnsavedChanges => "Unsaved changes",
        GuiText::ConfigurationUpToDate => "Configuration is up to date",
        GuiText::ConfigDraftUnavailable => "Config draft is unavailable.",
        GuiText::SourceUserFile => "user file",
        GuiText::SourceBundledDefault => "bundled default; Save creates the user file",
        _ => unreachable!(),
    }
}

const fn english_hotwords(key: GuiText) -> &'static str {
    match key {
        GuiText::Details => "Details",
        GuiText::Ok => "OK",
        GuiText::NoValidConfig => "No valid configuration is loaded.",
        GuiText::NoHotwordProvider => "No local or command ASR provider supports hotword files.",
        GuiText::NoHotwordProviderSelected => "No hotword-capable provider is selected.",
        GuiText::SaveOrResetHotwordBeforeSelecting => {
            "Save or reset the edited hotword content before selecting another file."
        }
        GuiText::SaveOrResetHotwordBeforeProvider => {
            "Save or reset hotword changes before selecting another provider."
        }
        GuiText::SelectedHotwordProviderUnavailable => {
            "The selected hotword provider is unavailable."
        }
        GuiText::SaveHotwordBeforePathChange => {
            "Save the edited hotword content before changing its configured path."
        }
        GuiText::HotwordPathCannotBeEmpty => "Hotword file path cannot be empty.",
        GuiText::SaveHotwordBeforePathClear => {
            "Save the edited hotword content before clearing its configured path."
        }
        GuiText::SaveHotwordBeforeReload => {
            "Save the edited hotword content before loading it again."
        }
        GuiText::SetOrResetPathBeforeLoad => {
            "Set or reset the hotword path before loading content."
        }
        GuiText::SetPathBeforeLoad => "Set a hotword file path before loading content.",
        GuiText::LoadingHotwordContent => "Loading hotword content…",
        GuiText::DiscardedStaleHotwordContent => {
            "Discarded stale hotword content loaded for a previous selection."
        }
        GuiText::LoadedHotwordContent => "Loaded configured hotword content.",
        GuiText::MissingHotwordFileEmptyEditor => {
            "Configured hotword file does not exist yet; loaded an empty editor."
        }
        GuiText::SetOrResetPathBeforeSave => "Set or reset the hotword path before saving content.",
        GuiText::LoadHotwordBeforeSave => "Load the configured hotword file before saving content.",
        GuiText::SetPathBeforeSave => "Set a hotword file path before saving content.",
        GuiText::SavingHotwordContent => "Saving hotword content…",
        GuiText::NoPendingHotwordActivation => "No saved hotword activation is pending retry.",
        GuiText::SelectPendingHotwordProvider => {
            "Select the provider with the pending hotword activation before retrying."
        }
        GuiText::RetryingHotwordActivation => "Retrying hotword activation…",
        GuiText::SettingHotwordPath => "Setting hotword path…",
        GuiText::ClearingHotwordPath => "Clearing hotword path…",
        GuiText::NoProviderSelected => "No provider selected",
        GuiText::AsrProvider => "ASR provider",
        GuiText::HotwordFile => "Hotword file",
        GuiText::HotwordPathPlaceholder => "Path to a UTF-8 hotword file",
        GuiText::Browse => "Browse…",
        GuiText::SetPath => "Set path",
        GuiText::ClearPath => "Clear path",
        GuiText::LoadContent => "Load content",
        GuiText::SaveContent => "Save content",
        GuiText::RetryActivation => "Retry activation",
        GuiText::OneHotwordPerLine => "One hotword entry per line",
        GuiText::HotwordActivationRetryable => {
            "Hotword configuration is saved; daemon activation can be retried"
        }
        GuiText::UnsavedHotwordContent => "Unsaved hotword content",
        GuiText::HotwordContentUnchanged => "Hotword content is unchanged",
        GuiText::LoadConfiguredHotwordFile => "Load the configured file to edit its contents",
        GuiText::SelectHotwordsFile => "Select Hotwords File",
        GuiText::TextFiles => "Text Files",
        GuiText::AllFiles => "All Files",
        GuiText::SelectingHotwordFile => "Selecting hotword file…",
        GuiText::SelectedHotwordFile => {
            "Selected a hotword file; use Set path to validate and apply it."
        }
        GuiText::InvalidUtf8HotwordPath => "The selected hotword path is not valid UTF-8.",
        _ => unreachable!(),
    }
}

const fn english_desktop(key: GuiText) -> &'static str {
    match key {
        GuiText::OpeningConfig => "Opening config file…",
        GuiText::OpeningNotificationDetails => "Opening notification details…",
        GuiText::NoValidConfigLoaded => "No valid config is loaded.",
        GuiText::ConfigOpenLaunchFailed => {
            "Cannot open the config file: the desktop opener could not be started."
        }
        GuiText::ConfigOpenReaperFailed => {
            "Cannot open the config file: the desktop opener could not be supervised safely."
        }
        GuiText::DetailsOpenLaunchFailed => {
            "Cannot open notification details: the desktop opener could not be started."
        }
        GuiText::DetailsOpenReaperFailed => {
            "Cannot open notification details: the desktop opener could not be supervised safely."
        }
        GuiText::ConfigOpened => "Passed the config file to the desktop opener.",
        GuiText::ConfigOpenedOnHost => "Passed the config file to the host desktop opener.",
        GuiText::DetailsOpened => "Passed notification details to the desktop opener.",
        GuiText::DetailsOpenedOnHost => "Passed notification details to the host desktop opener.",
        _ => unreachable!(),
    }
}

const fn simplified_chinese(key: GuiText) -> &'static str {
    if (key as u8) <= GuiText::SourceBundledDefault as u8 {
        simplified_chinese_core(key)
    } else if (key as u8) <= GuiText::InvalidUtf8HotwordPath as u8 {
        simplified_chinese_hotwords(key)
    } else {
        simplified_chinese_desktop(key)
    }
}

const fn simplified_chinese_core(key: GuiText) -> &'static str {
    match key {
        GuiText::ApplicationTitle => "Vinput 配置",
        GuiText::Control => "控制",
        GuiText::Resources => "资源",
        GuiText::Llm => "LLM",
        GuiText::Hotwords => "热词",
        GuiText::OpenConfig => "打开配置",
        GuiText::DaemonService => "守护进程",
        GuiText::Recording => "录音",
        GuiText::ReloadConfig => "重新加载配置",
        GuiText::StartRecording => "开始录音",
        GuiText::StopRecording => "停止录音",
        GuiText::SavingConfiguration => "正在保存配置…",
        GuiText::StartingRecording => "正在开始录音…",
        GuiText::StoppingRecording => "正在停止录音…",
        GuiText::RecordingStarted => "录音已开始。",
        GuiText::RecordingStopped => "录音已停止；识别结果已交付给前端。",
        GuiText::DaemonLoading => "守护进程：加载中…",
        GuiText::OwnerMonitorConnecting => "所有者监控：正在连接 D-Bus 信号…",
        GuiText::OwnerMonitorReady => "所有者监控：信号驱动的协调已启用。",
        GuiText::RefreshDaemon => "刷新守护进程",
        GuiText::StartDaemon => "启动守护进程",
        GuiText::StopDaemon => "停止守护进程",
        GuiText::RestartDaemon => "重启守护进程",
        GuiText::StartingDaemon => "正在启动守护进程…",
        GuiText::StoppingDaemon => "正在停止守护进程…",
        GuiText::RestartingDaemon => "正在重启守护进程…",
        GuiText::General => "常规",
        GuiText::AudioAndVad => "音频和 VAD",
        GuiText::DefaultLanguage => "默认语言",
        GuiText::DefaultLanguagePlaceholder => "例如 en-US 或 zh-CN",
        GuiText::CaptureDevice => "录音设备",
        GuiText::PipeWireTarget => "PipeWire 目标",
        GuiText::ActiveAsrProvider => "当前 ASR 提供商",
        GuiText::ActiveScene => "当前场景",
        GuiText::LockedWhileFinishing => "操作完成前已锁定",
        GuiText::DuckOutput => "录音时降低输出音量",
        GuiText::EnableVad => "启用语音活动检测",
        GuiText::SaveConfiguration => "保存配置",
        GuiText::ResetChanges => "重置更改",
        GuiText::UnsavedChanges => "有未保存的更改",
        GuiText::ConfigurationUpToDate => "配置已是最新",
        GuiText::ConfigDraftUnavailable => "配置草稿不可用。",
        GuiText::SourceUserFile => "用户文件",
        GuiText::SourceBundledDefault => "内置默认值；保存后将创建用户文件",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_hotwords(key: GuiText) -> &'static str {
    match key {
        GuiText::Details => "查看详情",
        GuiText::Ok => "确定",
        GuiText::NoValidConfig => "未加载有效配置。",
        GuiText::NoHotwordProvider => "没有支持热词文件的本地或命令 ASR 提供商。",
        GuiText::NoHotwordProviderSelected => "未选择支持热词的提供商。",
        GuiText::SaveOrResetHotwordBeforeSelecting => {
            "选择其他文件前，请先保存或重置已编辑的热词内容。"
        }
        GuiText::SaveOrResetHotwordBeforeProvider => "选择其他提供商前，请先保存或重置热词更改。",
        GuiText::SelectedHotwordProviderUnavailable => "所选热词提供商不可用。",
        GuiText::SaveHotwordBeforePathChange => "更改已配置路径前，请先保存已编辑的热词内容。",
        GuiText::HotwordPathCannotBeEmpty => "热词文件路径不能为空。",
        GuiText::SaveHotwordBeforePathClear => "清除已配置路径前，请先保存已编辑的热词内容。",
        GuiText::SaveHotwordBeforeReload => "重新加载前，请先保存已编辑的热词内容。",
        GuiText::SetOrResetPathBeforeLoad => "加载内容前，请先设置或重置热词路径。",
        GuiText::SetPathBeforeLoad => "加载内容前，请先设置热词文件路径。",
        GuiText::LoadingHotwordContent => "正在加载热词内容…",
        GuiText::DiscardedStaleHotwordContent => "已丢弃先前选择对应的过期热词内容。",
        GuiText::LoadedHotwordContent => "已加载配置的热词内容。",
        GuiText::MissingHotwordFileEmptyEditor => "配置的热词文件尚不存在；已加载空编辑器。",
        GuiText::SetOrResetPathBeforeSave => "保存内容前，请先设置或重置热词路径。",
        GuiText::LoadHotwordBeforeSave => "保存内容前，请先加载已配置的热词文件。",
        GuiText::SetPathBeforeSave => "保存内容前，请先设置热词文件路径。",
        GuiText::SavingHotwordContent => "正在保存热词内容…",
        GuiText::NoPendingHotwordActivation => "没有待重试的已保存热词激活。",
        GuiText::SelectPendingHotwordProvider => "重试前，请选择存在待处理热词激活的提供商。",
        GuiText::RetryingHotwordActivation => "正在重试热词激活…",
        GuiText::SettingHotwordPath => "正在设置热词路径…",
        GuiText::ClearingHotwordPath => "正在清除热词路径…",
        GuiText::NoProviderSelected => "未选择提供商",
        GuiText::AsrProvider => "ASR 提供商",
        GuiText::HotwordFile => "热词文件",
        GuiText::HotwordPathPlaceholder => "UTF-8 热词文件路径",
        GuiText::Browse => "浏览…",
        GuiText::SetPath => "设置路径",
        GuiText::ClearPath => "清除路径",
        GuiText::LoadContent => "加载内容",
        GuiText::SaveContent => "保存内容",
        GuiText::RetryActivation => "重试激活",
        GuiText::OneHotwordPerLine => "每行一个热词",
        GuiText::HotwordActivationRetryable => "热词配置已保存；可以重试守护进程激活",
        GuiText::UnsavedHotwordContent => "热词内容尚未保存",
        GuiText::HotwordContentUnchanged => "热词内容未更改",
        GuiText::LoadConfiguredHotwordFile => "加载已配置文件后即可编辑内容",
        GuiText::SelectHotwordsFile => "选择热词文件",
        GuiText::TextFiles => "文本文件",
        GuiText::AllFiles => "所有文件",
        GuiText::SelectingHotwordFile => "正在选择热词文件…",
        GuiText::SelectedHotwordFile => "已选择热词文件；请使用“设置路径”验证并应用。",
        GuiText::InvalidUtf8HotwordPath => "所选热词路径不是有效的 UTF-8。",
        _ => unreachable!(),
    }
}

const fn simplified_chinese_desktop(key: GuiText) -> &'static str {
    match key {
        GuiText::OpeningConfig => "正在打开配置文件…",
        GuiText::OpeningNotificationDetails => "正在打开通知详情…",
        GuiText::NoValidConfigLoaded => "未加载有效配置。",
        GuiText::ConfigOpenLaunchFailed => "无法打开配置文件：桌面打开程序无法启动。",
        GuiText::ConfigOpenReaperFailed => "无法打开配置文件：无法安全监管桌面打开程序。",
        GuiText::DetailsOpenLaunchFailed => "无法打开通知详情：桌面打开程序无法启动。",
        GuiText::DetailsOpenReaperFailed => "无法打开通知详情：无法安全监管桌面打开程序。",
        GuiText::ConfigOpened => "已将配置文件交给桌面打开程序。",
        GuiText::ConfigOpenedOnHost => "已将配置文件交给宿主桌面打开程序。",
        GuiText::DetailsOpened => "已将通知详情交给桌面打开程序。",
        GuiText::DetailsOpenedOnHost => "已将通知详情交给宿主桌面打开程序。",
        _ => unreachable!(),
    }
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
