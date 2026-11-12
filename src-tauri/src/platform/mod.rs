//! Everything that differs by operating system, behind the same functions on each,
//! so the rest of the app never asks which one it's on:
//!
//! - `can_recycle`, `background`, `tie_to_app`, `lower_this_thread`: files and processes.
//! - `setup`, `notify`, `frame_state`, `set_maximize_button`, `system_menu`: the window.
//! - `open_video`, `read_sound`, `plain_h264`: decoding the patented formats (see `crate::decode`).
//!
//! `unix` covers macOS and Linux (where the tests run) until macOS gets its own.

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use windows::*;

#[cfg(not(windows))]
mod unix;
#[cfg(not(windows))]
pub use unix::*;
