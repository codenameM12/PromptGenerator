//! Differential parity for Jobs + Datasets against the source-driven corpus.
//!
//! The panel replay functions and the `Backend` trait are imported from source
//! by path (the crate has no lib target). `RecordingBackend` implements the
//! existing trait, logs ordered `(command, JSON arguments)` and returns
//! scripted responses; it also collects the calls the replay closure emits.

#[path = "../src/backend.rs"]
mod backend;
#[path = "../src/views/datasets.rs"]
mod datasets_view;
#[path = "../src/errors.rs"]
mod errors;
#[path = "../src/views/jobs.rs"]
mod jobs_view;
#[path = "../src/persistence.rs"]
mod persistence;
#[path = "../src/platform/mod.rs"]
mod platform;
#[path = "../src/shortcuts.rs"]
mod shortcuts;
#[path = "../src/theme.rs"]
mod theme;
#[path = "../src/view_model.rs"]
mod view_model;

use std::cell::RefCell;
use std::collections::HashMap;

use serde::Deserialize;
use serde::de::DeserializeOwned;
use serde_json::{Value, json};

use backend::{
    Asset, Backend, BackendError, ComposeRequest, DescribeRequest, DragPrepareRequest, Generation,
    ImportResult, PreparedDrag,
};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct Call {
    command: String,
    args: Value,
}

#[derive(Debug, Default)]
struct RecordingBackend {
    calls: RefCell<Vec<Call>>,
    scripted: RefCell<HashMap<String, Value>>,
}

impl RecordingBackend {
    fn with_scripted(scripted: HashMap<String, Value>) -> Self {
        Self {
            calls: RefCell::new(Vec::new()),
            scripted: RefCell::new(scripted),
        }
    }

    fn record(&self, command: &str, args: Value) {
        self.calls.borrow_mut().push(Call {
            command: command.to_string(),
            args,
        });
    }

    fn take_calls(&self) -> Vec<Call> {
        self.calls.borrow_mut().drain(..).collect()
    }

    fn scripted<T: DeserializeOwned + Default>(&self, command: &str) -> T {
        self.scripted
            .borrow()
            .get(command)
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok())
            .unwrap_or_default()
    }
}

impl Backend for RecordingBackend {
    fn list_assets(&self) -> Result<Vec<Asset>, BackendError> {
        self.record("assets_list", json!({}));
        Ok(self.scripted("assets_list"))
    }
    fn import_paths(&self, paths: &[String]) -> Result<Vec<ImportResult>, BackendError> {
        self.record("assets_import", json!({ "request": { "paths": paths } }));
        Ok(self.scripted("assets_import"))
    }
    fn import_clipboard(
        &self,
        file_name: Option<&str>,
        bytes: &[u8],
    ) -> Result<Vec<ImportResult>, BackendError> {
        self.record(
            "assets_import_clipboard",
            json!({ "request": { "fileName": file_name, "bytes": bytes.len() } }),
        );
        Ok(self.scripted("assets_import_clipboard"))
    }
    fn run_describe(&self, request: DescribeRequest) -> Result<Vec<String>, BackendError> {
        self.record(
            "pipeline_describe",
            serde_json::to_value(request).unwrap_or_default(),
        );
        Ok(self.scripted("pipeline_describe"))
    }
    fn run_compose(&self, request: ComposeRequest) -> Result<Vec<Generation>, BackendError> {
        self.record(
            "pipeline_compose",
            serde_json::to_value(request).unwrap_or_default(),
        );
        Ok(self.scripted("pipeline_compose"))
    }
    fn cancel(&self) -> Result<(), BackendError> {
        self.record("pipeline_cancel", json!({}));
        Ok(())
    }
    fn prepare_drag(&self, request: DragPrepareRequest) -> Result<PreparedDrag, BackendError> {
        self.record(
            "drag_prepare",
            serde_json::to_value(request).unwrap_or_default(),
        );
        let scripted = self
            .scripted
            .borrow()
            .get("drag_prepare")
            .cloned()
            .and_then(|value| serde_json::from_value(value).ok());
        Ok(scripted.unwrap_or(PreparedDrag {
            files: Vec::new(),
            prepared_dir: String::new(),
            suggested_name: String::new(),
        }))
    }

    fn call(&self, command: &str, args: Value) -> Result<Value, BackendError> {
        self.record(command, args);
        Ok(self
            .scripted
            .borrow()
            .get(command)
            .cloned()
            .unwrap_or(Value::Null))
    }
}

#[derive(Debug, Deserialize)]
struct Corpus {
    fixtures: Value,
    scenarios: Vec<Scenario>,
}

#[derive(Debug, Deserialize)]
struct Scenario {
    name: String,
    panel: String,
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct Event {
    action: Value,
    calls: Vec<Call>,
    state: Value,
}

const CORPUS: &str = include_str!("../../port/corpus/jobs_datasets.json");

#[test]
fn jobs_and_datasets_match_the_recorded_typescript_behaviour() {
    let corpus: Corpus = serde_json::from_str(CORPUS).expect("parse corpus");
    let mut compared = 0usize;
    let mut differences = 0usize;

    for scenario in &corpus.scenarios {
        let mut jobs_state = jobs_view::JobsUiState::default();
        let mut datasets_state = datasets_view::DatasetsUiState::default();
        let backend = RecordingBackend::with_scripted(HashMap::new());
        let record = |command: &str, args: Value| {
            // Route the replay through the same Backend::call seam the live
            // view uses; the test double logs the call and returns a script.
            let _ = backend.call(command, args);
        };

        for event in &scenario.events {
            if event.action.get("kind").and_then(Value::as_str) == Some("mount") {
                backend.take_calls();
            }
            match scenario.panel.as_str() {
                "jobs" => jobs_view::replay(&event.action, &record, &mut jobs_state),
                "datasets" => datasets_view::replay(
                    &event.action,
                    &record,
                    &mut datasets_state,
                    &corpus.fixtures,
                ),
                other => panic!("unknown panel {other}"),
            }
            let actual_calls = backend.take_calls();
            compared += 1;
            if actual_calls != event.calls {
                differences += 1;
                eprintln!(
                    "{} / {}: calls differ\n  expected: {:?}\n  actual:   {:?}",
                    scenario.name,
                    event.action,
                    event.calls.iter().map(|c| &c.command).collect::<Vec<_>>(),
                    actual_calls.iter().map(|c| &c.command).collect::<Vec<_>>(),
                );
            }

            let (expected_state, actual_state) = match scenario.panel.as_str() {
                "jobs" => {
                    let expected: jobs_view::JobsUiState =
                        serde_json::from_value(event.state.clone()).expect("typed jobs state");
                    (
                        serde_json::to_value(&expected).expect("serialize expected"),
                        serde_json::to_value(&jobs_state).expect("serialize actual"),
                    )
                }
                _ => {
                    let expected: datasets_view::DatasetsUiState =
                        serde_json::from_value(event.state.clone()).expect("typed datasets state");
                    (
                        serde_json::to_value(&expected).expect("serialize expected"),
                        serde_json::to_value(&datasets_state).expect("serialize actual"),
                    )
                }
            };
            if expected_state != actual_state {
                differences += 1;
                eprintln!(
                    "{} / {}: state differs\n  expected: {:?}\n  actual:   {:?}",
                    scenario.name, event.action, expected_state, actual_state,
                );
            }
        }
    }

    println!("jobs_datasets: {compared} inputs compared, {differences} differences");
    assert_eq!(differences, 0);
}

#[test]
fn recording_backend_logs_trait_calls_in_order() {
    let backend = RecordingBackend::default();
    let _ = backend.list_assets();
    let _ = backend.cancel();
    let calls = backend.take_calls();
    assert_eq!(
        calls
            .iter()
            .map(|call| call.command.as_str())
            .collect::<Vec<_>>(),
        vec!["assets_list", "pipeline_cancel"]
    );
}
