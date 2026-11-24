use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

pub fn read_value(ini: &Path, section: &str, key: &str) -> Option<String> {
    let text = fs::read_to_string(ini).ok()?;
    let header = format!("[{section}]");
    let mut in_sec = false;
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_sec = t.eq_ignore_ascii_case(&header);
        } else if in_sec {
            if let Some((k, v)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim().to_string());
                }
            }
        }
    }
    None
}

pub struct SectionReader {

    map: HashMap<String, String>,
}

impl SectionReader {
    pub fn load(ini: &Path, section: &str) -> SectionReader {
        let sec_lc = section.to_ascii_lowercase();
        let mut map = HashMap::new();
        if let Ok(text) = fs::read_to_string(ini) {
            let mut in_sec = false;
            for line in text.lines() {
                let t = line.trim();
                if t.starts_with('[') {
                    in_sec = t.to_ascii_lowercase() == format!("[{sec_lc}]");
                } else if in_sec {
                    if let Some((k, v)) = t.split_once('=') {

                        let key = k.trim().to_ascii_lowercase();
                        map.entry(key).or_insert_with(|| v.trim().to_string());
                    }
                }
            }
        }
        SectionReader { map }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.map.get(&key.to_ascii_lowercase()).cloned()
    }
}

pub fn set_value(ini: &Path, section: &str, key: &str, value: &str) -> io::Result<()> {
    set_values(ini, &[(section, key, value)])
}

pub fn set_values(ini: &Path, kvs: &[(&str, &str, &str)]) -> io::Result<()> {

    let mut items: Vec<(&str, &str, &str)> = Vec::with_capacity(kvs.len());
    for kv in kvs {
        match items
            .iter_mut()
            .find(|(s, k, _)| s.eq_ignore_ascii_case(kv.0) && k.eq_ignore_ascii_case(kv.1))
        {
            Some(slot) => *slot = *kv,
            None => items.push(*kv),
        }
    }
    let text = fs::read_to_string(ini).unwrap_or_default();

    let mut present: Vec<String> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|r| r.strip_suffix(']')) {
            present.push(name.trim().to_ascii_lowercase());
        }
    }

    let mut done = vec![false; items.len()];
    let mut out = String::new();
    let mut cur: Option<String> = None;

    macro_rules! flush {
        () => {
            if let Some(sec) = &cur {
                for (i, (s, k, v)) in items.iter().enumerate() {
                    if done[i] || !s.eq_ignore_ascii_case(sec) {
                        continue;
                    }
                    out.push_str(k);
                    out.push_str(" = ");
                    out.push_str(v);
                    out.push('\n');
                    done[i] = true;
                }
            }
        };
    }

    for line in text.lines() {
        let t = line.trim_start();
        if t.starts_with('[') {
            flush!();
            cur = t
                .trim_end()
                .trim_start_matches('[')
                .trim_end_matches(']')
                .to_string()
                .into();
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut matched = false;
        if let Some(sec) = &cur {
            if let Some((k, _)) = t.split_once('=') {
                for (i, (s, kk, v)) in items.iter().enumerate() {
                    if done[i] || !s.eq_ignore_ascii_case(sec) {
                        continue;
                    }
                    if kk.eq_ignore_ascii_case(k.trim()) {
                        out.push_str(kk);
                        out.push_str(" = ");
                        out.push_str(v);
                        out.push('\n');
                        done[i] = true;
                        matched = true;
                        break;
                    }
                }
            }
        }
        if matched {
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    flush!();

    for (i, (s, _, _)) in items.iter().enumerate() {
        if done[i] || present.iter().any(|p| p == &s.to_ascii_lowercase()) {
            continue;
        }
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        out.push('[');
        out.push_str(s);
        out.push_str("]\n");
        for (j, (s2, k, v)) in items.iter().enumerate() {
            if done[j] || !s2.eq_ignore_ascii_case(s) {
                continue;
            }
            out.push_str(k);
            out.push_str(" = ");
            out.push_str(v);
            out.push('\n');
            done[j] = true;
        }
    }
    fs::write(ini, out)
}

pub struct Writer<'a> {
    path: &'a Path,
    kvs: Vec<(&'a str, &'a str, String)>,
}

impl<'a> Writer<'a> {

    pub fn set(&mut self, section: &'a str, key: &'a str, value: &str) {
        self.kvs.push((section, key, value.to_string()));
    }

    pub fn flush(self) -> io::Result<()> {
        let refs: Vec<(&str, &str, &str)> = self
            .kvs
            .iter()
            .map(|(s, k, v)| (*s, *k, v.as_str()))
            .collect();
        set_values(self.path, &refs)
    }
}

pub fn writer(path: &Path) -> Writer<'_> {
    Writer {
        path,
        kvs: Vec::new(),
    }
}

/// The keys and values of a section, in the order of the file, with the keys as written.
pub fn section_entries(ini: &Path, section: &str) -> Vec<(String, String)> {
    let Ok(text) = fs::read_to_string(ini) else {
        return Vec::new();
    };
    let mut entries = Vec::new();
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = section_name(t) {
            in_section = name.eq_ignore_ascii_case(section);
        } else if in_section && !t.starts_with(';') {
            if let Some((k, v)) = t.split_once('=') {
                entries.push((k.trim().to_string(), v.trim().to_string()));
            }
        }
    }
    entries
}

/// Removes keys from a section: the `keys` named, or with `None` the whole section with its
/// header. The rest of the file stays as it is.
pub fn remove_entries(ini: &Path, section: &str, keys: Option<&[&str]>) -> io::Result<()> {
    let Ok(text) = fs::read_to_string(ini) else {
        return Ok(());
    };
    let mut out = String::new();
    let mut in_section = false;
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = section_name(t) {
            in_section = name.eq_ignore_ascii_case(section);
            if in_section && keys.is_none() {
                continue;
            }
        } else if in_section {
            let removed = match keys {
                None => true,
                Some(keys) => t
                    .split_once('=')
                    .is_some_and(|(k, _)| keys.iter().any(|key| key.eq_ignore_ascii_case(k.trim()))),
            };
            if removed {
                continue;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    fs::write(ini, out)
}

/// The name of a section header line (`[name]`), or `None` for any other line.
fn section_name(line: &str) -> Option<&str> {
    line.strip_prefix('[').and_then(|rest| rest.strip_suffix(']')).map(str::trim)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_ini(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("dnet_ini_test");
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn read_requires_matching_section() {
        let ini = temp_ini("read_section.ini");
        fs::write(&ini, "[a]\nkey = 1\n[b]\nkey = 2\n[c]\nother = 3\n").unwrap();
        assert_eq!(read_value(&ini, "a", "key").as_deref(), Some("1"));
        assert_eq!(read_value(&ini, "b", "key").as_deref(), Some("2"));
        assert_eq!(read_value(&ini, "c", "key"), None);
        assert_eq!(read_value(&ini, "missing", "key"), None);

        assert_eq!(read_value(&ini, "A", "KEY").as_deref(), Some("1"));
        fs::remove_file(&ini).ok();
    }

    #[test]
    fn set_value_across_sections_preserves_content() {
        let ini = temp_ini("set_multi.ini");
        fs::write(
            &ini,
            "; comment\n[net]\nip = 1.2.3.4\nextra = keep\n[other]\nx = 1\n",
        )
        .unwrap();
        set_value(&ini, "net", "ip", "5.6.7.8").unwrap();
        let out = fs::read_to_string(&ini).unwrap();
        assert!(out.starts_with("; comment\n[net]\n"));
        assert!(out.contains("ip = 5.6.7.8\n"));
        assert!(out.contains("extra = keep"));
        assert!(out.contains("[other]\nx = 1"));
        assert_eq!(read_value(&ini, "net", "ip").as_deref(), Some("5.6.7.8"));

        set_value(&ini, "other", "x", "2").unwrap();
        let out = fs::read_to_string(&ini).unwrap();
        assert!(out.contains("x = 2"));
        assert!(out.contains("ip = 5.6.7.8"));
        fs::remove_file(&ini).ok();
    }

    #[test]
    fn set_values_matches_sequential_set_value() {
        let base = "; note\n[net]\nip = 1.2.3.4\nextra = keep\n[other]\nx = 1\n";
        let kvs = [
            ("net", "ip", "5.6.7.8"),
            ("net", "adapter", "abc"),
            ("new", "k", "v"),
            ("other", "x", "2"),
        ];
        let one = temp_ini("batch_one.ini");
        let seq = temp_ini("batch_seq.ini");
        let bat = temp_ini("batch_writer.ini");
        fs::write(&one, base).unwrap();
        fs::write(&seq, base).unwrap();
        fs::write(&bat, base).unwrap();

        set_values(&one, &kvs).unwrap();
        for (s, k, v) in kvs {
            set_value(&seq, s, k, v).unwrap();
        }
        let mut w = writer(&bat);
        for (s, k, v) in kvs {
            w.set(s, k, v);
        }
        w.flush().unwrap();

        let expected = fs::read_to_string(&seq).unwrap();

        assert_eq!(fs::read_to_string(&one).unwrap(), expected);
        assert_eq!(fs::read_to_string(&bat).unwrap(), expected);
        for f in [&one, &seq, &bat] {
            fs::remove_file(f).ok();
        }
    }

    #[test]
    fn set_value_appends_missing_section() {
        let ini = temp_ini("set_append.ini");
        fs::write(&ini, "[a]\nx = 1").unwrap();
        set_value(&ini, "net", "adapter", "abc").unwrap();
        set_value(&ini, "net", "ip", "1.1.1.1").unwrap();
        let out = fs::read_to_string(&ini).unwrap();
        let a_pos = out.find("[a]").unwrap();
        let net_pos = out.find("[net]").unwrap();
        let adapter_pos = out.find("adapter").unwrap();
        let ip_pos = out.find("ip").unwrap();
        assert!(a_pos < net_pos && adapter_pos < ip_pos);
        assert!(out.contains("adapter = abc\n"));
        assert!(out.contains("ip = 1.1.1.1\n"));
        assert!(out.contains("x = 1"));
        fs::remove_file(&ini).ok();
    }

    #[test]
    fn section_entries_keep_the_order_and_skip_comments() {
        let ini = temp_ini("entries.ini");
        fs::write(&ini, "[a]\nx = 1\n[input]\n; note\nKind = dinput\nratio = 128\n[b]\ny = 2\n").unwrap();
        assert_eq!(
            section_entries(&ini, "INPUT"),
            vec![("Kind".to_string(), "dinput".to_string()), ("ratio".to_string(), "128".to_string())]
        );
        assert!(section_entries(&ini, "missing").is_empty());
        fs::remove_file(&ini).ok();
    }

    #[test]
    fn remove_entries_takes_out_keys_or_a_whole_section() {
        let ini = temp_ini("remove.ini");
        fs::write(&ini, "; top\n[system]\nkeychip = A\nloglevel = info\n[input]\nkind = dinput\n[vpn]\nconfig = c\n").unwrap();
        remove_entries(&ini, "system", Some(&["LogLevel"])).unwrap();
        remove_entries(&ini, "input", None).unwrap();
        assert_eq!(fs::read_to_string(&ini).unwrap(), "; top\n[system]\nkeychip = A\n[vpn]\nconfig = c\n");
        fs::remove_file(&ini).ok();
    }
}
