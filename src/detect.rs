use std::fs;
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::db::{self, DbMatch};
use crate::storage::Loader;
use rust_i18n::t;

const PE_DOS_MAGIC: u16 = 0x5A4D;
const PE_SIGNATURE: u32 = 0x0000_4550;
const MACHINE_I386: u16 = 0x014C;
const MACHINE_AMD64: u16 = 0x8664;
const SECTION_SIZE: u64 = 40;
const ELF_MAGIC: [u8; 4] = [0x7F, b'E', b'L', b'F'];
const ELFCLASS32: u8 = 1;
const PT_LOAD: u32 = 1;

const KITTEN_CRC_OFFSET: u64 = 10;
const KITTEN_CRC_LEN: u64 = 0x4000;
const CHUNK: usize = 1 << 20;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileKind {
    Pe { machine: u16 },
    Elf32,
    Unknown,
}

impl FileKind {

    pub fn arch(&self) -> &'static str {
        match self {
            FileKind::Pe { machine } if *machine == MACHINE_AMD64 => "x64",
            FileKind::Pe { machine } if *machine == MACHINE_I386 => "x86",
            FileKind::Pe { .. } => "PE",
            FileKind::Elf32 => "x86",
            FileKind::Unknown => "?",
        }
    }
}

#[derive(Clone)]
pub struct IdentifyResult {
    pub kind: FileKind,
    pub crc: u32,
    pub db: Option<DbMatch>,
}

const CARD_READER_FILE: &str = "picodaemon.exe";

fn is_reader_name(name: &std::ffi::OsStr) -> bool {
    name.to_string_lossy()
        .eq_ignore_ascii_case(CARD_READER_FILE)
}

pub fn probe_card_reader(dir: &Path) -> Option<(PathBuf, String)> {
    let entry = fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| is_reader_name(&e.file_name()))?;
    let path = entry.path();
    let crc = identify(&path).ok()?.crc;
    let version = db::match_card_reader(crc)?;
    Some((path, version))
}

pub fn identify(path: &Path) -> io::Result<IdentifyResult> {
    let mut file = File::open(path)?;
    let kind = peek_kind(&mut file)?;
    match kind {
        FileKind::Pe { .. } => {
            let crc = pe_text_crc(&mut file)?;
            Ok(IdentifyResult {
                kind,
                crc,
                db: db::lookup(crc, Loader::Mice),
            })
        }
        FileKind::Elf32 => {
            let crc = elf_crc(&mut file)?;
            Ok(IdentifyResult {
                kind,
                crc,
                db: db::lookup(crc, Loader::Kitten),
            })
        }
        FileKind::Unknown => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            t!("detect.file_is_not_recognizable_pe"),
        )),
    }
}

fn peek_kind(file: &mut File) -> io::Result<FileKind> {
    let mut magic = [0u8; 5];
    read_exact_at(file, &mut magic, 0)?;
    if magic[0..2] == PE_DOS_MAGIC.to_le_bytes() {
        let e_lfanew = read_u32(file, 0x3C)? as u64;
        if read_u32(file, e_lfanew)? == PE_SIGNATURE {
            return Ok(FileKind::Pe {
                machine: read_u16(file, e_lfanew + 4)?,
            });
        }
    }
    if magic[0..4] == ELF_MAGIC {

        if magic[4] == ELFCLASS32 {
            return Ok(FileKind::Elf32);
        }
    }
    Ok(FileKind::Unknown)
}

fn pe_text_crc(file: &mut File) -> io::Result<u32> {
    let e_lfanew = read_u32(file, 0x3C)? as u64;
    let num_sections = read_u16(file, e_lfanew + 6)? as usize;
    let opt_size = read_u16(file, e_lfanew + 20)? as u64;
    let section_table = e_lfanew + 24 + opt_size;

    for i in 0..num_sections {
        let off = section_table + i as u64 * SECTION_SIZE;
        let mut name = [0u8; 8];
        read_exact_at(file, &mut name, off)?;

        let name = name.split(|b| *b == 0).next().unwrap_or(&name[..]);
        if name == b".text" {
            let raw_size = read_u32(file, off + 16)? as u64;
            let raw_ptr = read_u32(file, off + 20)? as u64;
            let mut hasher = crc32fast::Hasher::new();
            hash_file_range(file, &mut hasher, raw_ptr, raw_size)?;
            return Ok(hasher.finalize());
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        t!("detect.no_text_section_found_in"),
    ))
}

fn elf_crc(file: &mut File) -> io::Result<u32> {
    let e_phoff = read_u32(file, 28)? as u64;
    let e_phentsize = read_u16(file, 42)? as u64;
    let e_phnum = read_u16(file, 44)? as usize;

    let mut segs: Vec<(u64, u64, u64)> = Vec::new();
    for i in 0..e_phnum {
        let off = e_phoff + i as u64 * e_phentsize;
        if read_u32(file, off)? == PT_LOAD {
            segs.push((
                read_u32(file, off + 8)? as u64,
                read_u32(file, off + 4)? as u64,
                read_u32(file, off + 16)? as u64,
            ));
        }
    }
    if segs.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            t!("detect.no_pt_load_segment_found"),
        ));
    }
    segs.sort_unstable_by_key(|s| s.0);

    let min_vaddr = segs[0].0;
    let start = min_vaddr + KITTEN_CRC_OFFSET;
    let end = start + KITTEN_CRC_LEN;

    let mut hasher = crc32fast::Hasher::new();
    let mut cursor = start;
    let zeros = vec![0u8; 4096];
    for (vaddr, off, filesz) in &segs {
        if cursor >= end {
            break;
        }
        let seg_start = (*vaddr).max(cursor);
        let seg_end = (vaddr + filesz).min(end);
        if seg_start >= seg_end {
            continue;
        }
        if seg_start > cursor {

            hash_zeros(&mut hasher, &zeros, seg_start - cursor);
        }
        hash_file_range(
            file,
            &mut hasher,
            off + (seg_start - vaddr),
            seg_end - seg_start,
        )?;
        cursor = seg_end;
    }
    if cursor < end {
        hash_zeros(&mut hasher, &zeros, end - cursor);
    }
    Ok(hasher.finalize())
}

fn hash_zeros(hasher: &mut crc32fast::Hasher, zeros: &[u8], mut len: u64) {
    while len > 0 {
        let n = len.min(zeros.len() as u64) as usize;
        hasher.update(&zeros[..n]);
        len -= n as u64;
    }
}

fn hash_file_range(
    file: &mut File,
    hasher: &mut crc32fast::Hasher,
    mut pos: u64,
    mut len: u64,
) -> io::Result<()> {
    let mut buf = vec![0u8; CHUNK];
    while len > 0 {
        let n = len.min(CHUNK as u64) as usize;
        read_exact_at(file, &mut buf[..n], pos)?;
        hasher.update(&buf[..n]);
        pos += n as u64;
        len -= n as u64;
    }
    Ok(())
}

fn read_exact_at(file: &mut File, buf: &mut [u8], off: u64) -> io::Result<()> {
    file.seek(SeekFrom::Start(off))?;
    file.read_exact(buf)
}

fn read_u16(file: &mut File, off: u64) -> io::Result<u16> {
    let mut b = [0u8; 2];
    read_exact_at(file, &mut b, off)?;
    Ok(u16::from_le_bytes(b))
}

fn read_u32(file: &mut File, off: u64) -> io::Result<u32> {
    let mut b = [0u8; 4];
    read_exact_at(file, &mut b, off)?;
    Ok(u32::from_le_bytes(b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn build_pe(text_bytes: &[u8], machine: u16) -> Vec<u8> {
        const OPT_SIZE: u16 = 0xE0;
        let dos_size = 0x40usize;
        let file_size =
            dos_size + 4 + 20 + OPT_SIZE as usize + SECTION_SIZE as usize + text_bytes.len();
        let mut buf = vec![0u8; file_size];

        buf[0..2].copy_from_slice(&PE_DOS_MAGIC.to_le_bytes());
        buf[0x3C..0x40].copy_from_slice(&(dos_size as u32).to_le_bytes());

        let nt = dos_size;
        buf[nt..nt + 4].copy_from_slice(&PE_SIGNATURE.to_le_bytes());
        buf[nt + 4..nt + 6].copy_from_slice(&machine.to_le_bytes());
        buf[nt + 6..nt + 8].copy_from_slice(&1u16.to_le_bytes());
        buf[nt + 20..nt + 22].copy_from_slice(&OPT_SIZE.to_le_bytes());

        let sec = nt + 24 + OPT_SIZE as usize;
        buf[sec..sec + 8].copy_from_slice(b".text\0\0\0");
        let raw_ptr = (sec + SECTION_SIZE as usize) as u32;
        buf[sec + 16..sec + 20].copy_from_slice(&(text_bytes.len() as u32).to_le_bytes());
        buf[sec + 20..sec + 24].copy_from_slice(&raw_ptr.to_le_bytes());
        buf[raw_ptr as usize..].copy_from_slice(text_bytes);
        buf
    }

    #[test]
    fn standard_crc32_vector() {

        let mut h = crc32fast::Hasher::new();
        h.update(b"123456789");
        assert_eq!(h.finalize(), 0xCBF4_3926);
    }

    #[test]
    fn pe_text_crc_matches_plain_crc32() {
        let text: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
        let mut expected = crc32fast::Hasher::new();
        expected.update(&text);
        let expected = expected.finalize();

        for machine in [MACHINE_I386, MACHINE_AMD64] {
            let buf = build_pe(&text, machine);
            let mut f = tempfile("pe_text", &buf);
            let crc = pe_text_crc(&mut f).expect("PE should parse");
            assert_eq!(crc, expected, "machine=0x{machine:04X}");
        }
    }

    #[test]
    fn pe_identify_returns_machine_and_crc() {
        let text = vec![0xAB; 256];
        let buf = build_pe(&text, MACHINE_AMD64);
        let mut f = tempfile("pe_ident", &buf);
        let kind = peek_kind(&mut f).expect("peek");
        assert_eq!(
            kind,
            FileKind::Pe {
                machine: MACHINE_AMD64
            }
        );
        assert_eq!(kind.arch(), "x64");
    }

    fn build_elf(seg_bytes: &[u8]) -> Vec<u8> {
        const PHOFF: usize = 52;
        let mut buf = vec![0u8; 0x100 + seg_bytes.len()];
        buf[0..4].copy_from_slice(&ELF_MAGIC);
        buf[4] = ELFCLASS32;
        buf[16..18].copy_from_slice(&2u16.to_le_bytes());
        buf[28..32].copy_from_slice(&(PHOFF as u32).to_le_bytes());
        buf[42..44].copy_from_slice(&32u16.to_le_bytes());
        buf[44..46].copy_from_slice(&1u16.to_le_bytes());

        buf[PHOFF..PHOFF + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[PHOFF + 4..PHOFF + 8].copy_from_slice(&0x100u32.to_le_bytes());
        buf[PHOFF + 8..PHOFF + 12].copy_from_slice(&0x8000_0000u32.to_le_bytes());
        buf[PHOFF + 16..PHOFF + 20].copy_from_slice(&(seg_bytes.len() as u32).to_le_bytes());
        buf[PHOFF + 20..PHOFF + 24].copy_from_slice(&(seg_bytes.len() as u32).to_le_bytes());
        buf[0x100..].copy_from_slice(seg_bytes);
        buf
    }

    #[test]
    fn elf_crc_matches_kitten_mapping() {

        let seg: Vec<u8> = (0..0x6000u32).map(|i| (i * 7 % 256) as u8).collect();
        let buf = build_elf(&seg);
        let mut f = tempfile("elf_crc", &buf);

        let mut expected = crc32fast::Hasher::new();
        expected.update(&seg[0xA..0xA + 0x4000]);
        let expected = expected.finalize();

        assert_eq!(elf_crc(&mut f).expect("ELF should parse"), expected);
    }

    #[test]
    fn elf_crc_gap_handles_zeros() {

        const PHOFF: usize = 52;
        let seg1 = vec![0x11; 0x100];
        let seg2 = vec![0x22; 0x100];
        let gap = 0x200usize;
        let mut buf = vec![0u8; 0x400];
        buf[0..4].copy_from_slice(&ELF_MAGIC);
        buf[4] = ELFCLASS32;
        buf[28..32].copy_from_slice(&(PHOFF as u32).to_le_bytes());
        buf[42..44].copy_from_slice(&32u16.to_le_bytes());
        buf[44..46].copy_from_slice(&2u16.to_le_bytes());
        let p1 = PHOFF;
        buf[p1..p1 + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[p1 + 4..p1 + 8].copy_from_slice(&0x100u32.to_le_bytes());
        buf[p1 + 8..p1 + 12].copy_from_slice(&0x8000_0000u32.to_le_bytes());
        buf[p1 + 16..p1 + 20].copy_from_slice(&(seg1.len() as u32).to_le_bytes());
        buf[p1 + 20..p1 + 24].copy_from_slice(&(seg1.len() as u32).to_le_bytes());
        let p2 = PHOFF + 32;
        let seg2_vaddr = 0x8000_0000u32 + 0x100 + gap as u32;
        let seg2_file = 0x100 + 0x100 + 0x100;
        buf[p2..p2 + 4].copy_from_slice(&PT_LOAD.to_le_bytes());
        buf[p2 + 4..p2 + 8].copy_from_slice(&(seg2_file as u32).to_le_bytes());
        buf[p2 + 8..p2 + 12].copy_from_slice(&seg2_vaddr.to_le_bytes());
        buf[p2 + 16..p2 + 20].copy_from_slice(&(seg2.len() as u32).to_le_bytes());
        buf[p2 + 20..p2 + 24].copy_from_slice(&(seg2.len() as u32).to_le_bytes());
        buf[0x100..0x200].copy_from_slice(&seg1);
        buf[seg2_file..seg2_file + 0x100].copy_from_slice(&seg2);

        let mut f = tempfile("elf_gap", &buf);
        let crc = elf_crc(&mut f).expect("ELF should parse");

        let mut stream = Vec::new();
        stream.extend_from_slice(&seg1);
        stream.extend_from_slice(&vec![0u8; gap]);
        stream.extend_from_slice(&seg2);

        let s = &stream[10..(10 + 0x4000).min(stream.len())];
        let mut pad = Vec::new();
        if s.len() < 0x4000 {
            pad.extend_from_slice(&vec![0u8; 0x4000 - s.len()]);
        }
        let mut expected = crc32fast::Hasher::new();
        expected.update(s);
        expected.update(&pad);
        assert_eq!(crc, expected.finalize());
    }

    #[test]
    fn reader_filename_is_case_insensitive() {
        use std::ffi::OsStr;
        assert!(is_reader_name(OsStr::new("picodaemon.exe")));
        assert!(is_reader_name(OsStr::new("PicoDaemon.EXE")));
        assert!(!is_reader_name(OsStr::new("game.exe")));
        assert!(!is_reader_name(OsStr::new("picodaemon")));
    }

    #[test]
    fn probe_card_reader_absent_dir_returns_none() {
        let dir = std::env::temp_dir().join(format!("launcher_probe_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        assert!(probe_card_reader(&dir).is_none());

        std::fs::write(dir.join("picodaemon.exe"), b"not an exe").unwrap();
        assert!(probe_card_reader(&dir).is_none());
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn tempfile(name: &str, bytes: &[u8]) -> File {
        let mut path = std::env::temp_dir();
        path.push(format!("launcher_test_{}_{}.bin", std::process::id(), name));
        let mut f = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(true)
            .open(&path)
            .expect("create temporary file");
        f.write_all(bytes).expect("write temporary file");
        f
    }
}

#[cfg(test)]
mod real_world_smoke {
    use super::*;

    #[test]
    fn identify_real_pe_exe() {

        let p = Path::new(r"E:\vsproject\kittentools\win-release\kitten.exe");
        if !p.is_file() {
            eprintln!("skipped: kitten.exe not found");
            return;
        }
        let r = identify(p).expect("kitten.exe should be identified");
        assert!(matches!(r.kind, FileKind::Pe { .. }));
        assert_ne!(r.crc, 0);
        assert!(r.db.is_none(), "kitten.exe itself should not match the game database");
        println!("kitten.exe crc = 0x{:08x}, arch = {}", r.crc, r.kind.arch());
    }
}
