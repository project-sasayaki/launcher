use std::fs;
use std::path::Path;

use rust_i18n::t;

pub(crate) struct Sections(Vec<(String, Vec<String>)>);

impl Sections {
    pub(crate) fn new() -> Self {
        Sections(Vec::new())
    }

    pub(crate) fn push(&mut self, section: &str, key: &str, value: &str) {
        if let Some((_, lines)) = self.0.iter_mut().find(|(s, _)| s == section) {
            lines.push(format!("{key} = {value}"));
        } else {
            self.0
                .push((section.to_string(), vec![format!("{key} = {value}")]));
        }
    }
}

impl std::fmt::Display for Sections {

    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, (title, lines)) in self.0.iter().enumerate() {
            if i > 0 {
                writeln!(f)?;
            }
            writeln!(f, "[{title}]")?;
            for line in lines {
                writeln!(f, "{line}")?;
            }
        }
        Ok(())
    }
}

pub(crate) fn bool_str(b: bool) -> &'static str {
    if b {
        "true"
    } else {
        "false"
    }
}

pub(crate) fn split_resolution(v: &str) -> Option<(u32, u32)> {
    let s = v.trim();
    let x = s.find(['x', 'X'])?;
    let w = s[..x].trim().parse().ok()?;
    let h = s[x + 1..].trim().parse().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

pub(crate) fn nx_num(v: &str) -> Option<u32> {
    let digits: String = v.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        return None;
    }
    digits.parse().ok().filter(|n| *n > 0)
}

pub(crate) fn deadzone_value(pct: u32) -> u32 {
    (((32767u64 * pct as u64) + 50) / 100) as u32
}

pub(crate) fn push_xi_button(s: &mut Sections, section: &str, key: &str, name: &Option<String>) {
    if let Some(v) = name.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
        s.push(section, key, v);
    }
}

pub(crate) fn copy_tool_files(
    tool_dir: &Path,
    dest: &Path,
    exclude: &[&str],
    label: &str,
) -> Result<(), String> {
    let entries = fs::read_dir(tool_dir).map_err(|e| {
        t!("render_util.failed_to_read_tool_directory", label = label, tool_dir = tool_dir.display(), error = e).to_string()
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| {
            t!("render_util.failed_to_read_tool_directory_entry", label = label, tool_dir = tool_dir.display(), error = e).to_string()
        })?;
        let Ok(ft) = entry.file_type() else {
            continue;
        };
        if !ft.is_file() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(String::from) else {
            continue;
        };
        let lower = name.to_lowercase();
        if exclude.contains(&lower.as_str()) || lower.ends_with(".log") {
            continue;
        }
        let src = entry.path();
        let dst = dest.join(&name);
        if same_file(&src, &dst) {
            continue;
        }
        fs::copy(&src, &dst).map_err(|e| {
            t!("render_util.failed_to_copy_tool_file", label = label, src = src.display(), dst = dst.display(), error = e).to_string()
        })?;
    }
    Ok(())
}

pub(crate) fn same_file(src: &Path, dst: &Path) -> bool {
    match (fs::metadata(src), fs::metadata(dst)) {
        (Ok(s), Ok(d)) => {
            s.is_file() && d.is_file() && s.len() == d.len() && s.modified().ok() == d.modified().ok()
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_preserves_order_and_blank_line() {
        let mut s = Sections::new();
        s.push("b", "k2", "v2");
        s.push("a", "k1", "v1");
        s.push("b", "k3", "v3");
        assert_eq!(s.to_string(), "[b]\nk2 = v2\nk3 = v3\n\n[a]\nk1 = v1\n");
    }

    #[test]
    fn split_resolution_variants() {
        assert_eq!(split_resolution("1920x1080"), Some((1920, 1080)));
        assert_eq!(split_resolution(" 1280X720 "), Some((1280, 720)));
        assert_eq!(split_resolution("0x100"), None);
        assert_eq!(split_resolution("abc"), None);
    }

    #[test]
    fn nx_num_variants() {
        assert_eq!(nx_num("2x"), Some(2));
        assert_eq!(nx_num("16X"), Some(16));
        assert_eq!(nx_num("4"), Some(4));
        assert_eq!(nx_num("x4"), None);
        assert_eq!(nx_num("0x"), None);
    }

    #[test]
    fn deadzone_rounds_half_up() {
        assert_eq!(deadzone_value(0), 0);
        assert_eq!(deadzone_value(100), 32767);
        assert_eq!(deadzone_value(20), 6553);
    }
}
