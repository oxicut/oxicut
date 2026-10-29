//! The OxiCut desktop app: Tauri commands over the engine.
//!
//! Everything with real logic lives in `engine`, `batch`, `models` and
//! `settings`, where it can be tested without a window; the functions here only
//! move data between the screen and those modules.

mod batch;
mod engine;
mod models;
mod projects;
mod settings;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tauri::ipc::Channel;
use tauri::{Emitter, Manager, State};

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

/// What the command line asked for: a project to open and scans to add to it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct StartupArgs {
    project: Option<String>,
    scans: Vec<String>,
}

fn startup_from(args: &[PathBuf]) -> StartupArgs {
    let (project, rest) = projects::split_startup(args);
    StartupArgs {
        project: project.map(|p| p.to_string_lossy().into_owned()),
        scans: to_strings(engine::expand_paths(&rest)),
    }
}

/// Arguments of this launch (`oxicut-app.exe project.oxicut scan.tif folder\`): double-clicking a
/// project, "open with", or starting the app on a batch of scans.
#[tauri::command]
fn startup_args() -> StartupArgs {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    startup_from(&args)
}

/// The folder new projects go in (`Documents\OxiCut`).
#[tauri::command]
fn projects_dir() -> String {
    projects::default_dir().to_string_lossy().into_owned()
}

/// Make an empty project called `name` in the projects folder; returns its path.
#[tauri::command(async)]
fn project_new(name: String) -> Result<String, String> {
    projects::create(&projects::default_dir(), &name).map(|p| p.to_string_lossy().into_owned())
}

#[tauri::command(async)]
fn project_open(path: String) -> Result<projects::OpenedProject, String> {
    projects::open(&PathBuf::from(path))
}

#[tauri::command(async)]
fn project_save(path: String, mut project: projects::Project) -> Result<(), String> {
    projects::save(&PathBuf::from(path), &mut project)
}

/// Rename a project file; returns the new path.
#[tauri::command(async)]
fn project_rename(path: String, name: String) -> Result<String, String> {
    projects::rename(&PathBuf::from(path), &name).map(|p| p.to_string_lossy().into_owned())
}

/// Whether each of these files exists, in the same order (for the recent-projects list).
#[tauri::command(async)]
fn files_exist(paths: Vec<String>) -> Vec<bool> {
    paths.iter().map(|p| PathBuf::from(p).is_file()).collect()
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

/// The queue thumbnail of a scan, as a data URL (rebuilt when the cache lost it).
#[tauri::command]
async fn get_thumbnail(path: String, state: State<'_, AppState>) -> Result<String, String> {
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || engine::load_thumbnail(&PathBuf::from(path), &cache))
        .await
        .map_err(|e| e.to_string())?
}

/// The straightened crops of a scan's boxes as small pictures, for the Crops view.
#[tauri::command]
async fn get_crops(
    path: String,
    scan_width: u32,
    boxes: Vec<engine::BoxDto>,
    side: u32,
    state: State<'_, AppState>,
) -> Result<Vec<engine::CropImage>, String> {
    let cache = state.cache_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        engine::render_crops(&PathBuf::from(path), scan_width, &boxes, side, &cache)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Cut one box for real, with the export settings, into a scratch folder, for the lightbox.
#[tauri::command]
async fn final_crop(
    path: String,
    region: engine::BoxDto,
    number: usize,
    options: ExportOptions,
    state: State<'_, AppState>,
) -> Result<batch::FinalCrop, String> {
    let scratch = state.cache_dir.join("final-crops");
    tauri::async_runtime::spawn_blocking(move || {
        batch::final_crop(&PathBuf::from(path), &region, number, &options, &scratch)
    })
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

/// The project's web address, for the About window.
#[tauri::command]
fn app_repository() -> &'static str {
    oxicut_core::REPOSITORY
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
        // First, so a second launch (double-clicking another project) hands its arguments to the
        // window that is already open instead of starting a second copy.
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            let args: Vec<PathBuf> = args.into_iter().skip(1).map(PathBuf::from).collect();
            let _ = app.emit("second-instance", startup_from(&args));
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            cancel: Arc::new(AtomicBool::new(false)),
            cache_dir: settings::cache_dir(),
        })
        .invoke_handler(tauri::generate_handler![
            startup_args,
            projects_dir,
            project_new,
            project_open,
            project_save,
            project_rename,
            files_exist,
            get_thumbnail,
            get_crops,
            final_crop,
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
            app_repository,
            download_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running the OxiCut app");
}
