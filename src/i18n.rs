//! The interface language.
//!
//! Every text the user sees lives in `locales/<locale>.yml` and is looked up by key with
//! rust-i18n's `t!` macro; the files are compiled into the binary by the `i18n!` call in
//! `main.rs`. This module knows which languages the launcher offers, how the choice is
//! stored in `config.ini`, and which language the system prefers.

use std::sync::atomic::{AtomicU8, Ordering};

/// A language the interface can be shown in.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Lang {
    Zh,
    En,
    Ja,
}

impl Lang {
    /// Every language, in the order the Settings page lists them.
    pub const ALL: [Lang; 3] = [Lang::Zh, Lang::En, Lang::Ja];

    /// The rust-i18n locale holding this language's texts: the name of its file in `locales/`.
    pub fn locale(self) -> &'static str {
        match self {
            Lang::Zh => "zh-CN",
            Lang::En => "en",
            Lang::Ja => "ja",
        }
    }

    /// The language's name in that language, as the language selector shows it.
    pub fn native_name(self) -> &'static str {
        match self {
            Lang::Zh => "中文",
            Lang::En => "English",
            Lang::Ja => "日本語",
        }
    }

    /// The value stored in `config.ini [ui] lang`.
    pub fn ini_value(self) -> &'static str {
        match self {
            Lang::Zh => "zh-CN",
            Lang::En => "en-US",
            Lang::Ja => "ja-JP",
        }
    }

    /// The language a `config.ini [ui] lang` value names, if it names one.
    pub fn from_ini_value(value: &str) -> Option<Lang> {
        Lang::ALL.into_iter().find(|lang| lang.ini_value() == value.trim())
    }

    /// The language for a Windows locale name such as `ja-JP` or `zh-Hans-CN`:
    /// Chinese and Japanese by their language part, English for everything else.
    pub fn from_locale_name(name: &str) -> Lang {
        let language = name.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
        match language.as_str() {
            "zh" => Lang::Zh,
            "ja" => Lang::Ja,
            _ => Lang::En,
        }
    }
}

/// The language shown now, kept as an index into [`Lang::ALL`] so code can match on a
/// `Lang` instead of comparing rust-i18n's locale string.
static CURRENT: AtomicU8 = AtomicU8::new(1);

/// The language the interface is shown in.
pub fn current() -> Lang {
    Lang::ALL
        .get(usize::from(CURRENT.load(Ordering::Relaxed)))
        .copied()
        .unwrap_or(Lang::En)
}

/// Shows the interface in `lang` from now on.
pub fn set_lang(lang: Lang) {
    let index = Lang::ALL.iter().position(|l| *l == lang).unwrap_or(1);
    CURRENT.store(index as u8, Ordering::Relaxed);
    rust_i18n::set_locale(lang.locale());
}

/// The language of the user's Windows locale.
pub fn detect_system() -> Lang {
    use windows::Win32::Globalization::GetUserDefaultLocaleName;

    let mut buf = [0u16; 85];
    let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
    if len <= 0 {
        return Lang::En;
    }
    // The returned length counts the terminating NUL.
    let n = (len as usize).saturating_sub(1).min(buf.len());
    Lang::from_locale_name(&String::from_utf16_lossy(&buf[..n]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::Path;

    #[test]
    fn ini_values_round_trip() {
        for lang in Lang::ALL {
            assert_eq!(Lang::from_ini_value(lang.ini_value()), Some(lang));
        }
        assert_eq!(Lang::from_ini_value("fr-FR"), None);
    }

    #[test]
    fn locale_names_map_to_languages() {
        assert_eq!(Lang::from_locale_name("ja-JP"), Lang::Ja);
        assert_eq!(Lang::from_locale_name("zh-CN"), Lang::Zh);
        assert_eq!(Lang::from_locale_name("zh-Hans-CN"), Lang::Zh);
        assert_eq!(Lang::from_locale_name("zh-TW"), Lang::Zh);
        assert_eq!(Lang::from_locale_name("en-US"), Lang::En);
        assert_eq!(Lang::from_locale_name("de-DE"), Lang::En);
        assert_eq!(Lang::from_locale_name(""), Lang::En);
    }

    /// Every locale file holds exactly the keys of the English one, and each text uses the
    /// same `%{name}` placeholders as its English counterpart.
    #[test]
    fn locale_files_agree_with_english() {
        let english = messages(Lang::En);
        assert!(!english.is_empty(), "en.yml has no texts");
        for lang in Lang::ALL {
            let texts = messages(lang);
            let missing: Vec<_> = english.keys().filter(|k| !texts.contains_key(*k)).collect();
            let extra: Vec<_> = texts.keys().filter(|k| !english.contains_key(*k)).collect();
            assert!(missing.is_empty(), "{}.yml lacks {missing:?}", lang.locale());
            assert!(extra.is_empty(), "{}.yml has unknown keys {extra:?}", lang.locale());
            for (key, text) in &texts {
                assert_eq!(
                    placeholders(text),
                    placeholders(&english[key]),
                    "{}.yml `{key}` has other placeholders than en.yml",
                    lang.locale()
                );
            }
        }
    }

    /// Every `t!` call in the sources names a key that exists in the locale files and passes
    /// exactly the placeholders its text uses; rust-i18n itself would show the bare key, or
    /// leave a `%{name}` in the text, at run time.
    #[test]
    fn calls_in_sources_match_the_texts() {
        let english = messages(Lang::En);
        let mut problems = BTreeSet::new();
        for call in source_calls(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
            match english.get(&call.key) {
                None => {
                    problems.insert(format!("{}: unknown key {}", call.file, call.key));
                }
                Some(text) if placeholders(text) != call.args.iter().map(String::as_str).collect() => {
                    problems.insert(format!(
                        "{}: {} passes {:?}, its text uses {:?}",
                        call.file,
                        call.key,
                        call.args,
                        placeholders(text)
                    ));
                }
                Some(_) => {}
            }
        }
        assert!(problems.is_empty(), "{problems:#?}");
    }

    /// Every text in the locale files is used somewhere in the sources, as a `t!` key or in a
    /// table of keys, so the files do not collect texts nobody shows.
    #[test]
    fn every_text_is_used() {
        let sources = source_files(&Path::new(env!("CARGO_MANIFEST_DIR")).join("src"))
            .into_iter()
            .map(|path| std::fs::read_to_string(path).expect("a readable source file"))
            .collect::<Vec<_>>()
            .join("
");
        let unused: Vec<_> =
            messages(Lang::En).into_keys().filter(|key| !sources.contains(&format!("\"{key}\""))).collect();
        assert!(unused.is_empty(), "texts no source uses: {unused:?}");
    }

    /// All texts of a locale, by key. `_rust_i18n_backend` is generated by the `i18n!` call.
    fn messages(lang: Lang) -> BTreeMap<String, String> {
        crate::_rust_i18n_backend()
            .messages_for_locale(lang.locale())
            .unwrap_or_default()
            .into_iter()
            .map(|(k, v)| (k.into_owned(), v.into_owned()))
            .filter(|(k, _)| k != "_version") // the file format version, not a text
            .collect()
    }

    /// The distinct placeholder names of a text, sorted.
    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.match_indices("%{")
            .filter_map(|(start, _)| {
                let rest = &text[start + 2..];
                rest.find('}').map(|end| &rest[..end])
            })
            .collect()
    }

    /// A call of the `t!` macro with a literal key, as written in a source file.
    struct SourceCall {
        file: String,
        key: String,
        /// The names of the `name = value` arguments.
        args: BTreeSet<String>,
    }

    /// Every call of the `t!` macro with a literal key in the `.rs` files under `dir`
    /// (`format!` and other macros whose names end in `t` are not calls of it).
    fn source_calls(dir: &Path) -> Vec<SourceCall> {
        let mut found = Vec::new();
        for path in source_files(dir) {
            let text = std::fs::read_to_string(&path).expect("a readable source file");
            for (start, _) in text.match_indices("t!(\"") {
                let before = text[..start].chars().next_back();
                if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                    continue;
                }
                let rest = &text[start + 4..];
                let Some(end) = rest.find('"') else { continue };
                found.push(SourceCall {
                    file: path.display().to_string(),
                    key: rest[..end].to_string(),
                    args: argument_names(&rest[end + 1..]),
                });
            }
        }
        found
    }

    /// The `.rs` files under `dir`, in all its subfolders.
    fn source_files(dir: &Path) -> Vec<std::path::PathBuf> {
        let mut files = Vec::new();
        for entry in std::fs::read_dir(dir).expect("the source folder should be readable") {
            let path = entry.expect("a readable folder entry").path();
            if path.is_dir() {
                files.extend(source_files(&path));
            } else if path.extension().is_some_and(|e| e == "rs") {
                files.push(path);
            }
        }
        files
    }

    /// The names of the `name = value` arguments of a macro call, given the text after its
    /// first argument. Commas inside brackets or strings do not separate arguments.
    fn argument_names(after_key: &str) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        let mut depth = 0usize;
        let mut in_string = false;
        let mut chars = after_key.char_indices().peekable();
        let mut arg_start = None;
        while let Some((i, c)) = chars.next() {
            if in_string {
                match c {
                    '\\' => {
                        chars.next();
                    }
                    '"' => in_string = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '"' => in_string = true,
                '(' | '[' | '{' => depth += 1,
                ')' | ']' | '}' if depth > 0 => depth -= 1,
                ')' if depth == 0 => break,
                ',' if depth == 0 => arg_start = Some(i + 1),
                '=' if depth == 0 && chars.peek().is_some_and(|(_, n)| *n != '=') => {
                    if let Some(s) = arg_start.take() {
                        names.insert(after_key[s..i].trim().to_string());
                    }
                }
                _ => {}
            }
        }
        names
    }
}
