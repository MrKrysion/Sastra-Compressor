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
    /// 0 keeps the frame rate.
    #[serde(default)]
    pub video_max_fps: u32,
    pub save_mode: SaveMode,
    /// Used when `save_mode` is `Folder`.
    pub output_folder: Option<String>,
    /// For this run: give new files a number rather than overwrite what's there.
    #[serde(default)]
    pub keep_existing: bool,
    /// For this run: the person agreed that originals with no Recycle Bin to go to are deleted.
    #[serde(default)]
    pub delete_for_good: bool,
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
/// bg.jpg would both be bg.webp: the second becomes bg_2.webp), no job
/// overwrites another job's original, and replacing never overwrites
/// anything else that's already there.
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
                // Another job's original is never fair game. Other files of that name are
                // protected when replacing, or when the person chose to keep them; the
                // interface asks before anything else is overwritten (see `existing`).
                let protect_disk = s.save_mode == SaveMode::Replace || s.keep_existing;
                let clashes_on_disk = !is_own_input && (inputs.contains(&k) || (protect_disk && candidate.exists()));
                if !taken.contains(&k) && !clashes_on_disk {
                    taken.insert(k);
                    return candidate;
                }
                n += 1;
            }
        })
        .collect()
}

/// Files already on disk that this run would overwrite.
pub fn existing(jobs: &[Job], s: &Settings) -> Vec<PathBuf> {
    plan(jobs, s)
        .into_iter()
        .zip(jobs)
        .filter(|(out, job)| out.exists() && key(out) != key(Path::new(&job.path)))
        .map(|(out, _)| out)
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

pub fn numbered(p: &Path, n: u32) -> PathBuf {
    let stem = p.file_stem().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = p.extension().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    p.with_file_name(format!("{stem}_{n}.{ext}"))
}

/// Windows doesn't tell upper and lower case apart in file names.
pub fn key(p: &Path) -> String {
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
            video_max_fps: 0,
            save_mode: mode,
            output_folder: folder.map(String::from),
            keep_existing: false,
            delete_for_good: false,
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

    #[test]
    fn no_mode_overwrites_another_original() {
        let dir = std::env::temp_dir().join(format!("sastra-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (png, webp) = (dir.join("school.png"), dir.join("school.webp"));
        std::fs::write(&png, b"a").unwrap();
        std::fs::write(&webp, b"b").unwrap();
        let folder = dir.to_string_lossy().into_owned();
        let j = [
            job(&png.to_string_lossy(), "school.png", Kind::Image),
            job(&webp.to_string_lossy(), "school.webp", Kind::Image),
        ];
        // Saving to the folder they're already in: school.webp is an original, so it is left alone.
        let out = plan(&j, &settings(SaveMode::Folder, Some(&folder)));
        assert_eq!(out[0], dir.join("school_2.webp"));
        assert_eq!(out[1], dir.join("school_compressed.webp"));
    }

    #[test]
    fn keeping_what_is_there_numbers_the_new_file() {
        let dir = std::env::temp_dir().join(format!("sastra-keep-{}", std::process::id()));
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).unwrap();
        std::fs::write(out_dir.join("cg.webp"), b"mine").unwrap();
        let folder = out_dir.to_string_lossy().into_owned();
        let j = [job(&dir.join("cg.png").to_string_lossy(), "cg.png", Kind::Image)];
        let s = settings(SaveMode::Folder, Some(&folder));
        assert_eq!(existing(&j, &s), [out_dir.join("cg.webp")]);
        let keep = Settings { keep_existing: true, ..s };
        assert_eq!(plan(&j, &keep), [out_dir.join("cg_2.webp")]);
        assert!(existing(&j, &keep).is_empty());
    }
}
