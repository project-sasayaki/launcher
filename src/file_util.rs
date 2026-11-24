//! Small file helpers for putting tools and patches into game folders.

use std::fs;
use std::path::{Path, PathBuf};

use rust_i18n::t;

/// Whether two files have the same content.
pub fn files_equal(a: &Path, b: &Path) -> bool {
    if let (Ok(ma), Ok(mb)) = (fs::metadata(a), fs::metadata(b)) {
        if ma.len() != mb.len() {
            return false;
        }
    }
    let (Ok(ra), Ok(rb)) = (fs::read(a), fs::read(b)) else {
        return false;
    };
    ra == rb
}

/// Copies `src` to `dest`, unless `dest` already has the same size and modification time.
pub fn copy_file_if_changed(src: &Path, dest: &Path) -> Result<(), String> {
    if let (Ok(a), Ok(b)) = (fs::metadata(src), fs::metadata(dest)) {
        if a.len() == b.len() && a.modified().ok() == b.modified().ok() {
            return Ok(());
        }
    }
    fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| t!("common.failed_to_copy", src = src.display(), error = e).to_string())
}

/// Copies every file under `src`, from all its subfolders, straight into `dest`.
pub fn copy_files_flat(src: &Path, dest: &Path) -> Result<(), String> {
    fs::create_dir_all(dest).map_err(|e| {
        t!("launch.failed_to_create_directory", dest = dest.display(), error = e).to_string()
    })?;
    let entries = fs::read_dir(src)
        .map_err(|e| t!("launch.failed_to_read_directory", src = src.display(), error = e).to_string())?;
    for entry in entries {
        let e = entry.map_err(|e| {
            t!("launch.failed_to_read_directory_entry", src = src.display(), error = e).to_string()
        })?;
        let ft = e.file_type().map_err(|e| t!("launch.failed_to_read_file_type", error = e).to_string())?;
        if ft.is_dir() {
            copy_files_flat(&e.path(), dest)?;
        } else if ft.is_file() {
            copy_file_if_changed(&e.path(), &dest.join(e.file_name()))?;
        }
    }
    Ok(())
}

/// The first file named `name` (in any case) under `dir` or its subfolders.
pub fn find_file_recursive(dir: &Path, name: &str) -> Option<PathBuf> {
    if !dir.is_dir() {
        return None;
    }
    for entry in fs::read_dir(dir).ok()?.flatten() {
        let p = entry.path();
        if p.is_dir() {
            if let Some(found) = find_file_recursive(&p, name) {
                return Some(found);
            }
        } else if entry.file_name().to_string_lossy().eq_ignore_ascii_case(name) {
            return Some(p);
        }
    }
    None
}

#[cfg(test)]
pub(crate) mod test_dir {
    //! Temporary folders for tests that touch the file system.

    use std::path::{Path, PathBuf};

    /// A fresh, empty folder for one test, named after it.
    pub fn fresh(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("dnet_launch_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    pub fn write(path: &Path, bytes: &[u8]) {
        std::fs::write(path, bytes).unwrap();
    }

    pub fn remove(p: &Path) {
        let _ = std::fs::remove_dir_all(p);
    }
}

#[cfg(test)]
mod tests {
    use super::test_dir::{fresh, remove, write};
    use super::*;

    #[test]
    fn files_equal_detects_difference() {
        let a = fresh("fe_a");
        let b = fresh("fe_b");
        write(&a.join("f"), b"x");
        write(&b.join("f"), b"y");
        assert!(!files_equal(&a.join("f"), &b.join("f")));
        write(&b.join("f"), b"x");
        assert!(files_equal(&a.join("f"), &b.join("f")));
        remove(&a);
        remove(&b);
    }
}
