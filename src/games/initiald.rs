use std::fs;
use std::path::{Path, PathBuf};

use rust_i18n::t;

use crate::db;
use crate::storage::GameRecord;

const INI_NAME: &str = "initialD.ini";

const NEWS_PORT: u16 = 8086;

fn host_or_default(host: &str) -> &str {
    let h = host.trim();
    if h.is_empty() {
        crate::server::SERVER_HOST
    } else {
        h
    }
}

#[cfg(test)]
pub fn supports(game_id: Option<&str>) -> bool {
    game_id.and_then(idas_version).is_some()
}

fn idas_version(game_id: &str) -> Option<u8> {
    match game_id {
        "SBML" => Some(4),
        "SBQZ" | "SBTS" => Some(5),
        "SBUU" | "SBVW" | "SBWV" => Some(6),
        "SBYD" | "SBYY" => Some(7),
        _ => None,
    }
}

/// Rewrites `initialD.ini` in the game's folder for the server `host`, when the game is
/// one of D4–D7 (known by the database's game id, or else the record's own).
pub fn patch_for(record: &GameRecord, host: &str) -> Result<bool, String> {
    let game_id = db::lookup_record(record)
        .and_then(|m| m.game_id.clone())
        .or_else(|| record.game_id.clone());
    patch_game_ini(&super::game_dir_of(record), game_id.as_deref(), host)
}

pub fn patch_game_ini(game_dir: &Path, game_id: Option<&str>, host: &str) -> Result<bool, String> {
    let Some(idas) = game_id.and_then(idas_version) else {
        return Ok(false);
    };
    let Some(path) = find_ini(game_dir) else {
        return Ok(false);
    };
    let src = match fs::read(&path) {
        Ok(b) => b,
        Err(_) => return Ok(false),
    };
    let out = patch_bytes(&src, idas, host);
    if out == src {
        return Ok(false);
    }

    let tmp = path.with_extension("ini.tmp");
    fs::write(&tmp, &out)
        .map_err(|e| t!("initiald.write_temp_failed", path = tmp.display(), error = e).to_string())?;
    fs::rename(&tmp, &path).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        t!("initiald.replace_failed", path = path.display(), error = e).to_string()
    })?;
    Ok(true)
}

fn find_ini(game_dir: &Path) -> Option<PathBuf> {
    let entries = fs::read_dir(game_dir).ok()?;
    for e in entries.flatten() {
        if e.file_name().to_string_lossy().eq_ignore_ascii_case(INI_NAME) {
            return Some(e.path());
        }
    }
    None
}

pub fn patch_bytes(src: &[u8], idas: u8, host: &str) -> Vec<u8> {
    let host = host_or_default(host);
    let url = format!("http://{host}:{NEWS_PORT}/idas{idas}/news/news50.txt");
    let mut out = Vec::with_capacity(src.len());
    let mut i = 0;
    while i < src.len() {

        let nl = src[i..].iter().position(|b| *b == b'\n').map(|p| i + p);
        let line_end = nl.unwrap_or(src.len());
        let line = &src[i..line_end];
        out.extend_from_slice(&patch_line(line, host, &url));
        match nl {
            Some(n) => {
                out.push(b'\n');
                i = n + 1;
            }
            None => i = src.len(),
        }
    }
    out
}

fn patch_line(line: &[u8], host: &str, url: &str) -> Vec<u8> {
    let cr = line.last() == Some(&b'\r');
    let content: &[u8] = if cr { &line[..line.len() - 1] } else { line };

    let trimmed = trim_ascii(content);
    if trimmed.is_empty()
        || trimmed.first() == Some(&b';')
        || trimmed.first() == Some(&b'#')
        || trimmed.first() == Some(&b'[')
    {
        return line.to_vec();
    }

    let Some(eq) = content.iter().position(|b| *b == b'=') else {
        return line.to_vec();
    };
    let key = trim_ascii(&content[..eq]);
    let key_str = match std::str::from_utf8(key) {
        Ok(s) => s,
        Err(_) => return line.to_vec(),
    };
    match new_value(key_str, host, url) {
        Some(v) => {

            let mut o = Vec::with_capacity(content.len());
            o.extend_from_slice(&content[..eq]);
            o.extend_from_slice(b"= ");
            o.extend_from_slice(v.as_bytes());
            if cr {
                o.push(b'\r');
            }
            o
        }
        None => line.to_vec(),
    }
}

fn new_value(key: &str, host: &str, url: &str) -> Option<String> {
    if key.starts_with("TAG_MATCH_SERVER_PORT_UDP_SEND") {
        Some("12353".to_string())
    } else if key.starts_with("TAG_MATCH_SERVER_PORT_UDP_RECV") {
        Some("12354".to_string())
    } else if key.contains("SERVER_ADDR") {

        Some(host.to_string())
    } else if key.starts_with("MATCH_SERVER_PORT_TCP") {
        Some("10003".to_string())
    } else if key.starts_with("MATCH_SERVER_PORT_UDP_SEND") {
        Some("12353".to_string())
    } else if key.starts_with("MATCH_SERVER_PORT_UDP_RECV") {
        Some("12354".to_string())
    } else if key.starts_with("USERDB_SERVER_PORT_TCP") {
        Some("10000".to_string())
    } else if key.starts_with("EVENT_SERVER_PORT_TCP") {
        Some("10000".to_string())
    } else if key.starts_with("ADV_NEWS_SERVER_URL") {
        Some(url.to_string())
    } else {
        None
    }
}

fn trim_ascii(mut b: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = b {
        if first.is_ascii_whitespace() {
            b = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = b {
        if last.is_ascii_whitespace() {
            b = rest;
        } else {
            break;
        }
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: &str = "sdgt.arcadehub.xyz";

    fn sample() -> Vec<u8> {
        ";; 注释\n\
         ;MATCH_SERVER_ADDR_0 = old\n\
         MATCH_SERVER_ADDR_0     = 1.2.3.4\n\
         MATCH_SERVER_ADDR_3     = linux.am3\n\
         USERDB_SERVER_ADDR_2    = id5001.am3.sega.co.jp\n\
         EVENT_SERVER_ADDR_1     = 203.179.83.139\n\
         SCREENSHOT_SERVER_ADDR_2 = a\n\
         SCREENSHOT_SERVER_ADDR_2 = b\n\
         MATCH_SERVER_PORT_TCP_0   = 7766\n\
         MATCH_SERVER_PORT_TCP_15  = 7766\n\
         MATCH_SERVER_PORT_UDP_SEND_8 = 12354\n\
         MATCH_SERVER_PORT_UDP_RECV_8 = 12353\n\
         USERDB_SERVER_PORT_TCP_5  = 22876\n\
         EVENT_SERVER_PORT_TCP_5   = 24029\n\
         ADV_NEWS_SERVER_URL_0 = http://id5-patch01/news/news50.txt\n\
         TAG_MATCH_SERVER_PORT_TCP_8 = 27666\n\
         TAG_MATCH_SERVER_PORT_UDP_SEND_8 = 23353\n\
         TAG_MATCH_SERVER_PORT_UDP_RECV_8 = 23354\n\
         STORE_LIVECAST_ENABLE = 1\n\
         CARDB_SERVER_ADDR = 157.109.70.97\n"
            .as_bytes()
            .to_vec()
    }

    #[test]
    fn applies_all_rules() {
        let out = patch_bytes(&sample(), 5, H);
        let s = String::from_utf8(out).unwrap();

        assert!(s.contains("MATCH_SERVER_ADDR_0     = sdgt.arcadehub.xyz"));
        assert!(s.contains("MATCH_SERVER_ADDR_3     = sdgt.arcadehub.xyz"));
        assert!(s.contains("USERDB_SERVER_ADDR_2    = sdgt.arcadehub.xyz"));
        assert!(s.contains("EVENT_SERVER_ADDR_1     = sdgt.arcadehub.xyz"));
        assert!(s.contains("SCREENSHOT_SERVER_ADDR_2 = sdgt.arcadehub.xyz"));

        assert!(s.contains("MATCH_SERVER_PORT_TCP_0   = 10003"));
        assert!(s.contains("MATCH_SERVER_PORT_TCP_15  = 10003"));
        assert!(s.contains("MATCH_SERVER_PORT_UDP_SEND_8 = 12353"));
        assert!(s.contains("MATCH_SERVER_PORT_UDP_RECV_8 = 12354"));
        assert!(s.contains("USERDB_SERVER_PORT_TCP_5  = 10000"));
        assert!(s.contains("EVENT_SERVER_PORT_TCP_5   = 10000"));

        assert!(s.contains("ADV_NEWS_SERVER_URL_0 = http://sdgt.arcadehub.xyz:8086/idas5/news/news50.txt"));
    }

    #[test]
    fn tag_udp_changed_tcp_unchanged() {
        let out = patch_bytes(&sample(), 5, H);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("TAG_MATCH_SERVER_PORT_UDP_SEND_8 = 12353"));
        assert!(s.contains("TAG_MATCH_SERVER_PORT_UDP_RECV_8 = 12354"));

        assert!(s.contains("TAG_MATCH_SERVER_PORT_TCP_8 = 27666"));
    }

    #[test]
    fn keeps_comments_alignment_and_other_keys() {
        let out = patch_bytes(&sample(), 5, H);
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(";; 注释"));
        assert!(s.contains(";MATCH_SERVER_ADDR_0 = old"));
        assert!(s.contains("STORE_LIVECAST_ENABLE = 1"));

        assert!(s.contains("CARDB_SERVER_ADDR = sdgt.arcadehub.xyz"));
    }

    #[test]
    fn idas_version_mapping_and_supports() {
        assert_eq!(idas_version("SBML"), Some(4));
        assert_eq!(idas_version("SBQZ"), Some(5));
        assert_eq!(idas_version("SBTS"), Some(5));
        assert_eq!(idas_version("SBWV"), Some(6));
        assert_eq!(idas_version("SBYY"), Some(7));
        assert_eq!(idas_version("SBZZ"), None);
        assert_eq!(idas_version("SDAL"), None);
        assert!(supports(Some("SBQZ")));
        assert!(!supports(Some("SBZZ")));
        assert!(!supports(None));
        assert!(!supports(Some("UNKNOWN")));
    }

    #[test]
    fn idempotent() {
        let once = patch_bytes(&sample(), 5, H);
        let twice = patch_bytes(&once, 5, H);
        assert_eq!(once, twice);
    }

    #[test]
    fn crlf_preserved() {
        let src = b"MATCH_SERVER_ADDR_0 = x\r\nMATCH_SERVER_PORT_TCP_0 = 1\r\n";
        let out = patch_bytes(src, 5, H);
        assert_eq!(
            out,
            b"MATCH_SERVER_ADDR_0 = sdgt.arcadehub.xyz\r\nMATCH_SERVER_PORT_TCP_0 = 10003\r\n"
        );
    }

    #[test]
    fn patch_game_ini_missing_file_or_unsupported() {
        let tmp =
            std::env::temp_dir().join(format!("initiald_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();

        assert_eq!(patch_game_ini(&tmp, Some("SBQZ"), H).unwrap(), false);

        let p = tmp.join("initialD.ini");
        fs::write(&p, "MATCH_SERVER_ADDR_0 = x\n").unwrap();
        assert_eq!(patch_game_ini(&tmp, Some("SBZZ"), H).unwrap(), false);
        assert_eq!(fs::read_to_string(&p).unwrap(), "MATCH_SERVER_ADDR_0 = x\n");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn patch_game_ini_supported_writes() {
        let tmp =
            std::env::temp_dir().join(format!("initiald_test2_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        let p = tmp.join("initialD.ini");
        fs::write(&p, "MATCH_SERVER_ADDR_0 = x\n").unwrap();
        assert_eq!(patch_game_ini(&tmp, Some("SBQZ"), H).unwrap(), true);
        assert!(fs::read_to_string(&p).unwrap().contains(H));
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn custom_server_host_used() {
        let out = patch_bytes(&sample(), 6, "192.168.100.114");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains("MATCH_SERVER_ADDR_0     = 192.168.100.114"));
        assert!(s.contains("USERDB_SERVER_ADDR_2    = 192.168.100.114"));
        assert!(s.contains("CARDB_SERVER_ADDR = 192.168.100.114"));
        assert!(s.contains(
            "ADV_NEWS_SERVER_URL_0 = http://192.168.100.114:8086/idas6/news/news50.txt"
        ));
        assert!(!s.contains(H), "the default private server address should not remain");
    }

    #[test]
    fn blank_host_falls_back_to_default() {
        let out = patch_bytes(&sample(), 5, "  ");
        let s = String::from_utf8(out).unwrap();
        assert!(s.contains(&format!("MATCH_SERVER_ADDR_0     = {}", crate::server::SERVER_HOST)));
        assert!(s.contains(&format!("http://{}:8086/idas5", crate::server::SERVER_HOST)));
    }
}
