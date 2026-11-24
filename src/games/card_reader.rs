//! The picodaemon card reader of Initial D Arcade Stage 4–8.
//!
//! picodaemon.exe runs under mice in the game folder and reads its settings from the game
//! folder's `mice\idas.ini`. Before a launch the launcher puts picodaemon.exe (and, for
//! Lindbergh games, the Cg runtime) into the game folder, copies mice next to it, writes
//! the reader's settings and finally starts it.
//!
//! The Cg runtime is taken from `tools\dependency`, or else from the NVIDIA Cg Toolkit
//! installed on the PC.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use rust_i18n::t;

use crate::detect;
use crate::file_util::{copy_file_if_changed, files_equal};
use crate::loaders::{LoaderPaths, mice};
use crate::storage::{Settings, launcher_dir};

/// A card reader ready to start: picodaemon.exe in the game folder and the mice to run it.
pub struct CardReader {
    pub picodaemon: PathBuf,
    pub mice: PathBuf,
}

/// What the reader's `mice\idas.ini` says.
#[derive(Default)]
pub struct ReaderSettings<'a> {
    /// The card file the reader inserts.
    pub card: Option<&'a str>,
    /// This PC's game address, which the reader reports to the ServerBox.
    pub game_ip: Option<&'a str>,
    /// Insert the card by itself whenever the game asks for one.
    pub auto_insert: bool,
    /// The input backend (`xinput` / `dinput`). Only the Lindbergh games need it here,
    /// because their card button is read by the reader, not by the game.
    pub io3_mode: Option<&'a str>,
    /// The gamepad button that inserts the card (Lindbergh games, see `io3_mode`).
    pub card_button: Option<&'a str>,
}

/// The tools folder the reader's files are taken from.
fn dependency_dir() -> PathBuf {
    launcher_dir().join("tools").join("dependency")
}

/// Makes sure the game folder has a picodaemon.exe the launcher knows, copying it from
/// `tools\dependency` when it has none or an unknown one.
pub fn ensure_picodaemon(game_dir: &Path) -> Result<PathBuf, String> {
    ensure_picodaemon_from(game_dir, &dependency_dir())
}

/// The Cg runtime, which Lindbergh games need.
const CG_LIBS: [&str; 2] = ["cg.dll", "cgGL.dll"];

/// Copies the Cg runtime into the game folder.
pub fn ensure_cg_libs(game_dir: &Path) -> Result<(), String> {
    ensure_cg_libs_from(game_dir, &cg_source_dirs())
}

/// The folders the Cg runtime is taken from, in order: `tools\dependency`, then the NVIDIA
/// Cg Toolkit installed on the PC. Its installer records the folder of its 32-bit dlls,
/// which the 32-bit kitten needs, in the `CG_BIN_PATH` environment variable.
fn cg_source_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![dependency_dir()];
    if let Some(bin) = std::env::var_os("CG_BIN_PATH") {
        dirs.push(PathBuf::from(bin));
    }
    dirs
}

fn ensure_picodaemon_from(game_dir: &Path, dep: &Path) -> Result<PathBuf, String> {
    let candidate = game_dir.join("picodaemon.exe");
    let src = dep.join("picodaemon.exe");

    if candidate.is_file() && detect::probe_card_reader(game_dir).is_some() {
        return Ok(candidate);
    }
    if candidate.is_file() && src.is_file() && files_equal(&candidate, &src) {
        return Ok(candidate);
    }
    if !src.is_file() {
        return Err(t!("launch.picodaemon_exe_not_found", src = src.display()).to_string());
    }
    copy_file_if_changed(&src, &candidate)?;
    Ok(candidate)
}

/// Copies the Cg runtime from the first of `sources` that has all of it. Both files come
/// from the same folder, so their versions match.
fn ensure_cg_libs_from(game_dir: &Path, sources: &[PathBuf]) -> Result<(), String> {
    let source = sources
        .iter()
        .find(|dir| CG_LIBS.iter().all(|name| dir.join(name).is_file()))
        .ok_or_else(|| t!("launch.cg_runtime_not_found", dir = dependency_dir().display()).to_string())?;
    for name in CG_LIBS {
        copy_file_if_changed(&source.join(name), &game_dir.join(name))?;
    }
    Ok(())
}

/// Copies mice next to picodaemon and writes the reader's `mice\idas.ini`.
pub fn deploy(
    picodaemon: &Path,
    settings: &Settings,
    detected: &LoaderPaths,
    reader: &ReaderSettings,
) -> Result<CardReader, String> {
    if !picodaemon.is_file() {
        return Err(t!("launch.card_reader_file_does_not", reader_path = picodaemon.display()).to_string());
    }
    let reader_dir = picodaemon.parent().map(Path::to_path_buf).unwrap_or_else(launcher_dir);

    let mice_exe = settings
        .mice_path
        .as_ref()
        .map(PathBuf::from)
        .or_else(|| detected.mice.clone())
        .ok_or_else(|| t!("launch.mice_exe_not_found").to_string())?;
    if !mice_exe.is_file() {
        return Err(t!("launch.mice_loader_does_not_exist", path = mice_exe.display()).to_string());
    }
    let tool_dir = mice_exe
        .parent()
        .ok_or_else(|| t!("launch.cannot_determine_mice_tool_directory", loader = mice_exe.display()).to_string())?;
    for name in ["mice.exe", "mice.dll"] {
        let src = tool_dir.join(name);
        if src.is_file() {
            copy_file_if_changed(&src, &reader_dir.join(name))?;
        }
    }

    let mice_dir = reader_dir.join("mice");
    fs::create_dir_all(&mice_dir).map_err(|e| {
        t!("launch.failed_to_create_card_reader", mice_dir = mice_dir.display(), error = e).to_string()
    })?;
    mice::write_atomic(&mice_dir.join("idas.ini"), &render_ini(reader))?;

    Ok(CardReader { picodaemon: picodaemon.to_path_buf(), mice: mice_exe })
}

/// The reader's `mice\idas.ini`.
fn render_ini(reader: &ReaderSettings) -> String {
    let set = |v: Option<&str>| v.map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
    let mut ini = format!("[CardReader]\nauto_insert = {}\n", reader.auto_insert);
    if let Some(card) = reader.card {
        ini.push_str(&format!("cardfile = {card}\n"));
    }
    if let Some(ip) = set(reader.game_ip) {
        ini.push_str(&format!("[ServerBox]\ngame_ip = {ip}\n"));
    }
    if let Some(mode) = set(reader.io3_mode) {
        ini.push_str(&format!("[io3]\nmode = {mode}\n"));
    }
    if let Some(button) = set(reader.card_button) {
        ini.push_str(&format!("[xinput]\ncard = {button}\n"));
    }
    ini
}

impl CardReader {
    /// Starts the reader as `mice.exe -p -b picodaemon.exe` in the game folder, with its
    /// console window visible.
    pub fn start(&self) -> Result<Child, String> {
        let dir = self.picodaemon.parent().map(Path::to_path_buf).unwrap_or_else(launcher_dir);
        let file = self.picodaemon.file_name().map(PathBuf::from).unwrap_or_else(|| self.picodaemon.clone());
        Command::new(&self.mice)
            .current_dir(&dir)
            .arg("-p")
            .arg("-b")
            .arg(&file)
            .spawn()
            .map_err(|e| {
                t!("launch.failed_to_start_card_reader", mice_exe = self.mice.display(), error = e).to_string()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_util::test_dir::{fresh, remove, write};

    #[test]
    fn ensure_cg_libs_missing_everywhere_errors() {
        let game = fresh("cg_missing_game");
        let dep = fresh("cg_missing_dep");
        let toolkit = fresh("cg_missing_toolkit");
        let err = ensure_cg_libs_from(&game, &[dep.clone(), toolkit.clone()]).unwrap_err();
        assert!(err.contains("cg.dll"), "the error should say cg.dll is missing: {err}");
        remove(&game);
        remove(&dep);
        remove(&toolkit);
    }

    #[test]
    fn ensure_cg_libs_prefers_the_dependency_folder() {
        let game = fresh("cg_game");
        let dep = fresh("cg_dep");
        let toolkit = fresh("cg_toolkit");
        for dir in [&dep, &toolkit] {
            write(&dir.join("cg.dll"), dir.to_string_lossy().as_bytes());
            write(&dir.join("cgGL.dll"), dir.to_string_lossy().as_bytes());
        }
        ensure_cg_libs_from(&game, &[dep.clone(), toolkit.clone()]).unwrap();
        assert_eq!(fs::read(game.join("cg.dll")).unwrap(), dep.to_string_lossy().as_bytes());
        assert_eq!(fs::read(game.join("cgGL.dll")).unwrap(), dep.to_string_lossy().as_bytes());
        remove(&game);
        remove(&dep);
        remove(&toolkit);
    }

    #[test]
    fn ensure_cg_libs_takes_both_from_the_toolkit_when_the_folder_lacks_one() {
        let game = fresh("cg_fallback_game");
        let dep = fresh("cg_fallback_dep");
        let toolkit = fresh("cg_fallback_toolkit");
        write(&dep.join("cg.dll"), b"dependency");
        write(&toolkit.join("cg.dll"), b"toolkit");
        write(&toolkit.join("cgGL.dll"), b"toolkit");
        ensure_cg_libs_from(&game, &[dep.clone(), toolkit.clone()]).unwrap();
        assert_eq!(fs::read(game.join("cg.dll")).unwrap(), b"toolkit".to_vec(), "the two files must not mix");
        assert_eq!(fs::read(game.join("cgGL.dll")).unwrap(), b"toolkit".to_vec());
        remove(&game);
        remove(&dep);
        remove(&toolkit);
    }

    #[test]
    fn ensure_picodaemon_missing_dependency_errors() {
        let game = fresh("pd_missing_game");
        let dep = fresh("pd_missing_dep");
        let err = ensure_picodaemon_from(&game, &dep).unwrap_err();
        assert!(err.contains("picodaemon.exe"), "the error should say picodaemon.exe is missing: {err}");
        remove(&game);
        remove(&dep);
    }

    #[test]
    fn ensure_picodaemon_copies_when_absent() {
        let game = fresh("pd_game");
        let dep = fresh("pd_dep");
        write(&dep.join("picodaemon.exe"), b"pico-build");
        let out = ensure_picodaemon_from(&game, &dep).unwrap();
        assert_eq!(out, game.join("picodaemon.exe"));
        assert!(game.join("picodaemon.exe").is_file());
        remove(&game);
        remove(&dep);
    }

    #[test]
    fn ensure_picodaemon_reuses_identical_existing() {
        let game = fresh("pd_reuse_game");
        let dep = fresh("pd_reuse_dep");
        write(&dep.join("picodaemon.exe"), b"same-build");
        ensure_picodaemon_from(&game, &dep).unwrap();
        let out = ensure_picodaemon_from(&game, &dep).unwrap();
        assert_eq!(out, game.join("picodaemon.exe"));
        assert!(game.join("picodaemon.exe").is_file());
        remove(&game);
        remove(&dep);
    }

    #[test]
    fn deploy_writes_cardfile_gameip_io3_and_card_button() {
        let root = fresh("deploy_reader");
        let tool = root.join("mice_tool");
        let reader_dir = root.join("reader");
        fs::create_dir_all(&tool).unwrap();
        fs::create_dir_all(&reader_dir).unwrap();
        write(&tool.join("mice.exe"), b"mice-bin");
        write(&tool.join("mice.dll"), b"dll-bin");
        write(&reader_dir.join("picodaemon.exe"), b"pico");
        let mut settings = Settings::default();
        settings.mice_path = Some(tool.join("mice.exe").to_string_lossy().into_owned());
        let detected = LoaderPaths::default();
        let picodaemon = reader_dir.join("picodaemon.exe");
        let ini = || fs::read_to_string(reader_dir.join("mice/idas.ini")).unwrap();

        let reader = deploy(
            &picodaemon,
            &settings,
            &detected,
            &ReaderSettings {
                card: Some("01010409725450755799"),
                game_ip: Some("192.168.100.101"),
                auto_insert: false,
                io3_mode: Some("xinput"),
                card_button: Some("START"),
            },
        )
        .unwrap();
        assert!(reader.mice.is_file());
        let text = ini();
        assert!(text.contains("[CardReader]\nauto_insert = false\ncardfile = 01010409725450755799\n"));
        assert!(text.contains("[ServerBox]\ngame_ip = 192.168.100.101\n"));
        assert!(text.contains("[io3]\nmode = xinput\n"));
        assert!(text.contains("[xinput]\ncard = START\n"));

        let basic = ReaderSettings { card: Some("0101"), auto_insert: true, ..Default::default() };
        deploy(&picodaemon, &settings, &detected, &basic).unwrap();
        let text = ini();
        assert!(text.contains("[CardReader]\nauto_insert = true\ncardfile = 0101\n"));
        assert!(!text.contains("[io3]"));
        assert!(!text.contains("[xinput]"));

        let blank_button = ReaderSettings {
            card: Some("0101"),
            io3_mode: Some("dinput"),
            card_button: Some("  "),
            ..Default::default()
        };
        deploy(&picodaemon, &settings, &detected, &blank_button).unwrap();
        let text = ini();
        assert!(text.contains("[CardReader]\nauto_insert = false\ncardfile = 0101\n"));
        assert!(text.contains("[io3]\nmode = dinput\n"));
        assert!(!text.contains("[xinput]"));

        remove(&root);
    }
}
