//! The native side of the custom title bar.
//!
//! The frameless window keeps its real Windows styles (Tauri only removes the
//! visible frame), so animations, snapping, shadows, and rounded corners stay.
//! Two things need help, because the webview covers the whole window:
//!
//! - **Snap layouts.** Windows 11 offers them only over something that answers
//!   hit-tests with `HTMAXBUTTON`, and the webview can't. So an invisible child
//!   window sits exactly over the interface's maximise button and answers for
//!   it. The interface draws the button; this reports hover and press to it.
//! - **The system menu.** Right-clicking the bar should open it, as on any window.
//!
//! The interface says where its button is in CSS pixels, measured from the
//! window's right edge. The window keeps it there at every size and scale, without
//! waiting for the interface, so the target never lags behind a resize.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::OnceLock;
use tauri::{AppHandle, Emitter};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{TrackMouseEvent, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT};
use windows_sys::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

const CLASS: &str = "SastraMaximiseButton";
const SUBCLASS_ID: usize = 0x5A57;
/// Not in every header: sent when a click would pass through to a non-client area.
const WM_NCMOUSELEAVE: u32 = 0x02A2;

static APP: OnceLock<AppHandle> = OnceLock::new();
static PARENT: AtomicIsize = AtomicIsize::new(0);
static BUTTON: AtomicIsize = AtomicIsize::new(0);
// Where the interface's maximise button is, in CSS pixels.
static RIGHT: AtomicU32 = AtomicU32::new(0);
static WIDTH: AtomicU32 = AtomicU32::new(0);
static HEIGHT: AtomicU32 = AtomicU32::new(0);
static ACTIVE: AtomicBool = AtomicBool::new(true);
static HOVER: AtomicBool = AtomicBool::new(false);
static PRESSED: AtomicBool = AtomicBool::new(false);
static TRACKING: AtomicBool = AtomicBool::new(false);

/// What the interface draws differently.
#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub struct FrameState {
    pub maximized: bool,
    pub active: bool,
    /// The pointer is over the maximise button, or pressing it.
    pub hover: bool,
    pub pressed: bool,
}

pub fn state() -> FrameState {
    let parent = parent();
    FrameState {
        // SAFETY: IsZoomed accepts any handle, and returns false for an invalid one.
        maximized: unsafe { IsZoomed(parent) } != 0,
        active: ACTIVE.load(Ordering::Relaxed),
        hover: HOVER.load(Ordering::Relaxed),
        pressed: PRESSED.load(Ordering::Relaxed),
    }
}

fn parent() -> HWND {
    PARENT.load(Ordering::Relaxed) as HWND
}

fn tell_interface() {
    if let Some(app) = APP.get() {
        let _ = app.emit("frame", state());
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// Sets up the main window. If this fails, the title bar still works: the interface's
/// own button handles clicks, and only the snap layouts on hover are missing.
pub fn attach(app: AppHandle, hwnd: isize) {
    let _ = APP.set(app);
    PARENT.store(hwnd, Ordering::Relaxed);
    let parent = hwnd as HWND;
    let class = wide(CLASS);
    // SAFETY: called once on the main thread with the app's own window. The class name
    // outlives RegisterClassExW and CreateWindowExW, which copy what they need.
    unsafe {
        let instance = windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());
        let info = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(button_proc),
            hInstance: instance,
            hCursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW),
            lpszClassName: class.as_ptr(),
            ..std::mem::zeroed()
        };
        RegisterClassExW(&info);
        // Layered with no bitmap: there is nothing to draw, so the webview shows through.
        let button = CreateWindowExW(
            WS_EX_LAYERED | WS_EX_NOREDIRECTIONBITMAP,
            class.as_ptr(),
            std::ptr::null(),
            WS_CHILD | WS_CLIPSIBLINGS,
            0,
            0,
            0,
            0,
            parent,
            std::ptr::null_mut(),
            instance,
            std::ptr::null(),
        );
        if !button.is_null() {
            SetLayeredWindowAttributes(button, 0, 255, LWA_ALPHA);
            BUTTON.store(button as isize, Ordering::Relaxed);
        }
        SetWindowSubclass(parent, Some(parent_proc), SUBCLASS_ID, 0);
    }
    place_button();
}

/// The interface reports where its maximise button is, from the window's right edge.
pub fn set_button(right: f64, width: f64, height: f64) {
    let clean = |v: f64| if v.is_finite() { v.clamp(0.0, 4096.0).round() as u32 } else { 0 };
    RIGHT.store(clean(right), Ordering::Relaxed);
    WIDTH.store(clean(width), Ordering::Relaxed);
    HEIGHT.store(clean(height), Ordering::Relaxed);
    place_button();
}

/// Keeps the invisible button over the drawn one, above the webview.
fn place_button() {
    let button = BUTTON.load(Ordering::Relaxed) as HWND;
    if button.is_null() {
        return;
    }
    let parent = parent();
    // SAFETY: both handles belong to this app; these calls fail harmlessly if either is gone.
    unsafe {
        let dpi = GetDpiForWindow(parent).max(96) as i64;
        let scale = |css: u32| ((css as i64 * dpi + 95) / 96) as i32;
        let (width, height) = (scale(WIDTH.load(Ordering::Relaxed)), scale(HEIGHT.load(Ordering::Relaxed)));
        if width == 0 || height == 0 {
            ShowWindow(button, SW_HIDE);
            return;
        }
        let mut client: RECT = std::mem::zeroed();
        GetClientRect(parent, &mut client);
        // The top few pixels resize the window, as on a normal title bar, except when
        // maximised, where the very top of the screen should still hit the button.
        let top = if IsZoomed(parent) != 0 { 0 } else { GetSystemMetricsForDpi(SM_CYFRAME, dpi as u32) };
        let x = client.right - scale(RIGHT.load(Ordering::Relaxed)) - width;
        SetWindowPos(button, HWND_TOP, x, top, width, (height - top).max(1), SWP_NOACTIVATE | SWP_SHOWWINDOW | SWP_NOSENDCHANGING);
    }
}

fn set_hover(hover: bool, pressed: bool) {
    let was = (HOVER.swap(hover, Ordering::Relaxed), PRESSED.swap(pressed, Ordering::Relaxed));
    if was != (hover, pressed) {
        tell_interface();
    }
}

fn toggle_maximize() {
    let parent = parent();
    // SAFETY: posting to the app's own window; WM_SYSCOMMAND takes the usual route, so
    // the system's animation plays and the interface's other windows are told.
    unsafe {
        let command = if IsZoomed(parent) != 0 { SC_RESTORE } else { SC_MAXIMIZE };
        PostMessageW(parent, WM_SYSCOMMAND, command as WPARAM, 0);
    }
}

/// Opens the system menu at the pointer, or at the bar's corner if it came from the
/// keyboard. Must run on the main thread.
pub fn show_system_menu(from_keyboard: bool) {
    let parent = parent();
    // SAFETY: the app's own window on its own thread. The menu belongs to the window
    // and isn't destroyed here.
    unsafe {
        let mut at = POINT { x: 0, y: 0 };
        if from_keyboard {
            let mut window: RECT = std::mem::zeroed();
            GetWindowRect(parent, &mut window);
            let dpi = GetDpiForWindow(parent).max(96) as i32;
            at = POINT { x: window.left + 8 * dpi / 96, y: window.top + 56 * dpi / 96 };
        } else {
            GetCursorPos(&mut at);
        }
        let menu = GetSystemMenu(parent, 0);
        if menu.is_null() {
            return;
        }
        // Windows only keeps these up to date when it opens the menu itself.
        let maximized = IsZoomed(parent) != 0;
        let enable = |item: u32, on: bool| {
            EnableMenuItem(menu, item, MF_BYCOMMAND | if on { MF_ENABLED } else { MF_GRAYED });
        };
        enable(SC_RESTORE, maximized);
        enable(SC_MOVE, !maximized);
        enable(SC_SIZE, !maximized);
        enable(SC_MINIMIZE, true);
        enable(SC_MAXIMIZE, !maximized);
        enable(SC_CLOSE, true);
        SetMenuDefaultItem(menu, if maximized { SC_RESTORE } else { SC_MAXIMIZE }, 0);
        SetForegroundWindow(parent);
        let chosen = TrackPopupMenu(menu, TPM_RETURNCMD | TPM_LEFTBUTTON | TPM_RIGHTBUTTON, at.x, at.y, 0, parent, std::ptr::null());
        if chosen != 0 {
            PostMessageW(parent, WM_SYSCOMMAND, chosen as WPARAM, 0);
        }
    }
}

/// Windows places the snap layouts menu under the rectangle it's given here. Left alone
/// it assumes a standard, shorter title bar, and the menu opens on top of the button.
/// Reporting the real one, which runs to the bar's bottom, opens it clear of the button.
unsafe fn report_button(lparam: LPARAM) {
    const MAXIMIZE_BUTTON: usize = 3;
    let button = BUTTON.load(Ordering::Relaxed) as HWND;
    if button.is_null() || lparam == 0 {
        return;
    }
    let mut rect: RECT = std::mem::zeroed();
    if GetWindowRect(button, &mut rect) != 0 {
        (*(lparam as *mut TITLEBARINFOEX)).rgrect[MAXIMIZE_BUTTON] = rect;
    }
}

unsafe extern "system" fn button_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        // The whole point: Windows now treats this as the window's maximise button.
        WM_NCHITTEST => HTMAXBUTTON as LRESULT,
        WM_NCMOUSEMOVE => {
            if !TRACKING.swap(true, Ordering::Relaxed) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                TrackMouseEvent(&mut track);
            }
            set_hover(true, PRESSED.load(Ordering::Relaxed));
            // Left to run by default, so the system's handling of the button still happens.
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_NCMOUSELEAVE => {
            TRACKING.store(false, Ordering::Relaxed);
            set_hover(false, false);
            0
        }
        // Handled here: by default these would act on this child, not on the app's window.
        WM_NCLBUTTONDOWN | WM_NCLBUTTONDBLCLK => {
            set_hover(true, true);
            0
        }
        WM_NCLBUTTONUP => {
            let pressed = PRESSED.load(Ordering::Relaxed);
            set_hover(true, false);
            if pressed {
                toggle_maximize();
            }
            0
        }
        WM_NCRBUTTONDOWN => 0,
        WM_NCRBUTTONUP => {
            show_system_menu(false);
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn parent_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _id: usize, _data: usize) -> LRESULT {
    match msg {
        WM_NCDESTROY => {
            RemoveWindowSubclass(hwnd, Some(parent_proc), SUBCLASS_ID);
            return DefSubclassProc(hwnd, msg, wparam, lparam);
        }
        WM_NCACTIVATE => {
            // wparam says whether the window now looks active, which is what the bar shows.
            if ACTIVE.swap(wparam != 0, Ordering::Relaxed) != (wparam != 0) {
                tell_interface();
            }
        }
        _ => {}
    }
    let result = DefSubclassProc(hwnd, msg, wparam, lparam);
    if msg == WM_GETTITLEBARINFOEX {
        report_button(lparam);
    }
    if msg == WM_SIZE || msg == WM_DPICHANGED {
        if wparam != SIZE_MINIMIZED as WPARAM {
            place_button();
        }
        if msg == WM_SIZE {
            tell_interface();
        }
    }
    result
}
