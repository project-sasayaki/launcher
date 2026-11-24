//! The loaders that run the games, and where to find them.
//!
//! - [`kitten`]: Lindbergh ELF games (`kitten.exe`)
//! - [`mice`]: RingEdge games (`mice.exe` / `mice64.exe`), also the picodaemon card reader
//! - [`segatools`]: ALLS / Nu games (`inject.exe` with a hook dll)
//!
//! Each module writes its loader's config files from the launcher's settings; this module
//! finds the loader executables, either set by hand in `data\settings.json` or found next
//! to the launcher.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use rust_i18n::t;

use crate::storage::{GameRecord, Loader, Settings, launcher_dir};
use crate::win_util::CREATE_NO_WINDOW;

pub mod kitten;
pub mod mice;
pub mod render_util;
pub mod segatools;

/// The loader executables found next to the launcher.
#[derive(Clone, Debug, Default)]
pub struct LoaderPaths {
    pub kitten: Option<PathBuf>,
    pub mice: Option<PathBuf>,
    pub mice64: Option<PathBuf>,
    /// segatools' `inject.exe`.
    pub inject: Option<PathBuf>,
}

const LOADER_NAMES: &[&str] = &["kitten.exe", "mice.exe", "mice64.exe", "inject.exe"];

/// Looks for the loaders in the launcher's folder, down to two levels of subfolders.
pub fn detect_loaders() -> LoaderPaths {
    let mut found = LoaderPaths::default();
    scan_dir(&launcher_dir(), 0, &mut found);
    found
}

fn scan_dir(dir: &Path, depth: u32, found: &mut LoaderPaths) {
    if depth > 2 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        if path.is_dir() {
            scan_dir(&path, depth + 1, found);
            continue;
        }
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let lower = name.to_lowercase();
        for cand in LOADER_NAMES {
            if lower != *cand {
                continue;
            }
            let slot = match *cand {
                "kitten.exe" => &mut found.kitten,
                "mice.exe" => &mut found.mice,
                "inject.exe" => &mut found.inject,
                _ => &mut found.mice64,
            };
            if slot.is_none() {
                *slot = Some(path.clone());
            }
        }
    }
}

/// The loader executable for a game: the path set in the settings, or else the one found
/// next to the launcher. For mice, a 64-bit game prefers mice64 and a 32-bit one mice.
pub fn resolve_loader(
    record: &GameRecord,
    settings: &Settings,
    detected: &LoaderPaths,
) -> Result<PathBuf, String> {
    let pick = |custom: &Option<String>, auto: &Option<PathBuf>| -> Option<PathBuf> {
        custom.as_ref().map(PathBuf::from).or_else(|| auto.clone())
    };
    let exists = |p: &Path| p.is_file();

    match record.loader {
        Loader::Kitten => {
            let p = pick(&settings.kitten_path, &detected.kitten)
                .ok_or_else(|| t!("launch.kitten_exe_not_found").to_string())?;
            if !exists(&p) {
                return Err(t!("launch.kitten_loader_does_not_exist", path = p.display()).to_string());
            }
            Ok(p)
        }
        Loader::Mice => {
            let is_x64 = record.arch.as_deref() == Some("x64");
            let preferred = if is_x64 {
                pick(&settings.mice64_path, &detected.mice64)
                    .or_else(|| pick(&settings.mice_path, &detected.mice))
            } else {
                pick(&settings.mice_path, &detected.mice)
                    .or_else(|| pick(&settings.mice64_path, &detected.mice64))
            };
            let p = preferred.ok_or_else(|| t!("launch.mice_exe_mice64_exe_not").to_string())?;
            if !exists(&p) {
                return Err(t!("launch.mice_loader_does_not_exist", path = p.display()).to_string());
            }
            Ok(p)
        }
        Loader::Segatools => {
            let p = pick(&settings.segatools_path, &detected.inject)
                .ok_or_else(|| t!("launch.inject_exe_not_found").to_string())?;
            if !exists(&p) {
                return Err(t!("launch.inject_exe_does_not_exist", path = p.display()).to_string());
            }
            Ok(p)
        }
    }
}

/// Starts kitten or mice as `<loader> -b <file>` in the folder of the record's file (with
/// `-t` in test mode), without a console window.
pub fn start_game(record: &GameRecord, loader: &Path) -> Result<Child, String> {
    let game = PathBuf::from(&record.path);
    if !game.is_file() {
        return Err(t!("common.game_file_does_not_exist", game = game.display()).to_string());
    }
    let game_dir = game.parent().map(Path::to_path_buf).unwrap_or_else(launcher_dir);
    if !game_dir.is_dir() {
        return Err(t!("common.game_directory_does_not_exist", game_dir = game_dir.display()).to_string());
    }
    // Only the file name: kitten splits a full path at its spaces.
    let file = game.file_name().map(PathBuf::from).unwrap_or_else(|| game.clone());

    let mut cmd = Command::new(loader);
    cmd.current_dir(&game_dir).arg("-b").arg(&file).creation_flags(CREATE_NO_WINDOW);
    if record.test_mode == Some(true) {
        cmd.arg("-t");
    }
    cmd.spawn()
        .map_err(|e| t!("launch.failed_to_start_loader", loader = loader.display(), error = e).to_string())
}
