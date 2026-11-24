//! Reads the family name out of a font file (TrueType `.ttf`, OpenType `.otf`, or the first
//! font of a collection `.ttc`): the name Windows knows the font by, and so the one a program
//! asks for when it has loaded the file.
//!
//! A font file is a set of tables. The `name` table holds the font's names as records, each
//! with a platform, a language and a name id; the family name is name id 1. Windows looks fonts
//! up by the record for the Windows platform (3) in US English (0x0409), stored in UTF-16
//! (big-endian).

use std::path::Path;

/// Name id of the family name.
const FAMILY_NAME: u16 = 1;
/// Platform id of the records Windows uses.
const WINDOWS_PLATFORM: u16 = 3;
/// Language id of US English on the Windows platform.
const US_ENGLISH: u16 = 0x0409;

/// The family name of the font in the file, or `None` when the file is not a font with one.
pub fn family_name(path: &Path) -> Option<String> {
    family_name_in(&std::fs::read(path).ok()?)
}

/// The family name of the font in `data`, the bytes of a font file.
fn family_name_in(data: &[u8]) -> Option<String> {
    // A collection begins with "ttcf" and the offsets of its fonts; take the first font.
    let font = if data.get(0..4)? == b"ttcf" { u32_at(data, 12)? as usize } else { 0 };
    let names = table(data, font, b"name")?;

    let count = u16_at(names, 2)? as usize;
    let strings = u16_at(names, 4)? as usize;
    let mut fallback = None;
    for record in (0..count).map(|i| names.get(6 + i * 12..6 + (i + 1) * 12)) {
        let record = record?;
        let (platform, language, id) = (u16_at(record, 0)?, u16_at(record, 4)?, u16_at(record, 6)?);
        if platform != WINDOWS_PLATFORM || id != FAMILY_NAME {
            continue;
        }
        let (length, offset) = (u16_at(record, 8)? as usize, u16_at(record, 10)? as usize);
        let name = utf16_be(names.get(strings + offset..strings + offset + length)?)?;
        if language == US_ENGLISH {
            return Some(name);
        }
        // A font without a US English name: its family name in another language.
        fallback.get_or_insert(name);
    }
    fallback
}

/// The table tagged `tag` of the font starting at `font` in `data`.
fn table<'a>(data: &'a [u8], font: usize, tag: &[u8; 4]) -> Option<&'a [u8]> {
    let tables = u16_at(data, font + 4)? as usize;
    (0..tables).find_map(|i| {
        let entry = font + 12 + i * 16;
        if data.get(entry..entry + 4)? != tag {
            return None;
        }
        let (offset, length) = (u32_at(data, entry + 8)? as usize, u32_at(data, entry + 12)? as usize);
        data.get(offset..offset + length)
    })
}

fn u16_at(data: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_be_bytes(data.get(at..at + 2)?.try_into().ok()?))
}

fn u32_at(data: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(data.get(at..at + 4)?.try_into().ok()?))
}

fn utf16_be(bytes: &[u8]) -> Option<String> {
    let units: Vec<u16> = bytes.chunks_exact(2).map(|pair| u16::from_be_bytes([pair[0], pair[1]])).collect();
    let name = String::from_utf16(&units).ok()?;
    (!name.is_empty()).then_some(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A font with only a `name` table, holding `(platform, language, name id, text)` records.
    fn font_with_names(names: &[(u16, u16, u16, &str)]) -> Vec<u8> {
        let mut strings = Vec::new();
        let mut records = Vec::new();
        for (platform, language, id, text) in names {
            let bytes: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
            for field in [*platform, 1, *language, *id, bytes.len() as u16, strings.len() as u16] {
                records.extend(field.to_be_bytes());
            }
            strings.extend(bytes);
        }
        let mut name_table = Vec::new();
        for field in [0u16, names.len() as u16, 6 + records.len() as u16] {
            name_table.extend(field.to_be_bytes());
        }
        name_table.extend(records);
        name_table.extend(strings);

        // Offset table (12 bytes) and one table record (16 bytes), then the table.
        let mut font = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        font.extend(b"name");
        font.extend(0u32.to_be_bytes());
        font.extend(28u32.to_be_bytes());
        font.extend((name_table.len() as u32).to_be_bytes());
        font.extend(name_table);
        font
    }

    #[test]
    fn reads_the_us_english_family_name() {
        let font = font_with_names(&[
            (WINDOWS_PLATFORM, 0x0411, FAMILY_NAME, "東風ゴシック"),
            (WINDOWS_PLATFORM, US_ENGLISH, 2, "Regular"),
            (WINDOWS_PLATFORM, US_ENGLISH, FAMILY_NAME, "Kochi Gothic"),
        ]);
        assert_eq!(family_name_in(&font).as_deref(), Some("Kochi Gothic"));
    }

    #[test]
    fn takes_another_language_when_there_is_no_us_english_name() {
        let font = font_with_names(&[(WINDOWS_PLATFORM, 0x0411, FAMILY_NAME, "東風ゴシック")]);
        assert_eq!(family_name_in(&font).as_deref(), Some("東風ゴシック"));
    }

    #[test]
    fn reads_the_first_font_of_a_collection() {
        let font = font_with_names(&[(WINDOWS_PLATFORM, US_ENGLISH, FAMILY_NAME, "MS Gothic")]);
        // "ttcf", version, one font at offset 16; the font's table offsets move by 16.
        let mut collection = b"ttcf".to_vec();
        collection.extend([0, 1, 0, 0, 0, 0, 0, 1, 0, 0, 0, 16]);
        let mut moved = font.clone();
        moved[20..24].copy_from_slice(&(28u32 + 16).to_be_bytes());
        collection.extend(moved);
        assert_eq!(family_name_in(&collection).as_deref(), Some("MS Gothic"));
    }

    #[test]
    fn a_file_that_is_not_a_font_has_no_name() {
        assert_eq!(family_name_in(b"hello"), None);
        assert_eq!(family_name_in(&font_with_names(&[(1, 0, FAMILY_NAME, "Mac only")])), None);
    }
}
