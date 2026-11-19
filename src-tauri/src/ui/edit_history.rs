//! Rust port of `src/lib/media/editHistory.ts`.
//!
//! This is a behaviour-preserving move. The duplicate check deliberately uses
//! serialized JSON (as the source does with `JSON.stringify`), and the 100-entry
//! cap is kept. Fixes belong in `PORTING.md`, not here.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FitMode {
    Fit,
    Fill,
    Pad,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Crop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trim {
    pub start_ms: f64,
    pub end_ms: f64,
}

/// Field order matches `DEFAULT_EDIT_OPS` in the source, because the duplicate
/// check compares serialized objects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditOps {
    pub crop: Option<Crop>,
    pub rotate: f64,
    pub flip_h: bool,
    pub flip_v: bool,
    pub fit: FitMode,
    pub trim: Option<Trim>,
    pub fps: Option<f64>,
    pub remove_audio: bool,
    pub mask_path: Option<String>,
    pub format: Option<String>,
    pub loop_video: bool,
    pub pad_color: Option<String>,
}

impl Default for EditOps {
    fn default() -> Self {
        Self {
            crop: None,
            rotate: 0.0,
            flip_h: false,
            flip_v: false,
            fit: FitMode::Fit,
            trim: None,
            fps: None,
            remove_audio: false,
            mask_path: None,
            format: None,
            loop_video: false,
            pad_color: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EditHistory {
    pub entries: Vec<EditOps>,
    pub index: usize,
}

pub fn create_history(initial: EditOps) -> EditHistory {
    EditHistory {
        entries: vec![initial],
        index: 0,
    }
}

pub fn current(history: &EditHistory) -> &EditOps {
    &history.entries[history.index]
}

pub fn can_undo(history: &EditHistory) -> bool {
    history.index > 0
}

pub fn can_redo(history: &EditHistory) -> bool {
    history.index < history.entries.len() - 1
}

pub fn push(history: EditHistory, next: EditOps) -> EditHistory {
    let current_json = serde_json::to_string(current(&history)).expect("serialize EditOps");
    let next_json = serde_json::to_string(&next).expect("serialize EditOps");
    if current_json == next_json {
        return history;
    }

    let keep = history.index + 1;
    let mut entries = history.entries;
    entries.truncate(keep);
    entries.push(next);
    if entries.len() > 100 {
        let excess = entries.len() - 100;
        entries.drain(0..excess);
    }
    let index = entries.len() - 1;
    EditHistory { entries, index }
}

pub fn undo(history: EditHistory) -> EditHistory {
    if can_undo(&history) {
        EditHistory {
            index: history.index - 1,
            ..history
        }
    } else {
        history
    }
}

pub fn redo(history: EditHistory) -> EditHistory {
    if can_redo(&history) {
        EditHistory {
            index: history.index + 1,
            ..history
        }
    } else {
        history
    }
}

pub fn reset(history: EditHistory) -> EditHistory {
    push(history, EditOps::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn records_edits_and_supports_undo_redo() {
        let mut history = create_history(EditOps::default());
        assert!(!can_undo(&history));

        history = push(
            history,
            EditOps {
                rotate: 90.0,
                ..EditOps::default()
            },
        );
        assert_eq!(current(&history).rotate, 90.0);
        assert!(can_undo(&history));

        history = undo(history);
        assert_eq!(current(&history).rotate, 0.0);
        assert!(can_redo(&history));

        history = redo(history);
        assert_eq!(current(&history).rotate, 90.0);
    }

    #[test]
    fn drops_the_redo_branch_after_a_new_edit() {
        let mut history = create_history(EditOps::default());
        history = push(
            history,
            EditOps {
                rotate: 90.0,
                ..EditOps::default()
            },
        );
        history = push(
            history,
            EditOps {
                rotate: 180.0,
                ..EditOps::default()
            },
        );
        history = undo(history);
        history = push(
            history,
            EditOps {
                rotate: 270.0,
                ..EditOps::default()
            },
        );

        assert_eq!(current(&history).rotate, 270.0);
        assert!(!can_redo(&history));
        assert_eq!(history.entries.len(), 3);
    }

    #[test]
    fn ignores_duplicate_states() {
        let history = create_history(EditOps::default());
        let history = push(history, EditOps::default());
        assert_eq!(history.entries.len(), 1);
    }

    #[test]
    fn resets_to_the_default_operations() {
        let history = create_history(EditOps::default());
        let history = push(
            history,
            EditOps {
                rotate: 90.0,
                flip_h: true,
                ..EditOps::default()
            },
        );
        let history = reset(history);
        assert_eq!(current(&history), &EditOps::default());
        assert!(can_undo(&history));
    }

    // ---- Differential harness: replay the corpus recorded from the TS source ----

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    enum Action {
        Create,
        Push { ops: EditOps },
        Undo,
        Redo,
        Reset,
    }

    #[derive(Debug, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct Observation {
        current: EditOps,
        can_undo: bool,
        can_redo: bool,
        entry_count: usize,
        index: usize,
        entries: Vec<EditOps>,
    }

    #[derive(Debug, Deserialize)]
    struct Case {
        name: String,
        observe: String,
        actions: Vec<Action>,
        observations: Vec<Observation>,
    }

    #[derive(Debug, Deserialize)]
    struct Corpus {
        cases: Vec<Case>,
    }

    fn observe(history: &EditHistory) -> Observation {
        Observation {
            current: current(history).clone(),
            can_undo: can_undo(history),
            can_redo: can_redo(history),
            entry_count: history.entries.len(),
            index: history.index,
            entries: history.entries.clone(),
        }
    }

    #[test]
    fn matches_the_recorded_typescript_behaviour() {
        let corpus: Corpus =
            serde_json::from_str(include_str!("../../../port/corpus/edit_history.json"))
                .expect("parse edit_history corpus");

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();

        for case in &corpus.cases {
            let mut history: Option<EditHistory> = None;
            let final_only = case.observe == "final";

            for (step, action) in case.actions.iter().enumerate() {
                history = Some(match action {
                    Action::Create => create_history(EditOps::default()),
                    Action::Push { ops } => push(history.take().unwrap(), ops.clone()),
                    Action::Undo => undo(history.take().unwrap()),
                    Action::Redo => redo(history.take().unwrap()),
                    Action::Reset => reset(history.take().unwrap()),
                });

                if final_only && step + 1 != case.actions.len() {
                    continue;
                }
                let observation_index = if final_only { 0 } else { step };
                let expected = &case.observations[observation_index];
                let actual = observe(history.as_ref().unwrap());
                compared += 1;
                if &actual != expected {
                    differences.push(format!(
                        "{} / step {step}: expected {expected:?}, got {actual:?}",
                        case.name
                    ));
                }
            }
        }

        assert!(
            differences.is_empty(),
            "editHistory: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "editHistory: {compared} inputs compared, {} differences",
            differences.len()
        );
    }
}
