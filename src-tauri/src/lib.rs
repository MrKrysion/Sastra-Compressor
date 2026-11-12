mod decode;
mod ffmpeg;
mod formats;
mod image_job;
mod platform;
mod runner;
mod scan;
mod settings;
#[cfg(test)]
mod tests;

use settings::{Job, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Manager, State};

#[derive(Default)]
struct Run {
    busy: Arc<AtomicBool>,
    cancel: Arc<AtomicBool>,
}

#[tauri::command]
fn formats() -> formats::Formats {
    formats::all()
}

/// Expands dropped or picked files and folders into the list to compress.
#[tauri::command]
async fn scan(paths: Vec<String>) -> scan::ScanResult {
    tauri::async_runtime::spawn_blocking(move || scan::scan(paths)).await.unwrap_or_default()
}

/// Where each file will be saved, in the same order, for showing before compressing.
#[tauri::command]
async fn plan(jobs: Vec<Job>, settings: Settings) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        settings::plan(&jobs, &settings).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
fn compress(app: AppHandle, run: State<Run>, jobs: Vec<Job>, settings: Settings) -> Result<(), String> {
    if run.busy.swap(true, Ordering::SeqCst) {
        return Err("Already compressing.".into());
    }
    run.cancel.store(false, Ordering::SeqCst);
    let (cancel, busy) = (run.cancel.clone(), run.busy.clone());
    let journal = journal(&app);
    std::thread::spawn(move || runner::run(app, jobs, settings, cancel, busy, journal));
    Ok(())
}

#[tauri::command]
fn cancel(run: State<Run>) {
    run.cancel.store(true, Ordering::SeqCst);
}

/// Files already on disk that compressing these jobs would overwrite.
#[tauri::command]
async fn existing(jobs: Vec<Job>, settings: Settings) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        settings::existing(&jobs, &settings).into_iter().map(|p| p.to_string_lossy().into_owned()).collect()
    })
    .await
    .unwrap_or_default()
}

/// Which of these files Windows wouldn't put in the Recycle Bin (USB and network drives, mostly).
#[tauri::command]
async fn no_bin(paths: Vec<String>) -> Vec<String> {
    tauri::async_runtime::spawn_blocking(move || {
        paths.into_iter().filter(|p| !runner::can_recycle(std::path::Path::new(p))).collect()
    })
    .await
    .unwrap_or_default()
}

/// Closes at once for the person, then tidies up out of sight: the running files are
/// stopped and their temporary copies removed before the app really exits.
#[tauri::command]
fn quit(app: AppHandle, run: State<Run>) {
    if let Some(w) = app.get_webview_window("main") {
        let _ = w.hide();
    }
    run.cancel.store(true, Ordering::SeqCst);
    let busy = run.busy.clone();
    std::thread::spawn(move || {
        // A large image can't be interrupted, but none takes anything like this long.
        for _ in 0..300 {
            if !busy.load(Ordering::SeqCst) {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        app.exit(0);
    });
}

/// How the window looks right now, for the title bar. None where the system draws it.
#[tauri::command]
fn frame_state() -> Option<serde_json::Value> {
    platform::frame_state()
}

/// Where the title bar's maximise button is, in CSS pixels from the window's right edge.
#[tauri::command]
fn set_maximize_button(right: f64, width: f64, height: f64) {
    platform::set_maximize_button(right, width, height);
}

/// Opens the window's system menu, as right-clicking a title bar or pressing Alt+Space does.
#[tauri::command]
fn system_menu(app: AppHandle, from_keyboard: bool) {
    platform::system_menu(&app, from_keyboard);
}

/// Lists a run's temporary files; see `runner::sweep`.
fn journal(app: &AppHandle) -> Option<std::path::PathBuf> {
    let dir = app.path().app_local_data_dir().ok()?;
    std::fs::create_dir_all(&dir).ok()?;
    Some(dir.join("running.txt"))
}

/// A notification when compressing is done. On Windows the app sends it itself, with
/// the logo as its picture; elsewhere the notification plugin does.
#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    platform::notify(&app, &title, &body)
}

#[tauri::command]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn run() {
    tauri::Builder::default()
        // First, so a second copy hands over and exits before doing anything else.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.show();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Run::default())
        .setup(|app| {
            platform::setup(app);
            if let Some(j) = journal(app.handle()) {
                std::thread::spawn(move || runner::sweep(&j));
            }
            // The interface shows the window once it has drawn; if it never does, show it anyway.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(w) = handle.get_webview_window("main") {
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![formats, scan, plan, compress, cancel, existing, no_bin, quit, frame_state, set_maximize_button, system_menu, notify, version])
        .run(tauri::generate_context!())
        .expect("error while running Sastra Compressor");
}
