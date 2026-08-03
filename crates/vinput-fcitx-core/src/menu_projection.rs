//! Pure projection of daemon menu snapshots into visible frontend rows.

use crate::MenuFilterState;

/// One scene row returned by the daemon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SceneMenuItem {
    /// Stable scene identifier.
    pub id: String,
    /// User-visible scene label.
    pub label: String,
}

/// One visible menu row and its original snapshot position.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectedMenuItem {
    /// User-visible label rendered by the C++ adapter.
    pub label: String,
    /// Daemon control operation selected by this row.
    pub control: MenuControl,
}

/// Complete daemon control operation attached to a projected menu row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MenuControl {
    /// Select a scene by stable identifier.
    SetActiveScene {
        /// Stable scene identifier passed to the daemon.
        scene_id: String,
        /// User-visible label used by frontend feedback.
        display_label: String,
    },
    /// Select an ASR provider/model target.
    SetActiveAsrTarget {
        /// Stable provider identifier passed to the daemon.
        provider_id: String,
        /// Concrete model value passed to the daemon.
        model_value: String,
        /// User-visible base label used by frontend feedback.
        display_label: String,
    },
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
                label: scene.label.clone(),
                control: MenuControl::SetActiveScene {
                    scene_id: scene.id.clone(),
                    display_label: scene.label.clone(),
                },
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
            label: target.rendered_label.clone(),
            control: MenuControl::SetActiveAsrTarget {
                provider_id: target.provider_id.clone(),
                model_value: target.model_value.clone(),
                display_label: asr_display_label(target).to_owned(),
            },
        })
        .collect()
}

fn asr_display_label(target: &AsrMenuItem) -> &str {
    if !target.display_title.is_empty() {
        &target.display_title
    } else if !target.item_id.is_empty() {
        &target.item_id
    } else {
        &target.model_value
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AsrMenuItem, AsrMenuProjectionState, MenuControl, SceneMenuItem, is_effective_asr_target,
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
                id: "__raw__".to_owned(),
                label: "Raw Dictation".to_owned(),
            },
            SceneMenuItem {
                id: "meeting".to_owned(),
                label: "Meeting Notes".to_owned(),
            },
            SceneMenuItem {
                id: "code".to_owned(),
                label: "Code Review".to_owned(),
            },
        ];

        let projection = project_scene_menu("meeting", &scenes, &active_filter("code review"));
        assert_eq!(projection.active_label, "Meeting Notes");
        assert_eq!(projection.items.len(), 1);
        assert_eq!(projection.items[0].label, "Code Review");
        assert_eq!(
            projection.items[0].control,
            MenuControl::SetActiveScene {
                scene_id: "code".to_owned(),
                display_label: "Code Review".to_owned(),
            }
        );
    }

    #[test]
    fn scene_projection_falls_back_to_active_id() {
        let projection = project_scene_menu(
            "missing",
            &[SceneMenuItem {
                id: "other".to_owned(),
                label: "Other".to_owned(),
            }],
            &MenuFilterState::default(),
        );
        assert_eq!(projection.active_label, "missing");
    }

    fn asr_item(provider: &str, model: &str, label: &str) -> AsrMenuItem {
        AsrMenuItem {
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
            asr_item("sherpa", "moonshine-en", "Moonshine English"),
            asr_item("sherpa", "paraformer-zh", "Paraformer Chinese"),
            asr_item("command", "custom", "Custom Command"),
        ];

        let projected = project_asr_menu(&state, &targets, &active_filter("chinese local"));
        assert_eq!(projected.len(), 1);
        assert_eq!(projected[0].label, "Paraformer Chinese [Local]");
        assert_eq!(
            projected[0].control,
            MenuControl::SetActiveAsrTarget {
                provider_id: "sherpa".to_owned(),
                model_value: "paraformer-zh".to_owned(),
                display_label: "Paraformer Chinese".to_owned(),
            }
        );
    }

    #[test]
    fn treats_stable_target_as_effective_after_successful_reload() {
        let target = asr_item("sherpa", "requested", "Requested");
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

    #[test]
    fn asr_control_label_uses_stable_fallbacks() {
        let state = AsrMenuProjectionState::default();
        let target = AsrMenuItem {
            provider_id: "remote".to_owned(),
            kind: "remote".to_owned(),
            item_id: "configured-endpoint".to_owned(),
            display_title: String::new(),
            model_value: "wire-value".to_owned(),
            rendered_label: "configured-endpoint [Remote]".to_owned(),
        };
        let projected = project_asr_menu(&state, &[target], &MenuFilterState::default());
        assert_eq!(
            projected[0].control,
            MenuControl::SetActiveAsrTarget {
                provider_id: "remote".to_owned(),
                model_value: "wire-value".to_owned(),
                display_label: "configured-endpoint".to_owned(),
            }
        );
    }
}
