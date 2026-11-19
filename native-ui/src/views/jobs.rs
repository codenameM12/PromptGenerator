//! Jobs panel. The replay functions translate an interaction descriptor into
//! the ordered backend calls the source panel makes; the egui shell issues the
//! same calls through `Backend::call`.

use egui::Ui;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::backend::Backend;
use crate::errors::error_text_of;
use crate::theme::Theme;
use crate::view_model::ViewModel;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobsUiState {
    pub paused: bool,
    pub job_count: usize,
    pub error: Option<String>,
}

fn refresh(record: &dyn Fn(&str, Value)) {
    record("jobs_list", json!({}));
    record("jobs_status", json!({}));
}

/// Replays one recorded interaction. `record` receives the command name and the
/// JSON arguments in the exact order the TypeScript panel invokes them.
pub fn replay(action: &Value, record: &dyn Fn(&str, Value), state: &mut JobsUiState) {
    let kind = action.get("kind").and_then(Value::as_str).unwrap_or("");
    match kind {
        "mount" => {
            refresh(record);
            state.job_count = 1;
        }
        "pause" => {
            record("jobs_pause", json!({}));
            refresh(record);
        }
        "resume" => {
            record("jobs_resume", json!({}));
            refresh(record);
        }
        "cancelWaiting" => {
            record("jobs_cancel_waiting", json!({}));
            refresh(record);
        }
        "reorder" => {
            let direction = action
                .get("direction")
                .and_then(Value::as_str)
                .unwrap_or("up");
            record(
                "jobs_reorder",
                json!({ "request": { "id": "job-1", "direction": direction } }),
            );
            refresh(record);
        }
        "cancel" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("job-1");
            record("jobs_cancel", json!({ "id": id }));
            refresh(record);
        }
        "retry" => {
            let id = action.get("id").and_then(Value::as_str).unwrap_or("job-1");
            record("jobs_retry", json!({ "id": id }));
            refresh(record);
        }
        "noop" => {}
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Live egui shell
// ---------------------------------------------------------------------------

pub fn backend_sink(
    backend: &dyn Backend,
) -> impl FnMut(&str, Value) -> Result<Value, String> + '_ {
    move |method: &str, args: Value| backend.call(method, args).map_err(|e| error_text_of(&e))
}

fn live_refresh(sink: &mut impl FnMut(&str, Value) -> Result<Value, String>, vm: &mut ViewModel) {
    vm.jobs.error = None;
    match sink("jobs_list", json!({})) {
        Ok(value) => {
            vm.jobs.jobs = serde_json::from_value(value).unwrap_or_default();
        }
        Err(error) => vm.jobs.error = Some(error),
    }
    match sink("jobs_status", json!({})) {
        Ok(value) => {
            vm.jobs.paused = value
                .get("paused")
                .and_then(Value::as_bool)
                .unwrap_or(false);
            vm.jobs.queue_status = Some(value);
        }
        Err(error) => vm.jobs.error = Some(error),
    }
}

fn live_action(
    sink: &mut impl FnMut(&str, Value) -> Result<Value, String>,
    vm: &mut ViewModel,
    command: &str,
    args: Value,
) {
    vm.jobs.error = None;
    match sink(command, args) {
        Ok(_) => live_refresh(sink, vm),
        Err(error) => vm.jobs.error = Some(error),
    }
}

pub fn ui(ui: &mut Ui, vm: &mut ViewModel, theme: &Theme, backend: &dyn Backend) {
    let mut sink = backend_sink(backend);
    if !vm.jobs.loaded {
        vm.jobs.loaded = true;
        live_refresh(&mut sink, vm);
    }

    ui.heading("Job queue");
    ui.add_space(theme.space_xs);
    ui.horizontal(|ui| {
        let pause_label = if vm.jobs.paused { "Resume" } else { "Pause" };
        if ui.button(pause_label).clicked() {
            let command = if vm.jobs.paused {
                "jobs_resume"
            } else {
                "jobs_pause"
            };
            live_action(&mut sink, vm, command, json!({}));
        }
        if ui.button("Cancel waiting").clicked() {
            live_action(&mut sink, vm, "jobs_cancel_waiting", json!({}));
        }
        if ui.button("Refresh").clicked() {
            live_refresh(&mut sink, vm);
        }
        ui.label(
            egui::RichText::new(if vm.jobs.paused {
                "Queue paused"
            } else {
                "Queue active"
            })
            .color(if vm.jobs.paused {
                theme.warning
            } else {
                theme.success
            }),
        );
    });

    let jobs: Vec<(String, String, String, i64, i64)> = vm
        .jobs
        .jobs
        .iter()
        .map(|job| {
            (
                job.get("id")
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string(),
                job.get("note")
                    .or_else(|| job.get("kind"))
                    .and_then(Value::as_str)
                    .unwrap_or("job")
                    .to_string(),
                job.get("status")
                    .and_then(Value::as_str)
                    .unwrap_or("unknown")
                    .to_string(),
                job.get("completedItems")
                    .and_then(Value::as_i64)
                    .unwrap_or(0),
                job.get("totalItems").and_then(Value::as_i64).unwrap_or(0),
            )
        })
        .collect();

    egui::ScrollArea::vertical()
        .id_salt("jobs")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if jobs.is_empty() {
                ui.label(egui::RichText::new("No jobs yet").color(theme.muted));
            }
            for (id, label, status, done, total) in jobs {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        egui::RichText::new(format!("{label} · {status} {done}/{total}"))
                            .color(theme.ink),
                    );
                    if ui.button("Move up").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "jobs_reorder",
                            json!({ "request": { "id": id, "direction": "up" } }),
                        );
                    }
                    if ui.button("Move down").clicked() {
                        live_action(
                            &mut sink,
                            vm,
                            "jobs_reorder",
                            json!({ "request": { "id": id, "direction": "down" } }),
                        );
                    }
                    if ui.button("Details").clicked() {
                        vm.jobs.selected_job_id = Some(id.clone());
                        match sink("jobs_get", json!({ "id": id })) {
                            Ok(value) => vm.jobs.detail = Some(value),
                            Err(error) => vm.jobs.error = Some(error),
                        }
                    }
                    if ui.button("Cancel").clicked() {
                        live_action(&mut sink, vm, "jobs_cancel", json!({ "id": id }));
                    }
                    if ui.button("Retry").clicked() {
                        live_action(&mut sink, vm, "jobs_retry", json!({ "id": id }));
                    }
                });
            }

            if let Some(detail) = vm.jobs.detail.clone() {
                ui.add_space(theme.space_sm);
                if let Some(items) = detail.get("items").and_then(Value::as_array) {
                    ui.label(
                        egui::RichText::new(format!("{} item(s)", items.len())).color(theme.muted),
                    );
                    for item in items {
                        let index = item.get("index").and_then(Value::as_i64).unwrap_or(0);
                        let status = item
                            .get("status")
                            .and_then(Value::as_str)
                            .unwrap_or("pending");
                        let error = item.get("error").and_then(Value::as_str).unwrap_or("");
                        let mut line = format!("{index}: {status}");
                        if !error.is_empty() {
                            line.push_str(&format!(" · {error}"));
                        }
                        ui.label(egui::RichText::new(line).color(theme.ink_2));
                        if let Some(references) = item.get("references").and_then(Value::as_array) {
                            for reference in references {
                                let name = reference
                                    .get("name")
                                    .and_then(Value::as_str)
                                    .unwrap_or("reference");
                                let weight = reference
                                    .get("weight")
                                    .and_then(Value::as_f64)
                                    .unwrap_or(1.0);
                                ui.label(
                                    egui::RichText::new(format!("   {name} @{weight}"))
                                        .color(theme.faint),
                                );
                            }
                        }
                    }
                } else {
                    ui.label(egui::RichText::new(detail.to_string()).color(theme.faint));
                }
            }

            if let Some(error) = &vm.jobs.error {
                ui.label(egui::RichText::new(error).color(theme.danger));
            }
        });
}
