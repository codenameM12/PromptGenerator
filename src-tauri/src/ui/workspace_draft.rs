//! Rust port of `src/lib/useWorkspaceDraft.ts` as a pure state machine.
//!
//! The React scheduler, the debounce timer, and `invoke` are replaced by the
//! recorded transition inputs. No behaviour changes: a load error leaves the
//! old draft in place, a save applies optimistically, a successful save clears
//! the error, and `clear` has no `catch`, so a rejected clear changes nothing.

use crate::commands::workspace::WorkspaceDraft;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct WorkspaceDraftState {
    pub draft: Option<WorkspaceDraft>,
    pub error: Option<String>,
}

pub fn initial() -> WorkspaceDraftState {
    WorkspaceDraftState::default()
}

pub fn load_success(state: &mut WorkspaceDraftState, loaded: Option<WorkspaceDraft>) {
    state.draft = loaded;
}

pub fn load_error(state: &mut WorkspaceDraftState, message: &str) {
    state.error = Some(message.to_string());
}

pub fn save_optimistic(state: &mut WorkspaceDraftState, next: WorkspaceDraft) {
    state.draft = Some(next);
}

pub fn save_success(state: &mut WorkspaceDraftState, saved: WorkspaceDraft) {
    state.draft = Some(saved);
    state.error = None;
}

pub fn save_error(state: &mut WorkspaceDraftState, message: &str) {
    state.error = Some(message.to_string());
}

pub fn clear_success(state: &mut WorkspaceDraftState) {
    state.draft = None;
    state.error = None;
}

/// The source `clear` awaits the IPC call without a catch: a rejection leaves
/// the state untouched (and surfaces only as a rejected hook promise).
pub fn clear_error(_state: &mut WorkspaceDraftState) {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{Guidance, edits::EditOps};
    use serde::Deserialize;
    use serde_json::Value;

    fn draft(updated_at: u64) -> WorkspaceDraft {
        WorkspaceDraft {
            asset_id: "asset-1".into(),
            edit_ops: Some(EditOps {
                crop: None,
                rotate: 0,
                flip_h: false,
                flip_v: false,
                fit: crate::commands::edits::FitMode::Fit,
                trim: None,
                fps: None,
                remove_audio: false,
                mask_path: None,
                format: None,
                loop_video: false,
                pad_color: None,
            }),
            selected_preset_id: Some("krea2".into()),
            guidance: Guidance {
                global: None,
                preset: None,
                run: None,
            },
            params: None,
            updated_at_ms: updated_at,
        }
    }

    #[test]
    fn load_error_keeps_the_previous_draft() {
        let mut state = initial();
        load_success(&mut state, Some(draft(1)));
        load_error(&mut state, "load failed");
        assert_eq!(state.draft, Some(draft(1)));
        assert_eq!(state.error.as_deref(), Some("load failed"));
    }

    #[test]
    fn save_applies_optimistically_then_clears_the_error() {
        let mut state = initial();
        load_error(&mut state, "old error");
        save_optimistic(&mut state, draft(2));
        assert_eq!(state.error.as_deref(), Some("old error"));
        save_success(&mut state, draft(3));
        assert_eq!(state.draft, Some(draft(3)));
        assert_eq!(state.error, None);
    }

    #[test]
    fn save_error_keeps_the_optimistic_draft() {
        let mut state = initial();
        save_optimistic(&mut state, draft(4));
        save_error(&mut state, "save failed");
        assert_eq!(state.draft, Some(draft(4)));
        assert_eq!(state.error.as_deref(), Some("save failed"));
    }

    #[test]
    fn clear_resets_both_and_a_failed_clear_changes_nothing() {
        let mut state = initial();
        load_success(&mut state, Some(draft(1)));
        clear_error(&mut state);
        assert_eq!(state.draft, Some(draft(1)));
        clear_success(&mut state);
        assert_eq!(state, initial());
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct Outcome<T> {
        ok: bool,
        value: Option<T>,
        message: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    #[serde(tag = "kind", rename_all = "camelCase")]
    enum Action {
        Mount {
            load: Outcome<Option<WorkspaceDraft>>,
        },
        Save {
            draft: WorkspaceDraft,
        },
        FlushSave {
            result: Outcome<WorkspaceDraft>,
        },
        Clear {
            result: Outcome<()>,
        },
        Unmount,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct RecordedCall {
        command: String,
        args: Value,
    }

    /// Typed call identity: comparing these avoids false differences from the
    /// Rust DTO serializing an absent `params` as `null` while the TS draft
    /// omits the key entirely.
    #[derive(Debug, Clone, PartialEq)]
    enum CallKind {
        GetDraft,
        Save(Box<WorkspaceDraft>),
        Clear,
    }

    fn recorded_calls(calls: &[RecordedCall]) -> Vec<CallKind> {
        calls
            .iter()
            .map(|call| match call.command.as_str() {
                "workspace_get_draft" => CallKind::GetDraft,
                "workspace_clear_draft" => CallKind::Clear,
                "workspace_save_draft" => {
                    let draft = serde_json::from_value(call.args["request"]["draft"].clone())
                        .expect("parse recorded save_draft draft");
                    CallKind::Save(Box::new(draft))
                }
                other => panic!("unexpected recorded command: {other}"),
            })
            .collect()
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedState {
        draft: Option<WorkspaceDraft>,
        error: Option<String>,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct Event {
        action: Action,
        calls: Vec<RecordedCall>,
        state: ExpectedState,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct Scenario {
        name: String,
        events: Vec<Event>,
    }

    #[derive(Debug, Deserialize)]
    struct Corpus {
        scenarios: Vec<Scenario>,
    }

    fn expected_calls(action: &Action, pending_draft: &Option<WorkspaceDraft>) -> Vec<CallKind> {
        match action {
            Action::Mount { .. } => vec![CallKind::GetDraft],
            Action::Save { .. } => Vec::new(),
            Action::FlushSave { .. } => vec![CallKind::Save(Box::new(
                pending_draft
                    .clone()
                    .expect("a save must precede the debounce flush"),
            ))],
            Action::Clear { .. } => vec![CallKind::Clear],
            Action::Unmount => Vec::new(),
        }
    }

    fn apply(action: &Action, state: &mut WorkspaceDraftState) {
        match action {
            Action::Mount { load } => {
                if load.ok {
                    load_success(state, load.value.clone().flatten());
                } else if let Some(message) = &load.message {
                    load_error(state, message);
                }
            }
            Action::Save { draft } => save_optimistic(state, draft.clone()),
            Action::FlushSave { result } => {
                if result.ok {
                    if let Some(saved) = &result.value {
                        save_success(state, saved.clone());
                    }
                } else if let Some(message) = &result.message {
                    save_error(state, message);
                }
            }
            Action::Clear { result } => {
                if result.ok {
                    clear_success(state);
                } else {
                    clear_error(state);
                }
            }
            Action::Unmount => {}
        }
    }

    #[test]
    fn matches_the_recorded_typescript_behaviour() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../port/corpus/workspace_draft.json"))
                .expect("parse workspace_draft corpus");

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();

        for scenario in &corpus.scenarios {
            let mut state = initial();
            let mut cancelled = false;
            let mut pending_draft: Option<WorkspaceDraft> = None;
            for (step, event) in scenario.events.iter().enumerate() {
                let calls = if cancelled {
                    Vec::new()
                } else {
                    expected_calls(&event.action, &pending_draft)
                };
                if !cancelled {
                    apply(&event.action, &mut state);
                }
                if let Action::Save { draft: saved } = &event.action {
                    pending_draft = Some(saved.clone());
                }
                if matches!(event.action, Action::FlushSave { .. }) {
                    pending_draft = None;
                }
                if matches!(event.action, Action::Unmount) {
                    cancelled = true;
                }

                compared += 1;
                let actual_state = ExpectedState {
                    draft: state.draft.clone(),
                    error: state.error.clone(),
                };
                let expected_calls = recorded_calls(&event.calls);
                if actual_state != event.state || calls != expected_calls {
                    differences.push(format!(
                        "{} / step {step}: expected state {:?} calls {:?}, got state {:?} calls {:?}",
                        scenario.name, event.state, expected_calls, actual_state, calls
                    ));
                }
            }
        }

        assert!(
            differences.is_empty(),
            "workspaceDraft: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "workspaceDraft: {compared} inputs compared, {} differences",
            differences.len()
        );
    }
}
