//! Rust-owned menu snapshot controllers and finalized projections.

use crate::{
    AsrDisplaySnapshot, AsrDisplayText, AsrMenuItem, AsrMenuProjectionState, MenuFilterState,
    ProjectedMenuItem, SceneMenuItem, SceneSnapshot, project_asr_menu, project_scene_menu,
};

/// Finalized menu summary and rows shared by Scene and ASR frontends.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MenuProjection {
    /// Fully rendered current-selection summary.
    pub summary: String,
    /// Visible rows accepted by the current filter.
    pub items: Vec<ProjectedMenuItem>,
}

/// Owns the latest daemon scene snapshot used by menus and recognition actions.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SceneMenuController {
    snapshot: Option<SceneSnapshot>,
}

impl SceneMenuController {
    /// Replaces the current scene snapshot atomically.
    pub fn replace_snapshot(&mut self, snapshot: SceneSnapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Returns the latest scene snapshot, when one has been loaded.
    #[must_use]
    pub fn snapshot(&self) -> Option<&SceneSnapshot> {
        self.snapshot.as_ref()
    }

    /// Returns the latest mutable scene snapshot, when one has been loaded.
    pub fn snapshot_mut(&mut self) -> Option<&mut SceneSnapshot> {
        self.snapshot.as_mut()
    }

    /// Finalizes a menu projection from the latest snapshot and filter.
    #[must_use]
    pub fn project(&self, filter: &MenuFilterState) -> Option<MenuProjection> {
        let snapshot = self.snapshot()?;
        let scenes = snapshot
            .scenes()
            .iter()
            .map(|scene| SceneMenuItem {
                id: scene.id.clone(),
                label: scene.label.clone(),
            })
            .collect::<Vec<_>>();
        let projection = project_scene_menu(snapshot.active_scene_id(), &scenes, filter);
        Some(MenuProjection {
            summary: projection.active_label,
            items: projection.items,
        })
    }
}

/// Owns the latest daemon ASR display snapshot used by the model menu.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsrMenuController {
    snapshot: Option<AsrDisplaySnapshot>,
}

impl AsrMenuController {
    /// Replaces the current ASR display snapshot atomically.
    pub fn replace_snapshot(&mut self, snapshot: AsrDisplaySnapshot) {
        self.snapshot = Some(snapshot);
    }

    /// Returns the latest ASR display snapshot, when one has been loaded.
    #[must_use]
    pub fn snapshot(&self) -> Option<&AsrDisplaySnapshot> {
        self.snapshot.as_ref()
    }

    /// Finalizes a localized menu projection from the latest snapshot and filter.
    #[must_use]
    pub fn project(
        &self,
        filter: &MenuFilterState,
        text: &AsrDisplayText<'_>,
    ) -> Option<MenuProjection> {
        let snapshot = self.snapshot()?;
        let state = AsrMenuProjectionState {
            target_provider_id: snapshot.target_provider_id().to_owned(),
            target_model_id: snapshot.target_model_id().to_owned(),
            effective_provider_id: snapshot.effective_provider_id().to_owned(),
            effective_model_id: snapshot.effective_model_id().to_owned(),
            reload_in_progress: snapshot.reload_in_progress(),
            last_error: snapshot.last_error().to_owned(),
        };
        let targets = snapshot
            .targets()
            .iter()
            .map(|target| AsrMenuItem {
                provider_id: target.provider_id.clone(),
                kind: target.kind.clone(),
                item_id: target.item_id.clone(),
                display_title: target.display_title.clone(),
                model_value: target.model_value.clone(),
                rendered_label: snapshot.render_target_label(target, text),
            })
            .collect::<Vec<_>>();
        Some(MenuProjection {
            summary: snapshot.render_effective_label(text),
            items: project_asr_menu(&state, &targets, filter),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AsrMenuController, SceneMenuController};
    use crate::{
        AsrDisplaySnapshot, AsrDisplaySnapshotItem, AsrDisplayText, MenuControl, MenuFilterState,
        SceneSnapshot,
    };

    const TEXT: AsrDisplayText<'static> = AsrDisplayText {
        local: "Local",
        remote: "Remote",
        command: "Command",
        loading_suffix: " (loading)",
        unavailable: "unavailable",
        loading_prefix: "Loading: ",
        error_prefix: "Error: ",
    };

    #[test]
    fn scene_controller_reprojects_the_latest_active_scene() {
        let mut snapshot = SceneSnapshot::new("raw".to_owned());
        snapshot.push("raw".to_owned(), "Raw".to_owned());
        snapshot.push("meeting".to_owned(), "Meeting".to_owned());
        let mut controller = SceneMenuController::default();
        assert!(controller.project(&MenuFilterState::default()).is_none());

        controller.replace_snapshot(snapshot);
        let projection = controller
            .project(&MenuFilterState::default())
            .expect("scene projection");
        assert_eq!(projection.summary, "Raw");
        assert_eq!(projection.items.len(), 1);
        assert!(matches!(
            &projection.items[0].control,
            MenuControl::SetActiveScene { scene_id, .. } if scene_id == "meeting"
        ));

        controller
            .snapshot_mut()
            .expect("scene snapshot")
            .set_active_scene_id("meeting".to_owned());
        let projection = controller
            .project(&MenuFilterState::default())
            .expect("updated scene projection");
        assert_eq!(projection.summary, "Meeting");
        assert!(matches!(
            &projection.items[0].control,
            MenuControl::SetActiveScene { scene_id, .. } if scene_id == "raw"
        ));
    }

    #[test]
    fn asr_controller_projects_latest_snapshot_with_localization() {
        let mut snapshot = AsrDisplaySnapshot::new(
            "remote".to_owned(),
            "cloud".to_owned(),
            "local".to_owned(),
            "small".to_owned(),
            true,
            String::new(),
        );
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: "local".to_owned(),
            kind: "local".to_owned(),
            item_id: "small".to_owned(),
            display_title: "Small".to_owned(),
            model_value: "small".to_owned(),
        });
        snapshot.push(AsrDisplaySnapshotItem {
            provider_id: "remote".to_owned(),
            kind: "remote".to_owned(),
            item_id: "cloud".to_owned(),
            display_title: "Cloud".to_owned(),
            model_value: "cloud".to_owned(),
        });
        let mut controller = AsrMenuController::default();
        assert!(
            controller
                .project(&MenuFilterState::default(), &TEXT)
                .is_none()
        );

        controller.replace_snapshot(snapshot);
        let projection = controller
            .project(&MenuFilterState::default(), &TEXT)
            .expect("ASR projection");
        assert_eq!(projection.summary, "Small | Loading: remote/Cloud");
        assert_eq!(projection.items.len(), 1);
        assert_eq!(projection.items[0].label, "Cloud [Remote] (loading)");
        assert!(matches!(
            &projection.items[0].control,
            MenuControl::SetActiveAsrTarget {
                provider_id,
                model_value,
                ..
            } if provider_id == "remote" && model_value == "cloud"
        ));
    }
}
