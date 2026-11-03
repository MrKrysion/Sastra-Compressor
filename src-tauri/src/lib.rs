mod ffmpeg;
mod formats;
mod image_job;
mod runner;
mod scan;
mod settings;
#[cfg(test)]
mod tests;
#[cfg(windows)]
mod windows;

use settings::{Job, Settings};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, State};

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
    std::thread::spawn(move || runner::run(app, jobs, settings, cancel, busy));
    Ok(())
}

#[tauri::command]
fn cancel(run: State<Run>) {
    run.cancel.store(true, Ordering::SeqCst);
}

/// A notification when compressing is done. On Windows the app sends it itself, with
/// the logo as its picture (see windows.rs); elsewhere the notification plugin does.
#[tauri::command]
fn notify(app: AppHandle, title: String, body: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        windows::notify(&app.config().identifier, &title, &body)
    }
    #[cfg(not(windows))]
    {
        use tauri_plugin_notification::NotificationExt;
        app.notification().builder().title(title).body(body).show().map_err(|e| e.to_string())
    }
}

#[tauri::command]
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .manage(Run::default())
        .setup(|app| {
            #[cfg(windows)]
            windows::register_for_notifications(&app.config().identifier);
            // The interface shows the window once it has drawn; if it never does, show it anyway.
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                std::thread::sleep(std::time::Duration::from_secs(3));
                if let Some(w) = tauri::Manager::get_webview_window(&handle, "main") {
                    if !w.is_visible().unwrap_or(true) {
                        let _ = w.show();
                    }
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![formats, scan, plan, compress, cancel, notify, version])
        .run(tauri::generate_context!())
        .expect("error while running Sastra Compressor");
}
