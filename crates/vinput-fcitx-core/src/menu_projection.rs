//! Pure projection of daemon menu snapshots into visible frontend rows.

use crate::MenuFilterState;

/// One scene row returned by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMenuItem {
    /// Position in the original daemon snapshot.
    pub source_index: usize,
    /// Stable scene identifier.
    pub id: String,
    /// User-visible scene label.
    pub label: String,
}

/// One visible menu row and its original snapshot position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedMenuItem {
    /// Position in the original daemon snapshot.
    pub source_index: usize,
    /// User-visible label rendered by the C++ adapter.
    pub label: String,
}

/// Visible scene rows plus the label for the active scene.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMenuProjection {
    /// Active scene label, or the stable id when no matching row exists.
    pub active_label: String,
    /// Non-active rows accepted by the current filter.
    pub items: Vec<ProjectedMenuItem>,
}

/// Projects a scene snapshot into visible candidate rows.
#[must_use]
pub fn project_scene_menu(
    active_scene_id: &str,
    scenes: &[SceneMenuItem],
    filter: &MenuFilterState,
) -> SceneMenuProjection {
    let mut active_label = active_scene_id.to_owned();
    let mut items = Vec::new();

    for scene in scenes {
        if scene.id == active_scene_id {
            active_label.clone_from(&scene.label);
            continue;
        }

        let search_text = format!("{} {}", scene.label, scene.id);
        if filter.matches(&search_text) {
            items.push(ProjectedMenuItem {
                source_index: scene.source_index,
                label: scene.label.clone(),
            });
        }
    }

    SceneMenuProjection {
        active_label,
        items,
    }
}

/// ASR target/effective state needed by menu projection.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AsrMenuProjectionState {
    /// Requested provider during or after reload.
    pub target_provider_id: String,
    /// Requested model during or after reload.
    pub target_model_id: String,
    /// Provider currently serving recognition.
    pub effective_provider_id: String,
    /// Model currently serving recognition.
    pub effective_model_id: String,
    /// Whether the requested backend is still loading.
    pub reload_in_progress: bool,
    /// Last reload error; an empty value means no error.
    pub last_error: String,
}

/// One ASR row prepared by the C++ localization adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsrMenuItem {
    /// Position in the original daemon snapshot.
    pub source_index: usize,
    /// Stable provider identifier.
    pub provider_id: String,
    /// Provider kind used for search.
    pub kind: String,
    /// Stable row identifier used for search.
    pub item_id: String,
    /// Localized or stable title used for search.
    pub display_title: String,
    /// Concrete configuration value used for selection and search.
    pub model_value: String,
    /// Fully rendered, localized candidate label.
    pub rendered_label: String,
}

/// Returns whether a row represents the effective ASR target.
#[must_use]
pub fn is_effective_asr_target(target: &AsrMenuItem, state: &AsrMenuProjectionState) -> bool {
    if target.provider_id != state.effective_provider_id {
        return false;
    }
    if target.model_value == state.effective_model_id {
        return true;
    }
    !state.reload_in_progress
        && state.last_error.is_empty()
        && target.provider_id == state.target_provider_id
        && target.model_value == state.target_model_id
}

/// Projects ASR rows into non-effective candidates accepted by the filter.
#[must_use]
pub fn project_asr_menu(
    state: &AsrMenuProjectionState,
    targets: &[AsrMenuItem],
    filter: &MenuFilterState,
) -> Vec<ProjectedMenuItem> {
    targets
        .iter()
        .filter(|target| !is_effective_asr_target(target, state))
        .filter(|target| {
            let search_text = format!(
                "{} {} {} {} {} {}",
                target.rendered_label,
                target.provider_id,
                target.kind,
                target.item_id,
                target.display_title,
                target.model_value
            );
            filter.matches(&search_text)
        })
        .map(|target| ProjectedMenuItem {
            source_index: target.source_index,
            label: target.rendered_label.clone(),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        AsrMenuItem, AsrMenuProjectionState, SceneMenuItem, is_effective_asr_target,
        project_asr_menu, project_scene_menu,
    };
    use crate::MenuFilterState;

    fn active_filter(query: &str) -> MenuFilterState {
        let mut filter = MenuFilterState::default();
        filter.activate();
        filter.append_text(query);
        filter
    }

    #[test]
    fn projects_scene_rows_and_active_label() {
        let scenes = vec![
            SceneMenuItem {
                source_index: 0,
                id: "__raw__".to_owned(),
                label: "Raw Dictation".to_owned(),
            },
            SceneMenuItem {
                source_index: 1,
                id: "meeting".to_owned(),
                label: "Meeting Notes".to_owned(),
            },
            SceneMenuItem {
                source_index: 2,
                id: "code".to_owned(),
                label: "Code Review".to_owned(),
            },
        ];

        let projection = project_scene_menu("meeting", &scenes, &active_filter("code review"));
        assert_eq!(projection.active_label, "Meeting Notes");
        assert_eq!(projection.items.len(), 1);
        assert_eq!(projection.items[0].source_index, 2);
        assert_eq!(projection.items[0].label, "Code Review");
    }

    #[test]
    fn scene_projection_falls_back_to_active_id() {
        let projection = project_scene_menu(
            "missing",
            &[SceneMenuItem {
                source_index: 7,
                id: "other".to_owned(),
                label: "Other".to_owned(),
            }],
            &MenuFilterState::default(),
        );
        assert_eq!(projection.active_label, "missing");
        assert_eq!(projection.items[0].source_index, 7);
    }

    fn asr_item(source_index: usize, provider: &str, model: &str, label: &str) -> AsrMenuItem {
        AsrMenuItem {
            source_index,
            provider_id: provider.to_owned(),
            kind: "local".to_owned(),
            item_id: model.to_owned(),
            display_title: label.to_owned(),
            model_value: model.to_owned(),
            rendered_label: format!("{label} [Local]"),
        }
    }

    #[test]
    fn excludes_the_effective_asr_row_and_filters_remaining_rows() {
        let state = AsrMenuProjectionState {
            target_provider_id: "sherpa".to_owned(),
            target_model_id: "moonshine-en".to_owned(),
            effective_provider_id: "sherpa".to_owned(),
            effective_model_id: "moonshine-en".to_owned(),
            reload_in_progress: false,
            last_error: String::new(),
        };
        let targets = vec![
            asr_item(0, "sherpa", "moonshine-en", "Moonshine English"),
            asr_item(1, "sherpa", "paraformer-zh", "Paraformer Chinese"),
            asr_item(2, "command", "custom", "Custom Command"),
        ];

        let projected = project_asr_menu(&state, &targets, &active_filter("chinese local"));
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].source_index, 1);
        assert_eq!(projected[0].label, "Paraformer Chinese [Local]");
    }

    #[test]
    fn treats_stable_target_as_effective_after_successful_reload() {
        let target = asr_item(4, "sherpa", "requested", "Requested");
        let mut state = AsrMenuProjectionState {
            target_provider_id: "sherpa".to_owned(),
            target_model_id: "requested".to_owned(),
            effective_provider_id: "sherpa".to_owned(),
            effective_model_id: "legacy-reported-id".to_owned(),
            reload_in_progress: false,
            last_error: String::new(),
        };
        assert!(is_effective_asr_target(&target, &state));

        state.reload_in_progress = true;
        assert!(!is_effective_asr_target(&target, &state));
        state.reload_in_progress = false;
        state.last_error = "reload failed".to_owned();
        assert!(!is_effective_asr_target(&target, &state));
    }
}
