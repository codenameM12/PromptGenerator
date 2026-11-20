pub mod builtin_presets;
pub mod commands;
pub mod dataset;
pub mod db;
pub mod diagnostics;
pub mod drag_out;
pub mod job_queue;
pub mod media;
pub mod native;
pub mod paths;
pub mod pipeline;
pub mod preset;
pub mod runtime;
pub mod settings;
pub mod title;
pub mod ui;
pub mod workspace;

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use runtime::client::LmStudioClient;
use runtime::lms_cli::LmsCli;
use runtime::manager::ModelManager;

use tauri::Manager;
use tauri_plugin_shell::ShellExt;

#[tauri::command]
async fn ffprobe_version(app: tauri::AppHandle) -> Result<String, String> {
    let output = app
        .shell()
        .sidecar("ffprobe")
        .map_err(|error| error.to_string())?
        .args(["-version"])
        .output()
        .await
        .map_err(|error| error.to_string())?;

    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .unwrap_or_default()
        .to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_window_state::Builder::default().build())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_drag::init())
        .register_uri_scheme_protocol("media", |context, request| {
            media::protocol::handle(context.app_handle(), request)
        })
        .invoke_handler(tauri::generate_handler![
            ffprobe_version,
            commands::assets::assets_import,
            commands::assets::assets_import_clipboard,
            commands::assets::assets_import_cancel,
            commands::assets::assets_list,
            commands::assets::assets_sample_frames,
            commands::edits::edits_save,
            commands::edits::edits_preview,
            commands::edits::edits_cancel,
            commands::edits::edits_export_mask,
            commands::edits::edits_latest,
            commands::models::models_list,
            commands::models::models_load,
            commands::models::models_unload,
            commands::models::models_unload_all,
            commands::models::models_status,
            commands::models::models_set_roles,
            commands::models::models_set_policy,
            commands::presets::presets_list,
            commands::presets::presets_get,
            commands::presets::presets_preview,
            commands::presets::presets_compile,
            commands::presets::presets_save,
            commands::presets::presets_duplicate,
            commands::presets::presets_delete,
            commands::presets::presets_import,
            commands::presets::presets_export,
            commands::pipeline::pipeline_describe,
            commands::pipeline::pipeline_compose,
            commands::pipeline::pipeline_batch,
            commands::pipeline::pipeline_cancel,
            commands::jobs::jobs_list,
            commands::jobs::jobs_get,
            commands::jobs::jobs_status,
            commands::jobs::jobs_enqueue,
            commands::jobs::jobs_pause,
            commands::jobs::jobs_resume,
            commands::jobs::jobs_retry,
            commands::jobs::jobs_cancel,
            commands::jobs::jobs_cancel_waiting,
            commands::jobs::jobs_reorder,
            commands::library::library_query,
            commands::library::library_get,
            commands::library::library_compare,
            commands::library::library_favorite,
            commands::library::library_update,
            commands::library::library_tags,
            commands::library::library_collections,
            commands::library::library_collection_create,
            commands::library::library_collection_rename,
            commands::library::library_collection_delete,
            commands::library::library_collection_assign,
            commands::library::library_versions,
            commands::library::library_diff,
            commands::library::library_restore_workspace,
            commands::library::library_trash,
            commands::library::library_restore_deleted,
            commands::library::library_assets_trash,
            commands::library::library_purge,
            commands::library::library_export,
            commands::library::library_import,
            commands::library::library_rerun,
            commands::library::library_backup,
            commands::library::library_restore,
            commands::datasets::datasets_list,
            commands::datasets::datasets_preview,
            commands::datasets::datasets_import,
            commands::datasets::datasets_rows,
            commands::datasets::datasets_run,
            commands::datasets::datasets_export,
            commands::datasets::datasets_delete,
            commands::datasets::datasets_profiles,
            commands::settings::settings_get,
            commands::settings::settings_update,
            commands::drag::drag_prepare,
            commands::drag::drag_cancel,
            commands::drag::drag_save,
            commands::drag::drag_reveal,
            commands::diagnostics::app_info,
            commands::diagnostics::diagnostics_export,
            commands::status::status_snapshot,
            commands::workspace::workspace_get_draft,
            commands::workspace::workspace_save_draft,
            commands::workspace::workspace_clear_draft,
        ])
        .setup(|app| {
            let exe_dir = std::env::current_exe()
                .ok()
                .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
                .unwrap_or_else(|| std::path::PathBuf::from("."));
            let per_user_data = app.path().app_data_dir().expect("resolve app data dir");
            let per_user_cache = app.path().app_cache_dir().expect("resolve app cache dir");
            let app_paths = paths::resolve_app_paths(
                &exe_dir,
                &per_user_data,
                &per_user_cache,
                paths::portable_requested(&exe_dir),
            )
            .expect("resolve app paths");

            if let Some(reason) = &app_paths.fallback_reason {
                eprintln!("[paths] {reason}");
            }

            let settings_store =
                settings::SettingsStore::load(&app_paths.config_dir).expect("load settings");
            let workspace_store =
                workspace::WorkspaceStore::load(&app_paths.data_dir).expect("load workspace");

            let database_path = app_paths.data_dir.join("prompt-studio.db");
            let mut database = db::Database::open(&database_path).expect("open database");
            database.migrate().expect("migrate database");
            let seeded =
                builtin_presets::seed(database.connection()).expect("seed built-in presets");
            if seeded > 0 {
                println!("[presets] seeded {seeded} built-in preset(s)");
            }

            let settings_snapshot = settings_store.get();
            let token = settings_store.token().map(ToOwned::to_owned);
            let client = LmStudioClient::new(settings_snapshot.endpoint.clone(), token)
                .expect("build LM Studio client");
            let manager = ModelManager::new(
                Arc::new(client),
                LmsCli::detect(),
                commands::models::manager_config(&settings_snapshot),
            );
            let idle_manager = manager.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let _ = idle_manager.unload_idle(Instant::now()).await;
                }
            });

            let media_access = media::protocol::MediaAccess::new(app_paths.cache_dir.clone());
            if let Ok(assets) = db::repository::assets::list(database.connection(), 100_000, 0) {
                for asset in assets {
                    media_access.allow_path(std::path::Path::new(&asset.source_path));
                }
            }

            app.manage(media::ffmpeg::MediaTools::resolve().expect("resolve ffmpeg/ffprobe"));
            drag_out::cleanup(&app_paths.cache_dir, std::time::SystemTime::now());
            app.manage(media::import::ImportControl::default());
            app.manage(commands::edits::EditControl::default());
            app.manage(commands::pipeline::PipelineControl::default());
            app.manage(commands::drag::DragControl::default());
            app.manage(manager);
            app.manage(media_access);
            app.manage(app_paths);
            app.manage(Mutex::new(settings_store));
            app.manage(Mutex::new(workspace_store));
            app.manage(Mutex::new(database));

            let queue = Arc::new(job_queue::JobQueue::default());
            app.manage(Arc::clone(&queue));
            if let Ok(interrupted) = job_queue::recover(app.state::<Mutex<db::Database>>().inner())
                && interrupted > 0
            {
                println!("[jobs] marked {interrupted} interrupted job(s) for resume");
            }
            queue.start(app.handle().clone());

            let report = match tauri::async_runtime::block_on(ffprobe_version(app.handle().clone()))
            {
                Ok(version) => format!("ok: {version}"),
                Err(error) => format!("error: {error}"),
            };
            println!("[sidecar] {report}");
            let _ = std::fs::write(
                std::env::temp_dir().join("prompt-studio-sidecar-check.txt"),
                &report,
            );
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn mock_lm_studio_models_endpoint_works() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/api/v1/models"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "data": [{ "id": "lfm3-vl-2b", "type": "llm", "state": "loaded" }]
            })))
            .mount(&server)
            .await;

        let response = reqwest::get(format!("{}/api/v1/models", server.uri()))
            .await
            .expect("mock request");
        assert!(response.status().is_success());
        let body: serde_json::Value = response.json().await.expect("json body");
        assert_eq!(body["data"][0]["id"], "lfm3-vl-2b");
    }

    #[test]
    fn csp_is_restrictive() {
        let config: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).expect("parse tauri config");
        let csp = config["app"]["security"]["csp"]
            .as_str()
            .expect("csp must be configured");
        assert!(csp.contains("default-src 'self'"));
        assert!(csp.contains("object-src 'none'"));
        assert!(csp.contains("frame-ancestors 'none'"));
        assert!(!csp.contains("unsafe-eval"));

        let dev_csp = config["app"]["security"]["devCsp"]
            .as_str()
            .expect("dev csp must be configured");
        assert!(dev_csp.contains("ws://localhost:1420"));
    }

    #[test]
    fn capability_allowlist_is_minimal() {
        let capabilities: serde_json::Value =
            serde_json::from_str(include_str!("../capabilities/default.json"))
                .expect("parse capabilities");
        let permissions: Vec<&str> = capabilities["permissions"]
            .as_array()
            .expect("permissions array")
            .iter()
            .filter_map(|value| value.as_str())
            .collect();

        assert!(permissions.contains(&"core:default"));
        assert!(!permissions.iter().any(|value| value.starts_with("shell:")));
        assert!(!permissions.iter().any(|value| value.starts_with("fs:")));
    }

    #[test]
    fn bundled_ffprobe_reports_version() {
        let binary = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("binaries")
            .join("ffprobe-x86_64-pc-windows-msvc.exe");
        let output = std::process::Command::new(&binary)
            .arg("-version")
            .output()
            .expect("run bundled ffprobe");
        assert!(output.status.success(), "ffprobe exited with failure");
        let stdout = String::from_utf8_lossy(&output.stdout);
        assert!(
            stdout.starts_with("ffprobe version"),
            "unexpected output: {stdout}"
        );
    }
}
