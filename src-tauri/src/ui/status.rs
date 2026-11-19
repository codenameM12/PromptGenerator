//! Rust port of `src/lib/useStatus.ts` as a pure state machine.
//!
//! The polling interval and `invoke` are replaced by recorded transition
//! inputs. The source quirk is kept: a failed poll preserves the previous
//! snapshot while clearing `checking`, and an error uses the hook's own
//! `error instanceof Error ? message : String(error)` rule.

use crate::commands::status::StatusSnapshot;

#[derive(Debug, Clone, PartialEq)]
pub struct StatusState {
    pub snapshot: Option<StatusSnapshot>,
    pub error: Option<String>,
    pub checking: bool,
}

pub fn initial() -> StatusState {
    StatusState {
        snapshot: None,
        error: None,
        checking: true,
    }
}

pub fn poll_success(state: &mut StatusState, snapshot: StatusSnapshot) {
    state.snapshot = Some(snapshot);
    state.error = None;
    state.checking = false;
}

pub fn poll_error(state: &mut StatusState, message: &str) {
    state.error = Some(message.to_string());
    state.checking = false;
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;
    use serde_json::{Value, json};

    fn snapshot(active_jobs: i64) -> StatusSnapshot {
        StatusSnapshot {
            endpoint: "http://127.0.0.1:1234".into(),
            reachable: true,
            latency_ms: Some(12),
            loaded_models: vec!["model-a".into()],
            active_jobs,
            cache_bytes: 4096,
            data_bytes: 8192,
            portable: false,
            storage_note: None,
            checked_at_ms: 1000,
        }
    }

    #[test]
    fn starts_checking_then_publishes_a_snapshot() {
        let mut state = initial();
        assert!(state.checking);
        poll_success(&mut state, snapshot(2));
        assert_eq!(state.snapshot, Some(snapshot(2)));
        assert!(!state.checking);
        assert_eq!(state.error, None);
    }

    #[test]
    fn an_error_keeps_the_previous_snapshot_and_recovers() {
        let mut state = initial();
        poll_success(&mut state, snapshot(2));
        poll_error(&mut state, "runtime down");
        assert_eq!(state.snapshot, Some(snapshot(2)));
        assert_eq!(state.error.as_deref(), Some("runtime down"));
        poll_success(&mut state, snapshot(1));
        assert_eq!(state.error, None);
        assert_eq!(state.snapshot, Some(snapshot(1)));
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
        Initial,
        Mount {
            first: Outcome<StatusSnapshot>,
            #[serde(rename = "intervalMs")]
            interval_ms: u64,
        },
        Poll {
            result: Outcome<StatusSnapshot>,
        },
        Unmount,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    struct Call {
        command: String,
        args: Value,
    }

    #[derive(Debug, Clone, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct ExpectedState {
        snapshot: Option<StatusSnapshot>,
        error: Option<String>,
        checking: bool,
    }

    #[derive(Debug, Clone, Deserialize)]
    struct Event {
        action: Action,
        calls: Vec<Call>,
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

    fn snapshot_call() -> Call {
        Call {
            command: "status_snapshot".into(),
            args: json!({}),
        }
    }

    fn expected_calls(action: &Action, cancelled: bool) -> Vec<Call> {
        if cancelled {
            return Vec::new();
        }
        match action {
            Action::Initial | Action::Poll { .. } => vec![snapshot_call()],
            Action::Mount { .. } | Action::Unmount => Vec::new(),
        }
    }

    fn apply(action: &Action, state: &mut StatusState) {
        match action {
            Action::Initial | Action::Unmount => {}
            Action::Mount { first, interval_ms } => {
                let _ = interval_ms;
                apply_outcome(first, state);
            }
            Action::Poll { result } => apply_outcome(result, state),
        }
    }

    fn apply_outcome(first: &Outcome<StatusSnapshot>, state: &mut StatusState) {
        if first.ok {
            if let Some(value) = &first.value {
                poll_success(state, value.clone());
            }
        } else if let Some(message) = &first.message {
            poll_error(state, message);
        }
    }

    #[test]
    fn matches_the_recorded_typescript_behaviour() {
        let corpus: Corpus = serde_json::from_str(include_str!("../../../port/corpus/status.json"))
            .expect("parse status corpus");

        let mut compared = 0usize;
        let mut differences: Vec<String> = Vec::new();

        for scenario in &corpus.scenarios {
            let mut state = initial();
            let mut cancelled = false;
            for (step, event) in scenario.events.iter().enumerate() {
                let calls = expected_calls(&event.action, cancelled);
                if !cancelled {
                    apply(&event.action, &mut state);
                }
                if matches!(event.action, Action::Unmount) {
                    cancelled = true;
                }

                compared += 1;
                let actual_state = ExpectedState {
                    snapshot: state.snapshot.clone(),
                    error: state.error.clone(),
                    checking: state.checking,
                };
                if actual_state != event.state || calls != event.calls {
                    differences.push(format!(
                        "{} / step {step}: expected state {:?} calls {:?}, got state {:?} calls {:?}",
                        scenario.name, event.state, event.calls, actual_state, calls
                    ));
                }
            }
        }

        assert!(
            differences.is_empty(),
            "status: {compared} inputs compared, {} differences\n{differences:#?}",
            differences.len()
        );
        println!(
            "status: {compared} inputs compared, {} differences",
            differences.len()
        );
    }
}
