//! English GUI presentation strings.

use super::GuiText;

pub(super) const fn text(key: GuiText) -> &'static str {
    if (key as u8) <= GuiText::SourceBundledDefault as u8 {
        english_core(key)
    } else if (key as u8) <= GuiText::InvalidUtf8HotwordPath as u8 {
        english_hotwords(key)
    } else if (key as u8) <= GuiText::DetailsOpenedOnHost as u8 {
        english_desktop(key)
    } else {
        english_resources(key)
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

const fn english_resources(key: GuiText) -> &'static str {
    match key {
        GuiText::FilterProvidersAndScenes => "Filter providers and scenes",
        GuiText::ManagedAsrModels => "Managed ASR models",
        GuiText::RegistryModelSelector => "Registry model id or short id",
        GuiText::InstallOrUpdate => "Install or update",
        GuiText::ManagedCommandAsrProviders => "Managed command ASR providers",
        GuiText::NoManagedModelsInstalled => "No managed ASR models installed.",
        GuiText::AsrProviders => "ASR providers",
        GuiText::AddCustomProvider => "Add custom provider",
        GuiText::ManagedTextAdapters => "Managed text adapters",
        GuiText::Adapters => "Adapters",
        GuiText::AddCustomAdapter => "Add custom adapter",
        GuiText::RefreshRuntime => "Refresh runtime",
        GuiText::NoTextAdaptersConfigured => "No text adapters configured.",
        GuiText::Remove => "Remove",
        GuiText::Edit => "Edit",
        GuiText::EditScript => "Edit script",
        GuiText::Start => "Start",
        GuiText::Stop => "Stop",
        GuiText::Local => "local",
        GuiText::Remote => "remote",
        GuiText::Command => "command",
        GuiText::UnselectedModel => "unselected model",
        GuiText::Active => "active",
        GuiText::Inactive => "inactive",
        GuiText::CommandAdapter => "command adapter",
        GuiText::RuntimeUnavailable => "runtime unavailable",
        GuiText::NotReportedByDaemon => "not reported by daemon",
        GuiText::Running => "running",
        GuiText::Stopped => "stopped",
        GuiText::CloseDetails => "Close details",
        GuiText::ResourceDetailsUnavailable => "Resource details unavailable",
        GuiText::Model => "Model",
        GuiText::StableId => "Stable id",
        GuiText::Status => "Status",
        GuiText::Backend => "Backend",
        GuiText::Runtime => "Runtime",
        GuiText::Family => "Family",
        GuiText::Language => "Language",
        GuiText::DeclaredSize => "Declared size",
        GuiText::RegularFiles => "Regular files",
        GuiText::Supported => "supported",
        GuiText::NotDeclared => "not declared",
        GuiText::InstallDirectory => "Install directory",
        GuiText::MetadataFile => "Metadata file",
        GuiText::Kind => "Kind",
        GuiText::Timeout => "Timeout",
        GuiText::Endpoint => "Endpoint",
        GuiText::ManagedScript => "Managed script",
        GuiText::Arguments => "Arguments",
        GuiText::Environment => "Environment",
        GuiText::LlmProvider => "LLM provider",
        GuiText::TextAdapter => "Text adapter",
        GuiText::Credential => "Credential",
        GuiText::ExtraBodyFields => "Extra body fields",
        GuiText::ExtensionFields => "Extension fields",
        GuiText::WorkingDirectory => "Working directory",
        GuiText::NotConfigured => "not configured",
        GuiText::Configured => "configured",
        GuiText::Yes => "yes",
        GuiText::No => "no",
        GuiText::AdapterLocal => "adapter/local",
        _ => unreachable!(),
    }
}
