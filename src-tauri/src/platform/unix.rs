//! macOS and Linux. The system draws the title bar and the Trash takes files from
//! any drive, so most of these do nothing. Decoding the patented formats isn't
//! supported here yet: on macOS it will go through AVFoundation.

use crate::decode::{Failure, Frames};
use std::path::Path;
use std::process::{Child, Command};
use std::sync::atomic::AtomicBool;
use tauri::AppHandle;

/// The Trash takes files from any drive.
pub fn can_recycle(_path: &Path) -> bool {
    true
}

pub fn background(_cmd: &mut Command) {}

pub fn tie_to_app(_child: &Child) {}

pub fn lower_this_thread() {}

pub fn setup(_app: &tauri::App) {}

pub fn notify(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification().builder().title(title).body(body).show().map_err(|e| e.to_string())
}

/// The system draws the title bar.
pub fn frame_state() -> Option<serde_json::Value> {
    None
}

pub fn set_maximize_button(_right: f64, _width: f64, _height: f64) {}

pub fn system_menu(_app: &AppHandle, _from_keyboard: bool) {}

pub fn open_video(_path: &Path) -> Result<Box<dyn Frames>, Failure> {
    Err(Failure::NoDecoder)
}

pub fn read_sound(_path: &Path, _wav: &Path, _cancel: &AtomicBool) -> Result<bool, Failure> {
    Err(Failure::NoDecoder)
}

pub fn plain_h264(_path: &Path) -> Option<bool> {
    None
}
