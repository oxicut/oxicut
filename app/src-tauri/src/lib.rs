//! The OxiCut desktop app: Tauri commands over the engine.
//!
//! Everything with real logic lives in `engine`, `batch`, `models` and
//! `settings`, where it can be tested without a window; the functions here only
//! move data between the screen and those modules.

mod batch;
mod engine;
mod models;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::State;
use tauri::ipc::Channel;

use batch::{DetectEvent, ExportEvent, ExportItem, ExportOptions, ExportSummary, FlagDto, IdBox};
use engine::{DetectSettings, ScanAnalysis};
use models::{DownloadEvent, ModelStatusDto};
use settings::AppSettings;

struct AppState {
    /// Set by `cancel_detection`; read between scans.
    cancel: Arc<AtomicBool>,
    cache_dir: PathBuf,
}

fn to_paths(paths: Vec<String>) -> Vec<PathBuf> {
    paths.into_iter().map(PathBuf::from).collect()
}

fn to_strings(paths: Vec<PathBuf>) -> Vec<String> {
    paths
        .into_iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
}

/// Scans named on the command line (`oxicut-app.exe scan.tif folder\`), so the
/// app can be started on a batch, or used as an "open with" target.
#[tauri::command]
fn startup_paths() -> Vec<String> {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    to_strings(engine::expand_paths(&args))
}

/// Files and folders (from a dialog or a drop) as a flat list of scans.
#[tauri::command]
fn expand_paths(paths: Vec<String>) -> Vec<String> {
    to_strings(engine::expand_paths(&to_paths(paths)))
}

/// Detect many scans in the background, reporting each through `on_event`.
#[tauri::command]
async fn detect_scans(
    paths: Vec<String>,
    settings: DetectSettings,
    on_event: Channel<DetectEvent>,
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.cancel.store(false, Ordering::SeqCst);
    let (cancel, cache) = (state.cancel.clone(), state.cache_dir.clone());
    tauri::async_runtime::spawn_blocking(move || {
        batch::detect_batch(&to_paths(paths), &settings, &cache, &cancel, &|event| {
            let _ = on_event.send(event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Stop scans that have not started yet; the ones running finish.
#[tauri::command]
fn cancel_detection(state: State<'_, AppState>) {
    state.cancel.store(true, Ordering::SeqCst);
}

/// Detect again on a single scan (after changing settings).
#[tauri::command]
async fn detect_one(
    path: String,
    settings: DetectSettings,
    state: State<'_, AppState>,
) -> Result<ScanAnalysis, String> {
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine::analyze_scan(&PathBuf::from(path), &settings, &cache)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// The screen-sized preview of a scan, as a data URL.
#[tauri::command]
async fn get_preview(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || engine::load_preview(&PathBuf::from(path), &cache))
        .await
        .map_err(|e| e.to_string())?
}

/// Review warnings for a scan's boxes.
#[tauri::command]
fn evaluate_flags(boxes: Vec<IdBox>, width: u32, height: u32) -> Vec<FlagDto> {
    batch::flags_for(&boxes, width, height)
}

/// Crop and save the approved scans.
#[tauri::command]
async fn export_scans(
    items: Vec<ExportItem>,
    options: ExportOptions,
    on_event: Channel<ExportEvent>,
) -> Result<ExportSummary, String> {
    tauri::async_runtime::spawn_blocking(move || {
        batch::export_batch(&items, &options, &|event| {
            let _ = on_event.send(event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn load_settings() -> AppSettings {
    settings::load()
}

#[tauri::command]
fn save_settings(settings: AppSettings) -> Result<(), String> {
    settings::save(&settings)
}

#[tauri::command]
fn models_status() -> Result<Vec<ModelStatusDto>, String> {
    models::statuses(&oxicut_models::models_dir().map_err(|e| e.to_string())?)
}

/// The OxiCut version, for the status bar.
#[tauri::command]
fn app_version() -> &'static str {
    oxicut_core::VERSION
}

/// Where downloaded models live, for display.
#[tauri::command]
fn models_folder() -> Result<String, String> {
    Ok(oxicut_models::models_dir()
        .map_err(|e| e.to_string())?
        .to_string_lossy()
        .into_owned())
}

#[tauri::command]
async fn download_models(ids: Vec<String>, on_event: Channel<DownloadEvent>) -> Result<(), String> {
    let dir = oxicut_models::models_dir().map_err(|e| e.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        models::download(&dir, &ids, &|event| {
            let _ = on_event.send(event);
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            cancel: Arc::new(AtomicBool::new(false)),
            cache_dir: settings::cache_dir(),
        })
        .invoke_handler(tauri::generate_handler![
            startup_paths,
            expand_paths,
            detect_scans,
            cancel_detection,
            detect_one,
            get_preview,
            evaluate_flags,
            export_scans,
            load_settings,
            save_settings,
            models_status,
            models_folder,
            app_version,
            download_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the OxiCut app");
}
