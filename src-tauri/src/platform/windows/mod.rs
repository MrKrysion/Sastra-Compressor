//! The Windows side of `platform`: the Recycle Bin check, tying ffmpeg to the
//! app's life, working at a lower priority, notifications, the custom title bar
//! (`frame`), and decoding through Media Foundation (`decode`).
//!
//! Windows only shows notifications from apps it knows, so the app introduces
//! itself on every start: a registry entry under the current user (no
//! administrator rights) with its display name. That name, "Sastra™
//! Compressor", heads each notification instead of the app's internal ID.
//!
//! Windows draws the small header icon as a one-colour symbol, which turned
//! the pink logo green, and with no icon it shows a grid placeholder. So the
//! header gets a fully transparent icon, and the logo is shown instead as the
//! notification's own picture, which keeps its colours.

mod decode;
mod frame;

pub use decode::{open_video, plain_h264, read_sound};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::OnceLock;
use tauri::{AppHandle, Manager};
use tauri_winrt_notification::{IconCrop, Toast};
use windows_sys::Win32::Storage::FileSystem::{GetDriveTypeW, GetVolumeNameForVolumeMountPointW, GetVolumePathNameW};
use windows_sys::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
    JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
};
use windows_sys::Win32::System::Threading::{GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

/// Whether Windows would put this file in the Recycle Bin. When it can't, it deletes
/// the file for good and reports success, which is what happens on a USB stick.
pub fn can_recycle(path: &Path) -> bool {
    const DRIVE_FIXED: u32 = 3;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
    let mut root = [0u16; 1024];
    // SAFETY: both buffers are valid and null-terminated where the API needs it, with the true length passed.
    unsafe {
        if GetVolumePathNameW(wide.as_ptr(), root.as_mut_ptr(), root.len() as u32) == 0 || GetDriveTypeW(root.as_ptr()) != DRIVE_FIXED {
            return false;
        }
    }
    let mut volume = [0u16; 64];
    // SAFETY: `root` was filled in above; `volume` is valid for the length passed.
    let named = unsafe { GetVolumeNameForVolumeMountPointW(root.as_ptr(), volume.as_mut_ptr(), volume.len() as u32) } != 0;
    if !named {
        return true;
    }
    // `\\?\Volume{guid}\`: each drive's Bin settings are stored under that guid.
    let volume = String::from_utf16_lossy(&volume);
    let (Some(start), Some(end)) = (volume.find('{'), volume.find('}')) else { return true };
    let settings = format!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\BitBucket\Volume\{}", &volume[start..=end]);
    let Ok(key) = RegKey::predef(HKEY_CURRENT_USER).open_subkey(settings) else { return true };
    // "Don't move files to the Recycle Bin", chosen in the Bin's properties.
    if key.get_value::<u32, _>("NukeOnDelete").unwrap_or(0) != 0 {
        return false;
    }
    // A file bigger than the Bin's maximum size is deleted for good too.
    let size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    key.get_value::<u32, _>("MaxCapacity").map_or(true, |mb| size <= u64::from(mb) * 1024 * 1024)
}

/// Windows doesn't end a program's child processes with it. Inside this job, ffmpeg
/// dies with the app however the app ends, even in a crash or from Task Manager.
pub fn tie_to_app(child: &Child) {
    static JOB: OnceLock<usize> = OnceLock::new();
    // SAFETY: the job handle lives as long as the app, on purpose: closing it, which Windows
    // does when the app ends, is what ends ffmpeg. The struct is plain data, sized correctly.
    unsafe {
        let job = *JOB.get_or_init(|| {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of_val(&limits) as u32,
            );
            job as usize
        });
        AssignProcessToJobObject(job as _, child.as_raw_handle() as _);
    }
}

/// Runs ffmpeg without a console window, behind anything else on the PC: it takes every core.
pub fn background(cmd: &mut Command) {
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    cmd.creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS);
}

/// So a batch of images leaves room for whatever else is running on the PC.
pub fn lower_this_thread() {
    // SAFETY: GetCurrentThread returns a stand-in handle that is always valid.
    unsafe {
        SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

const LOGO: &[u8] = include_bytes!("../../../icons/notification-logo.png");
const BLANK: &[u8] = include_bytes!("../../../icons/notification-blank.png");
/// The app's ID before it became com.krysion.sastra.compressor; only test copies used it.
const OLD_ID: &str = "dev.sastra.compressor";

fn data_dir(app_id: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(app_id))
}

/// At start: the app introduces itself for notifications and gets its title bar.
pub fn setup(app: &tauri::App) {
    let _ = try_register(&app.config().identifier);
    if let Some(hwnd) = app.get_webview_window("main").and_then(|w| w.hwnd().ok()) {
        frame::attach(app.handle().clone(), hwnd.0 as isize);
    }
}

/// How the window looks right now, for the title bar.
pub fn frame_state() -> Option<serde_json::Value> {
    serde_json::to_value(frame::state()).ok()
}

pub fn set_maximize_button(right: f64, width: f64, height: f64) {
    frame::set_button(right, width, height);
}

pub fn system_menu(app: &AppHandle, from_keyboard: bool) {
    let _ = app.run_on_main_thread(move || frame::show_system_menu(from_keyboard));
}

fn try_register(app_id: &str) -> std::io::Result<()> {
    let classes = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(r"Software\Classes", winreg::enums::KEY_ALL_ACCESS);
    // Tidy up after the old ID: its registration and its folder of settings.
    if app_id != OLD_ID {
        if let Ok(classes) = &classes {
            let _ = classes.delete_subkey_all(format!(r"AppUserModelId\{OLD_ID}"));
        }
        if let Some(old) = data_dir(OLD_ID) {
            let _ = std::fs::remove_dir_all(old);
        }
    }
    let dir = data_dir(app_id).ok_or_else(|| std::io::Error::other("no LOCALAPPDATA"))?;
    std::fs::create_dir_all(&dir)?;
    let blank = dir.join("notification-header.png");
    if !blank.exists() {
        std::fs::write(&blank, BLANK)?;
    }
    let (key, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"Software\Classes\AppUserModelId\{app_id}"))?;
    key.set_value("DisplayName", &"Sastra™ Compressor")?;
    key.set_value("IconUri", &blank.to_string_lossy().into_owned())?;
    Ok(())
}

/// Shows a notification with the logo as its picture.
pub fn notify(app: &AppHandle, title: &str, body: &str) -> Result<(), String> {
    let app_id = app.config().identifier.as_str();
    let dir = data_dir(app_id).ok_or("no LOCALAPPDATA")?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let logo = dir.join("notification-logo.png");
    if std::fs::metadata(&logo).map(|m| m.len() as usize != LOGO.len()).unwrap_or(true) {
        std::fs::write(&logo, LOGO).map_err(|e| e.to_string())?;
    }
    Toast::new(app_id)
        .icon(&logo, IconCrop::Square, "")
        .title(title)
        .text1(body)
        .show()
        .map_err(|e| format!("{e:?}"))
}
