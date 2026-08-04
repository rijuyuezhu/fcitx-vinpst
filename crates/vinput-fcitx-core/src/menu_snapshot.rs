//! Rust-owned daemon snapshots consumed by the retained Fcitx adapter.

/// One scene row returned by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneSnapshotItem {
    /// Stable scene identifier.
    pub id: String,
    /// User-visible scene label.
    pub label: String,
}

/// Scene state returned by `GetSceneState`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneSnapshot {
    active_scene_id: String,
    scenes: Vec<SceneSnapshotItem>,
}

impl SceneSnapshot {
    /// Creates an empty scene snapshot for the active scene id.
    #[must_use]
    pub fn new(active_scene_id: String) -> Self {
        Self {
            active_scene_id,
            scenes: Vec::new(),
        }
    }

    /// Appends one daemon scene row in wire order.
    pub fn push(&mut self, id: String, label: String) {
        self.scenes.push(SceneSnapshotItem { id, label });
    }

    /// Returns the active scene id.
    #[must_use]
    pub fn active_scene_id(&self) -> &str {
        &self.active_scene_id
    }

    /// Updates the active scene id after a successful frontend selection.
    pub fn set_active_scene_id(&mut self, active_scene_id: String) {
        self.active_scene_id = active_scene_id;
    }

    /// Returns the scene rows in daemon order.
    #[must_use]
    pub fn scenes(&self) -> &[SceneSnapshotItem] {
        &self.scenes
    }

    /// Returns the active scene label, falling back to the stable id.
    #[must_use]
    pub fn active_label(&self) -> &str {
        self.scenes
            .iter()
            .find(|scene| scene.id == self.active_scene_id)
            .map_or(self.active_scene_id.as_str(), |scene| scene.label.as_str())
    }
}

/// One ASR display-menu row returned by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrDisplaySnapshotItem {
    /// Stable provider identifier.
    pub provider_id: String,
    /// Provider implementation kind.
    pub kind: String,
    /// Stable row identifier.
    pub item_id: String,
    /// Localized or registry-provided display title.
    pub display_title: String,
    /// Concrete model value passed back to the daemon.
    pub model_value: String,
}

impl AsrDisplaySnapshotItem {
    /// Returns the preferred user-visible base label.
    #[must_use]
    pub fn base_label(&self) -> &str {
        if self.display_title.is_empty() {
            if self.item_id.is_empty() {
                &self.provider_id
            } else {
                &self.item_id
            }
        } else {
            &self.display_title
        }
    }
}

/// Localized fragments used to render ASR menu presentation without Fcitx types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AsrDisplayText<'a> {
    /// Local provider kind label.
    pub local: &'a str,
    /// Remote provider kind label.
    pub remote: &'a str,
    /// Command provider kind label.
    pub command: &'a str,
    /// Suffix appended to the requested row while a reload is active.
    pub loading_suffix: &'a str,
    /// Fallback used when no effective backend label is available.
    pub unavailable: &'a str,
    /// Prefix used when presenting the requested backend during reload.
    pub loading_prefix: &'a str,
    /// Prefix used when presenting the last backend reload error.
    pub error_prefix: &'a str,
}

/// ASR state returned by `GetAsrDisplayMenuState`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsrDisplaySnapshot {
    target_provider_id: String,
    target_model_id: String,
    effective_provider_id: String,
    effective_model_id: String,
    reload_in_progress: bool,
    last_error: String,
    targets: Vec<AsrDisplaySnapshotItem>,
}

impl AsrDisplaySnapshot {
    /// Creates an empty ASR display snapshot from the daemon state fields.
    #[must_use]
    pub fn new(
        target_provider_id: String,
        target_model_id: String,
        effective_provider_id: String,
        effective_model_id: String,
        reload_in_progress: bool,
        last_error: String,
    ) -> Self {
        Self {
            target_provider_id,
            target_model_id,
            effective_provider_id,
            effective_model_id,
            reload_in_progress,
            last_error,
            targets: Vec::new(),
        }
    }

    /// Appends one daemon target row in wire order.
    pub fn push(&mut self, item: AsrDisplaySnapshotItem) {
        self.targets.push(item);
    }

    /// Returns the requested provider id.
    #[must_use]
    pub fn target_provider_id(&self) -> &str {
        &self.target_provider_id
    }

    /// Returns the requested model id.
    #[must_use]
    pub fn target_model_id(&self) -> &str {
        &self.target_model_id
    }

    /// Returns the provider currently serving recognition.
    #[must_use]
    pub fn effective_provider_id(&self) -> &str {
        &self.effective_provider_id
    }

    /// Returns the model currently serving recognition.
    #[must_use]
    pub fn effective_model_id(&self) -> &str {
        &self.effective_model_id
    }

    /// Returns whether the requested target is still loading.
    #[must_use]
    pub const fn reload_in_progress(&self) -> bool {
        self.reload_in_progress
    }

    /// Returns the last backend reload error.
    #[must_use]
    pub fn last_error(&self) -> &str {
        &self.last_error
    }

    /// Returns target rows in daemon order.
    #[must_use]
    pub fn targets(&self) -> &[AsrDisplaySnapshotItem] {
        &self.targets
    }

    /// Returns whether a target is the requested row currently loading.
    #[must_use]
    pub fn is_loading_target(&self, target: &AsrDisplaySnapshotItem) -> bool {
        self.reload_in_progress
            && target.provider_id == self.target_provider_id
            && target.model_value == self.target_model_id
    }

    /// Resolves a provider/model pair to its preferred display title.
    #[must_use]
    pub fn display_title_for<'a>(&'a self, provider_id: &str, model_value: &'a str) -> &'a str {
        self.targets
            .iter()
            .find(|target| target.provider_id == provider_id && target.model_value == model_value)
            .map_or(model_value, AsrDisplaySnapshotItem::base_label)
    }

    /// Returns the preferred base label for the effective backend.
    #[must_use]
    pub fn effective_base_label(&self) -> &str {
        if self.effective_model_id.is_empty() {
            &self.effective_provider_id
        } else {
            self.display_title_for(&self.effective_provider_id, &self.effective_model_id)
        }
    }

    /// Returns the preferred base label for the requested backend.
    #[must_use]
    pub fn target_base_label(&self) -> &str {
        if self.target_model_id.is_empty() {
            &self.target_provider_id
        } else {
            self.display_title_for(&self.target_provider_id, &self.target_model_id)
        }
    }

    /// Renders one target row using localized provider-kind and loading fragments.
    #[must_use]
    pub fn render_target_label(
        &self,
        target: &AsrDisplaySnapshotItem,
        text: &AsrDisplayText<'_>,
    ) -> String {
        let kind = match target.kind.as_str() {
            "local" => text.local,
            "remote" => text.remote,
            "command" => text.command,
            other => other,
        };
        let mut label = format!("{} [{kind}]", target.base_label());
        if self.is_loading_target(target) {
            label.push_str(text.loading_suffix);
        }
        label
    }

    /// Renders the effective backend summary using localized status fragments.
    #[must_use]
    pub fn render_effective_label(&self, text: &AsrDisplayText<'_>) -> String {
        let mut label = self.effective_base_label().to_owned();
        if label.is_empty() {
            label.push_str(text.unavailable);
        }
        if self.reload_in_progress && !self.target_provider_id.is_empty() {
            label.push_str(" | ");
            label.push_str(text.loading_prefix);
            label.push_str(&self.target_provider_id);
            if !self.target_model_id.is_empty() {
                label.push('/');
                label.push_str(self.target_base_label());
            }
        }
        if !self.last_error.is_empty() {
            label.push_str(" | ");
            label.push_str(text.error_prefix);
            label.push_str(&self.last_error);
        }
        label
    }
}

#[cfg(test)]
mod tests {
    use super::{AsrDisplaySnapshot, AsrDisplaySnapshotItem, AsrDisplayText, SceneSnapshot};

    const TEXT: AsrDisplayText<'static> = AsrDisplayText {
        local: "Local",
        remote: "Remote",
        command: "Command",
        loading_suffix: " (loading)",
        unavailable: "unavailable",
        loading_prefix: "Loading: ",
        error_prefix: "Error: ",
    };

    fn target(
        provider_id: &str,
        item_id: &str,
        display_title: &str,
        model_value: &str,
    ) -> AsrDisplaySnapshotItem {
        AsrDisplaySnapshotItem {
            provider_id: provider_id.to_owned(),
            kind: "local".to_owned(),
            item_id: item_id.to_owned(),
            display_title: display_title.to_owned(),
            model_value: model_value.to_owned(),
        }
    }

    #[test]
    fn owns_scene_rows_and_resolves_active_label() {
        let mut snapshot = SceneSnapshot::new("meeting".to_owned());
        snapshot.push("raw".to_owned(), "Raw".to_owned());
        snapshot.push("meeting".to_owned(), "Meeting Notes".to_owned());

        assert_eq!(snapshot.active_scene_id(), "meeting");
        assert_eq!(snapshot.active_label(), "Meeting Notes");
        assert_eq!(snapshot.scenes()[0].id, "raw");

        snapshot.set_active_scene_id("missing".to_owned());
        assert_eq!(snapshot.active_label(), "missing");
    }

    #[test]
    fn resolves_asr_labels_and_loading_row() {
        let mut snapshot = AsrDisplaySnapshot::new(
            "sherpa".to_owned(),
            "requested".to_owned(),
            "sherpa".to_owned(),
            "effective".to_owned(),
            true,
            String::new(),
        );
        snapshot.push(target(
            "sherpa",
            "effective",
            "Effective Model",
            "effective",
        ));
        snapshot.push(target("sherpa", "requested", "", "requested"));

        assert_eq!(snapshot.effective_base_label(), "Effective Model");
        assert_eq!(snapshot.target_base_label(), "requested");
        assert!(snapshot.is_loading_target(&snapshot.targets()[1]));
        assert!(!snapshot.is_loading_target(&snapshot.targets()[0]));
    }

    #[test]
    fn falls_back_from_missing_display_metadata() {
        let mut snapshot = AsrDisplaySnapshot::new(
            "remote".to_owned(),
            String::new(),
            "remote".to_owned(),
            "unknown-model".to_owned(),
            false,
            "reload failed".to_owned(),
        );
        snapshot.push(target("remote", "", "", ""));

        assert_eq!(snapshot.targets()[0].base_label(), "remote");
        assert_eq!(snapshot.effective_base_label(), "unknown-model");
        assert_eq!(snapshot.target_base_label(), "remote");
        assert_eq!(snapshot.last_error(), "reload failed");
    }

    #[test]
    fn renders_localized_target_rows_and_preserves_unknown_kinds() {
        let mut snapshot = AsrDisplaySnapshot::new(
            "sherpa".to_owned(),
            "requested".to_owned(),
            "sherpa".to_owned(),
            "effective".to_owned(),
            true,
            String::new(),
        );
        snapshot.push(target("sherpa", "requested", "Requested", "requested"));
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: "custom".to_owned(),
            kind: "plugin".to_owned(),
            item_id: "custom-model".to_owned(),
            display_title: "Custom".to_owned(),
            model_value: "custom-model".to_owned(),
        });

        assert_eq!(
            snapshot.render_target_label(&snapshot.targets()[0], &TEXT),
            "Requested [Local] (loading)"
        );
        assert_eq!(
            snapshot.render_target_label(&snapshot.targets()[1], &TEXT),
            "Custom [plugin]"
        );
    }

    #[test]
    fn renders_effective_loading_and_error_summary() {
        let mut snapshot = AsrDisplaySnapshot::new(
            "sherpa".to_owned(),
            "requested".to_owned(),
            "sherpa".to_owned(),
            "effective".to_owned(),
            true,
            "reload failed".to_owned(),
        );
        snapshot.push(target(
            "sherpa",
            "effective",
            "Effective Model",
            "effective",
        ));
        snapshot.push(target(
            "sherpa",
            "requested",
            "Requested Model",
            "requested",
        ));

        assert_eq!(
            snapshot.render_effective_label(&TEXT),
            "Effective Model | Loading: sherpa/Requested Model | Error: reload failed"
        );

        let unavailable = AsrDisplaySnapshot::new(
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            false,
            String::new(),
        );
        assert_eq!(unavailable.render_effective_label(&TEXT), "unavailable");
    }
}
