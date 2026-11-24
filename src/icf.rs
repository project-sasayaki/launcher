//! ICF1: the encrypted version information file of ALLS / Nu games (IDAC, IDZ).
//!
//! The file is AES-128-CBC encrypted in 4 KiB chunks with a fixed key and IV; after
//! decrypting a chunk, its first 16 bytes are XORed with the chunk's file offset. The
//! plain text is a 0x40-byte header (CRC32 of the rest, total size, entry count, app id,
//! CRC32 of the enabled entries) followed by 0x40-byte entries; the first enabled entry of
//! type APP carries the game's version.

use std::fmt;
use std::path::{Path, PathBuf};

use aes::Aes128;
use cbc::cipher::{BlockDecryptMut, KeyIvInit, generic_array::GenericArray};
use cbc::Decryptor;
use rust_i18n::t;

/// What an ICF1 file says about the game it belongs to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IcfInfo {
    /// The four-letter app id, e.g. `SDGT`.
    pub id: String,
    /// `major.minor.build`, minor and build with two digits, e.g. `3.21.00`.
    pub version: String,
}

/// Why an ICF1 file could not be read.
#[derive(Debug)]
pub enum IcfError {
    /// The file could not be read from disk.
    Read { path: PathBuf, error: std::io::Error },
    /// The file is not a whole number of AES blocks (this includes an empty file).
    NotBlockSized { len: usize },
    /// The decrypted data is shorter than the header.
    TooShort { len: usize },
    /// The CRC32 in the header does not match the rest of the file.
    HeaderCrc { stored: u32, actual: u32 },
    /// The size in the header is not the file's size.
    SizeMismatch { stored: usize, actual: usize },
    /// Header bytes that must be zero are not.
    NonZeroPadding,
    /// The entry count does not fit the file's size (`expected_len` is `None` when the
    /// count is too large to compute a length from).
    EntryCount { count: u64, expected_len: Option<usize>, actual: usize },
    /// The app id is empty or not ASCII.
    BadAppId,
    /// The CRC32 over the enabled entries does not match the header.
    EntryCrc { stored: u32, actual: u32 },
    /// There is no enabled APP entry, so the file holds no game version.
    NoAppEntry,
}

impl fmt::Display for IcfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            IcfError::Read { path, error } => {
                t!("icf.read_failed", path = path.display(), error = error)
            }
            IcfError::NotBlockSized { len } => t!("icf.not_block_sized", len = len),
            IcfError::TooShort { len } => t!("icf.too_short", len = len),
            IcfError::HeaderCrc { stored, actual } => {
                t!("icf.header_crc", stored = stored : {:08x}, actual = actual : {:08x})
            }
            IcfError::SizeMismatch { stored, actual } => {
                t!("icf.size_mismatch", stored = stored, actual = actual)
            }
            IcfError::NonZeroPadding => t!("icf.non_zero_padding"),
            IcfError::EntryCount { count, expected_len: Some(expected), actual } => t!(
                "icf.entry_count_mismatch",
                count = count,
                expected = expected : {:#x},
                actual = actual : {:#x}
            ),
            IcfError::EntryCount { count, expected_len: None, .. } => {
                t!("icf.entry_count_invalid", count = count)
            }
            IcfError::BadAppId => t!("icf.bad_app_id"),
            IcfError::EntryCrc { stored, actual } => {
                t!("icf.entry_crc", stored = stored : {:08x}, actual = actual : {:08x})
            }
            IcfError::NoAppEntry => t!("icf.no_app_entry"),
        };
        f.write_str(&text)
    }
}

impl std::error::Error for IcfError {}

const MASTER_KEY: [u8; 16] = [
    0x09, 0xCA, 0x5E, 0xFD, 0x30, 0xC9, 0xAA, 0xEF, 0x38, 0x04, 0xD0, 0xA7, 0xE3, 0xFA, 0x71, 0x20,
];
const MASTER_IV: [u8; 16] = [
    0xB1, 0x55, 0xC2, 0x2C, 0x2E, 0x7F, 0x04, 0x91, 0xFA, 0x7F, 0x0F, 0xDC, 0x21, 0x7A, 0xFF, 0x90,
];

/// Each chunk of this size is encrypted on its own, starting from the IV again.
const CHUNK: usize = 0x1000;
const HEAD: usize = 0x40;
const ENTRY: usize = 0x40;

const TYPE_APP: u32 = 0x0001;

/// Reads, decrypts and parses the ICF1 file at `path`.
pub fn parse_icf(path: &Path) -> Result<IcfInfo, IcfError> {
    let cipher = std::fs::read(path)
        .map_err(|error| IcfError::Read { path: path.to_path_buf(), error })?;
    parse_icf_bytes(&decrypt_icf(&cipher)?)
}

fn decrypt_icf(cipher: &[u8]) -> Result<Vec<u8>, IcfError> {
    if cipher.is_empty() || cipher.len() % 16 != 0 {
        return Err(IcfError::NotBlockSized { len: cipher.len() });
    }
    let mut plain = cipher.to_vec();
    for (idx, chunk) in plain.chunks_mut(CHUNK).enumerate() {
        let mut dec = Decryptor::<Aes128>::new_from_slices(&MASTER_KEY, &MASTER_IV)
            .expect("the key and IV are fixed 16-byte values");
        for block in chunk.chunks_exact_mut(16) {
            dec.decrypt_block_mut(GenericArray::from_mut_slice(block));
        }
        let location = (idx * CHUNK) as u64;
        for (i, b) in chunk.iter_mut().take(16).enumerate() {
            *b ^= (location >> ((i % 8) * 8)) as u8;
        }
    }
    Ok(plain)
}

fn parse_icf_bytes(data: &[u8]) -> Result<IcfInfo, IcfError> {
    if data.len() < HEAD {
        return Err(IcfError::TooShort { len: data.len() });
    }

    let crc = crc32fast::hash(&data[4..]);
    if crc != rd_u32(data, 0) {
        return Err(IcfError::HeaderCrc { stored: rd_u32(data, 0), actual: crc });
    }
    let size = rd_u32(data, 4) as usize;
    if size != data.len() {
        return Err(IcfError::SizeMismatch { stored: size, actual: data.len() });
    }
    if rd_u64(data, 8) != 0 {
        return Err(IcfError::NonZeroPadding);
    }
    let count = rd_u64(data, 0x10);
    let expected_len = (count as usize).checked_add(1).and_then(|c| c.checked_mul(ENTRY));
    if expected_len != Some(data.len()) {
        return Err(IcfError::EntryCount { count, expected_len, actual: data.len() });
    }

    let id = std::str::from_utf8(&data[0x18..0x1C]).map_err(|_| IcfError::BadAppId)?;
    if id.is_empty() {
        return Err(IcfError::BadAppId);
    }

    if (0..7).any(|i| rd_u32(data, 0x24 + i * 4) != 0) {
        return Err(IcfError::NonZeroPadding);
    }

    let entries = || data[HEAD..].chunks_exact(ENTRY);
    let is_enabled = |e: &[u8]| e[0] == 2 && e[1] == 1;

    let sub = entries().filter(|e| is_enabled(e)).fold(0, |acc, e| acc ^ crc32fast::hash(e));
    if sub != rd_u32(data, 0x20) {
        return Err(IcfError::EntryCrc { stored: rd_u32(data, 0x20), actual: sub });
    }

    let app = entries()
        .find(|e| is_enabled(e) && rd_u32(e, 4) == TYPE_APP)
        .ok_or(IcfError::NoAppEntry)?;
    let (build, minor, major) = (app[0x20], app[0x21], rd_u16(app, 0x22));
    Ok(IcfInfo { id: id.to_string(), version: format!("{major}.{minor:02}.{build:02}") })
}

fn rd_u16(d: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([d[off], d[off + 1]])
}

fn rd_u32(d: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(d[off..off + 4].try_into().expect("the range read is a fixed 4 bytes"))
}

fn rd_u64(d: &[u8], off: usize) -> u64 {
    u64::from_le_bytes(d[off..off + 8].try_into().expect("the range read is a fixed 8 bytes"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cbc::cipher::BlockEncryptMut;
    use cbc::Encryptor;

    type EntrySpec = (bool, u32, (u16, u8, u8));

    fn build_plain_icf(entries: &[EntrySpec]) -> Vec<u8> {
        let count = entries.len();
        let mut data = vec![0u8; HEAD * (count + 1)];
        data[0x18..0x1C].copy_from_slice(b"SDGT");
        data[0x1C..0x1F].copy_from_slice(b"X64");
        data[0x1F] = 2;
        for (i, (enabled, ty, (major, minor, build))) in entries.iter().enumerate() {
            let off = HEAD + i * ENTRY;
            if *enabled {
                data[off] = 2;
                data[off + 1] = 1;
            }
            data[off + 4..off + 8].copy_from_slice(&ty.to_le_bytes());
            data[off + 0x20] = *build;
            data[off + 0x21] = *minor;
            data[off + 0x22..off + 0x24].copy_from_slice(&major.to_le_bytes());
        }
        let mut sub = 0u32;
        for i in 0..count {
            let e = &data[HEAD + i * ENTRY..HEAD + (i + 1) * ENTRY];
            if e[0] == 2 && e[1] == 1 {
                sub ^= crc32fast::hash(e);
            }
        }
        data[0x10..0x18].copy_from_slice(&(count as u64).to_le_bytes());
        data[0x20..0x24].copy_from_slice(&sub.to_le_bytes());
        let total = data.len() as u32;
        data[4..8].copy_from_slice(&total.to_le_bytes());
        let crc = crc32fast::hash(&data[4..]);
        data[0..4].copy_from_slice(&crc.to_le_bytes());
        data
    }

    fn encrypt_icf_bytes(plain: &[u8]) -> Vec<u8> {
        let mut out = plain.to_vec();
        for (idx, chunk) in out.chunks_mut(CHUNK).enumerate() {
            let location = (idx * CHUNK) as u64;
            for (i, b) in chunk.iter_mut().take(16).enumerate() {
                *b ^= (location >> ((i % 8) * 8)) as u8;
            }
            let mut enc = Encryptor::<Aes128>::new_from_slices(&MASTER_KEY, &MASTER_IV)
                .expect("the key and IV are fixed 16-byte values");
            for block in chunk.chunks_exact_mut(16) {
                enc.encrypt_block_mut(GenericArray::from_mut_slice(block));
            }
        }
        out
    }

    fn temp_icf_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("dnet_icf_test_{}_{}.bin", std::process::id(), name))
    }

    fn write_temp_icf(name: &str, bytes: &[u8]) -> std::path::PathBuf {
        let p = temp_icf_path(name);
        std::fs::write(&p, bytes).expect("failed to write the temporary ICF");
        p
    }

    const SYSTEM: u32 = 0x0000;
    const APP: u32 = 0x0001;

    #[test]
    fn round_trip_sdgt_3_21_00() {

        let plain = build_plain_icf(&[
            (true, SYSTEM, (2, 0, 0)),
            (true, APP, (3, 21, 0)),
        ]);
        let p = write_temp_icf("roundtrip", &encrypt_icf_bytes(&plain));
        let info = parse_icf(&p).expect("the synthetic ICF should parse");
        assert_eq!(info.id, "SDGT");
        assert_eq!(info.version, "3.21.00");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn disabled_app_entry_is_skipped() {
        let plain = build_plain_icf(&[
            (false, APP, (3, 21, 0)),
            (true, APP, (2, 0, 1)),
        ]);
        let info = parse_icf_bytes(&plain).expect("the second enabled APP entry should be found");
        assert_eq!(info.version, "2.00.01");
    }

    #[test]
    fn version_pads_minor_and_build() {
        let plain = build_plain_icf(&[(true, APP, (12, 3, 7))]);
        let info = parse_icf_bytes(&plain).expect("should parse");
        assert_eq!(info.version, "12.03.07");
        let plain = build_plain_icf(&[(true, APP, (3, 21, 5))]);
        assert_eq!(parse_icf_bytes(&plain).unwrap().version, "3.21.05");
    }

    #[test]
    fn multi_chunk_file_round_trips() {

        let mut entries = vec![(true, APP, (3, 21, 0))];
        entries.resize(64, (false, 0xFFFF, (0, 0, 0)));
        let plain = build_plain_icf(&entries);
        assert_eq!(plain.len(), 0x1040);
        let p = write_temp_icf("multichunk", &encrypt_icf_bytes(&plain));
        let info = parse_icf(&p).expect("an ICF spanning blocks should parse");
        assert_eq!(info.version, "3.21.00");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn tampered_cipher_fails_main_crc() {
        let plain = build_plain_icf(&[(true, APP, (3, 21, 0))]);
        let mut cipher = encrypt_icf_bytes(&plain);
        cipher[10] ^= 0xFF;
        let p = write_temp_icf("tampered", &cipher);
        let err = parse_icf(&p).expect_err("a tampered ciphertext should fail");
        assert!(matches!(err, IcfError::HeaderCrc { .. }), "actual error: {err}");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn truncated_data_fails_count_check() {

        let mut plain = build_plain_icf(&[(true, APP, (3, 21, 0))]);
        let cut = plain.len() - ENTRY;
        plain.truncate(cut);
        let total = plain.len() as u32;
        plain[4..8].copy_from_slice(&total.to_le_bytes());
        let crc = crc32fast::hash(&plain[4..]);
        plain[0..4].copy_from_slice(&crc.to_le_bytes());
        let err = parse_icf_bytes(&plain).expect_err("a truncated file should fail");
        assert!(matches!(err, IcfError::EntryCount { .. }), "actual error: {err}");
    }

    #[test]
    fn too_short_data_fails() {
        let err = parse_icf_bytes(&[0u8; 0x20]).expect_err("a file that is too short should fail");
        assert!(matches!(err, IcfError::TooShort { .. }), "actual error: {err}");
    }

    #[test]
    fn non_block_sized_file_fails() {
        let p = write_temp_icf("garbage", &[0xABu8; 100]);
        let err = parse_icf(&p).expect_err("a length that is not a multiple of 16 should fail");
        assert!(matches!(err, IcfError::NotBlockSized { .. }), "actual error: {err}");
        std::fs::remove_file(&p).ok();
    }

    #[test]
    fn no_enabled_app_entry_fails() {
        let plain = build_plain_icf(&[(true, SYSTEM, (2, 0, 0))]);
        let err = parse_icf_bytes(&plain).expect_err("a file without APP entries should fail");
        assert!(matches!(err, IcfError::NoAppEntry), "actual error: {err}");
    }

    #[test]
    fn decrypt_encrypt_round_trip_is_identity() {
        let plain = build_plain_icf(&[(true, APP, (3, 21, 0))]);
        let cipher = encrypt_icf_bytes(&plain);
        assert_eq!(decrypt_icf(&cipher).unwrap(), plain);
    }
}
