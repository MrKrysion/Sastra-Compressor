//! What the person chose in the interface, and where each file is saved.

use crate::formats::{output_ext, Kind};
use crate::scan::SUFFIX;
use serde::Deserialize;
use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

#[derive(Deserialize, Clone, Copy, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    Smaller,
    Balanced,
    Sharper,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SaveMode {
    /// `name_compressed.ext` next to each original.
    Beside,
    /// The compressed file takes the original's place; the original goes to the Recycle Bin.
    Replace,
    /// Same names and subfolders in another folder.
    Folder,
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub image_quality: Level,
    /// 0 keeps the width.
    pub image_max_width: u32,
    /// Opus bitrate in kbit/s.
    pub audio_bitrate: u32,
    pub video_quality: Level,
    /// 0 keeps the width.
    pub video_max_width: u32,
    pub save_mode: SaveMode,
    /// Used when `save_mode` is `Folder`.
    pub output_folder: Option<String>,
}

impl Settings {
    /// libwebp quality, 0–100 (higher is sharper and larger).
    pub fn webp_quality(&self) -> f32 {
        match self.image_quality {
            Level::Smaller => 50.0,
            Level::Balanced => 75.0,
            Level::Sharper => 90.0,
        }
    }

    /// x264 constant rate factor (lower is sharper and larger).
    pub fn crf(&self) -> u32 {
        match self.video_quality {
            Level::Smaller => 30,
            Level::Balanced => 26,
            Level::Sharper => 22,
        }
    }
}

#[derive(Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct Job {
    pub id: String,
    pub path: String,
    pub rel: String,
    pub kind: Kind,
}

/// Where every job is saved. Two jobs never get the same file (bg.png and
/// bg.jpg would both be bg.webp: the second becomes bg_2.webp), and
/// replacing never overwrites a file that isn't one of the originals.
pub fn plan(jobs: &[Job], s: &Settings) -> Vec<PathBuf> {
    let inputs: HashSet<String> = jobs.iter().map(|j| key(Path::new(&j.path))).collect();
    let mut taken = HashSet::new();
    jobs.iter()
        .map(|job| {
            let want = wanted(job, s);
            let input = Path::new(&job.path);
            let mut n = 1;
            loop {
                let candidate = if n == 1 { want.clone() } else { numbered(&want, n) };
                let k = key(&candidate);
                let is_own_input = k == key(input);
                // When replacing, another file of the same name is only fair game if it is
                // this file's own original; elsewhere this app's earlier output is overwritten.
                let clashes_on_disk = s.save_mode == SaveMode::Replace
                    && !is_own_input
                    && (inputs.contains(&k) || candidate.exists());
                if !taken.contains(&k) && !clashes_on_disk {
                    taken.insert(k);
                    return candidate;
                }
                n += 1;
            }
        })
        .collect()
}

fn wanted(job: &Job, s: &Settings) -> PathBuf {
    let input = Path::new(&job.path);
    let ext = output_ext(job.kind);
    let stem = input.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let beside = input.with_file_name(format!("{stem}{SUFFIX}.{ext}"));
    match (s.save_mode, &s.output_folder) {
        (SaveMode::Replace, _) => input.with_extension(ext),
        (SaveMode::Folder, Some(folder)) => {
            // A path from the interface is never allowed to climb out of the folder.
            let safe: PathBuf = Path::new(&job.rel)
                .with_extension(ext)
                .components()
                .filter(|c| matches!(c, Component::Normal(_)))
                .collect();
            let out = Path::new(folder).join(safe);
            if key(&out) == key(input) {
                beside
            } else {
                out
            }
        }
        _ => beside,
    }
}

fn numbered(p: &Path, n: u32) -> PathBuf {
    let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    p.with_file_name(format!("{stem}_{n}.{ext}"))
}

/// Windows doesn't tell upper and lower case apart in file names.
fn key(p: &Path) -> String {
    let p = std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    let s = p.to_string_lossy().replace('\\', "/");
    if cfg!(windows) { s.to_lowercase() } else { s }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(mode: SaveMode, folder: Option<&str>) -> Settings {
        Settings {
            image_quality: Level::Balanced,
            image_max_width: 0,
            audio_bitrate: 96,
            video_quality: Level::Balanced,
            video_max_width: 0,
            save_mode: mode,
            output_folder: folder.map(String::from),
        }
    }

    fn job(path: &str, rel: &str, kind: Kind) -> Job {
        Job { id: path.into(), path: path.into(), rel: rel.into(), kind }
    }

    #[test]
    fn beside_the_original() {
        let j = [job("/vn/bg/school.png", "school.png", Kind::Image)];
        assert_eq!(plan(&j, &settings(SaveMode::Beside, None)), [PathBuf::from("/vn/bg/school_compressed.webp")]);
    }

    #[test]
    fn in_a_folder_keeps_subfolders() {
        let j = [job("/vn/assets/bg/school.png", "bg/school.png", Kind::Image)];
        assert_eq!(plan(&j, &settings(SaveMode::Folder, Some("/out"))), [PathBuf::from("/out/bg/school.webp")]);
    }

    #[test]
    fn never_climbs_out_of_the_folder() {
        let j = [job("/vn/a.wav", "../../etc/a.wav", Kind::Audio)];
        assert_eq!(plan(&j, &settings(SaveMode::Folder, Some("/out"))), [PathBuf::from("/out/etc/a.ogg")]);
    }

    #[test]
    fn replacing_swaps_the_extension() {
        let j = [job("/vn/bg/school.png", "school.png", Kind::Image), job("/vn/op.mp4", "op.mp4", Kind::Video)];
        assert_eq!(
            plan(&j, &settings(SaveMode::Replace, None)),
            [PathBuf::from("/vn/bg/school.webp"), PathBuf::from("/vn/op.mp4")]
        );
    }

    #[test]
    fn two_files_never_share_an_output() {
        let j = [job("/vn/bg.png", "bg.png", Kind::Image), job("/vn/bg.jpg", "bg.jpg", Kind::Image)];
        assert_eq!(
            plan(&j, &settings(SaveMode::Beside, None)),
            [PathBuf::from("/vn/bg_compressed.webp"), PathBuf::from("/vn/bg_compressed_2.webp")]
        );
        // bg.webp is itself one of the originals, so bg.png can't take its name.
        let j = [job("/vn/bg.png", "bg.png", Kind::Image), job("/vn/bg.webp", "bg.webp", Kind::Image)];
        assert_eq!(
            plan(&j, &settings(SaveMode::Replace, None)),
            [PathBuf::from("/vn/bg_2.webp"), PathBuf::from("/vn/bg.webp")]
        );
    }
}
