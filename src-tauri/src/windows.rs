//! Windows notifications.
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

use std::path::PathBuf;
use tauri_winrt_notification::{IconCrop, Toast};
use winreg::enums::HKEY_CURRENT_USER;
use winreg::RegKey;

const LOGO: &[u8] = include_bytes!("../icons/notification-logo.png");
const BLANK: &[u8] = include_bytes!("../icons/notification-blank.png");
/// The app's ID before it became com.krysion.sastra.compressor; only test copies used it.
const OLD_ID: &str = "dev.sastra.compressor";

fn data_dir(app_id: &str) -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|d| PathBuf::from(d).join(app_id))
}

pub fn register_for_notifications(app_id: &str) {
    let _ = try_register(app_id);
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
pub fn notify(app_id: &str, title: &str, body: &str) -> Result<(), String> {
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
