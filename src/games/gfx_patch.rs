//! The graphics patch of Initial D Arcade Stage 6–8 and the D4/D5 ServerBox.
//!
//! Ultimate-ASI-Loader (`tools\asi-loader\dinput8.dll`) goes into the folder as
//! `winmm.dll` and loads the shaderfix plugins from the folder's `scripts\`. Turning the
//! patch off removes `winmm.dll` and leaves `scripts\` in place.

use std::fs;
use std::path::{Path, PathBuf};

use crate::file_util::{copy_file_if_changed, copy_files_flat, find_file_recursive};

/// Puts the patch into `dir`, or takes it out when `enabled` is false.
pub fn deploy(dir: &Path, enabled: bool) -> Result<(), String> {
    deploy_from(dir, enabled, &crate::update::tools_dir())
}

fn deploy_from(dir: &Path, enabled: bool, tools: &Path) -> Result<(), String> {
    let winmm = dir.join("winmm.dll");
    if !enabled {
        if winmm.is_file() {
            let _ = fs::remove_file(&winmm);
        }
        return Ok(());
    }

    if let Some(dll) = find_asi_loader(tools) {
        copy_file_if_changed(&dll, &winmm)?;
    }
    let shaderfix = tools.join("shaderfix");
    if shaderfix.is_dir() {
        copy_files_flat(&shaderfix, &dir.join("scripts"))?;
    }
    Ok(())
}

/// Ultimate-ASI-Loader's `dinput8.dll` in `tools\asi-loader`, in any of its subfolders.
pub fn find_asi_loader(tools: &Path) -> Option<PathBuf> {
    let root = tools.join("asi-loader");
    let direct = root.join("dinput8.dll");
    if direct.is_file() {
        return Some(direct);
    }
    find_file_recursive(&root, "dinput8.dll")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_util::test_dir::{fresh, remove, write};

    fn tools_with_gfx(name: &str) -> PathBuf {
        let tools = fresh(name);
        fs::create_dir_all(tools.join("asi-loader")).unwrap();
        write(&tools.join("asi-loader/dinput8.dll"), b"loader");
        fs::create_dir_all(tools.join("shaderfix/plugins")).unwrap();
        write(&tools.join("shaderfix/shaderfix.asi"), b"asi");
        write(&tools.join("shaderfix/plugins/extra.asi"), b"extra");
        tools
    }

    #[test]
    fn deploys_loader_and_shaderfix() {
        let dir = fresh("gfx_dir");
        let tools = tools_with_gfx("gfx_tools");
        deploy_from(&dir, true, &tools).unwrap();
        assert_eq!(fs::read(dir.join("winmm.dll")).unwrap(), b"loader".to_vec());
        assert_eq!(fs::read(dir.join("scripts/shaderfix.asi")).unwrap(), b"asi".to_vec());
        assert_eq!(fs::read(dir.join("scripts/extra.asi")).unwrap(), b"extra".to_vec());
        remove(&dir);
        remove(&tools);
    }

    #[test]
    fn disabled_removes_winmm() {
        let dir = fresh("gfx_off_dir");
        let tools = tools_with_gfx("gfx_off_tools");
        write(&dir.join("winmm.dll"), b"old");
        deploy_from(&dir, false, &tools).unwrap();
        assert!(!dir.join("winmm.dll").is_file(), "with the switch off winmm.dll should be removed");
        remove(&dir);
        remove(&tools);
    }

    #[test]
    fn missing_components_is_noop() {
        let dir = fresh("gfx_empty_dir");
        let tools = fresh("gfx_empty_tools");
        deploy_from(&dir, true, &tools).unwrap();
        assert!(!dir.join("winmm.dll").exists());
        assert!(!dir.join("scripts").exists());
        remove(&dir);
        remove(&tools);
    }

    #[test]
    fn finds_loader_in_nested_dir() {
        let dir = fresh("gfx_nested_dir");
        let tools = fresh("gfx_nested_tools");
        fs::create_dir_all(tools.join("asi-loader/Ultimate-ASI-Loader-win32")).unwrap();
        write(&tools.join("asi-loader/Ultimate-ASI-Loader-win32/dinput8.dll"), b"loader");
        deploy_from(&dir, true, &tools).unwrap();
        assert_eq!(fs::read(dir.join("winmm.dll")).unwrap(), b"loader".to_vec());
        remove(&dir);
        remove(&tools);
    }
}
