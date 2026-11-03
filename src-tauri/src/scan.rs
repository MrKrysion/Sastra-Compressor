//! Turns dropped or picked paths into a list of files to compress.

use crate::formats::{kind_of, Kind};
use serde::Serialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

/// Marks the files this app writes next to the originals.
pub const SUFFIX: &str = "_compressed";

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Item {
    pub path: String,
    pub name: String,
    /// Path inside the folder it was added from ("bg/school.png"), or the
    /// name alone for a file added by itself. Kept when saving to a folder.
    pub rel: String,
    pub kind: Kind,
    pub size: u64,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ScanResult {
    pub items: Vec<Item>,
    /// Names of files that aren't images, audio, or video we can read.
    pub unsupported: Vec<String>,
}

pub fn scan(paths: Vec<String>) -> ScanResult {
    let mut out = ScanResult::default();
    let mut seen = HashSet::new();
    for p in paths {
        let path = PathBuf::from(&p);
        if path.is_dir() {
            walk(&path, &path, &mut out, &mut seen, 0);
        } else if path.is_file() {
            add(&path, None, &mut out, &mut seen);
        }
    }
    out
}

fn walk(root: &Path, dir: &Path, out: &mut ScanResult, seen: &mut HashSet<PathBuf>, depth: u32) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name().to_ascii_lowercase());
    for e in entries {
        let path = e.path();
        let hidden = e.file_name().to_string_lossy().starts_with('.');
        let Ok(ft) = e.file_type() else { continue };
        if hidden || ft.is_symlink() {
            continue;
        }
        if ft.is_dir() {
            walk(root, &path, out, seen, depth + 1);
        } else if ft.is_file() {
            add(&path, Some(root), out, seen);
        }
    }
}

fn add(path: &Path, root: Option<&Path>, out: &mut ScanResult, seen: &mut HashSet<PathBuf>) {
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    let Some(kind) = kind_of(&ext) else {
        // Ignore system clutter silently; tell about everything else.
        let lower = name.to_ascii_lowercase();
        if !matches!(lower.as_str(), "thumbs.db" | "desktop.ini") {
            out.unsupported.push(name);
        }
        return;
    };
    let canon = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if !seen.insert(canon) {
        return;
    }
    let rel = root
        .and_then(|r| path.strip_prefix(r).ok())
        .map(|r| r.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| name.clone());
    let size = fs::metadata(path).map(|m| m.len()).unwrap_or(0);
    out.items.push(Item { path: path.to_string_lossy().into_owned(), name, rel, kind, size });
}
