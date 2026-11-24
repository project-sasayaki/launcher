//! The ServerBox of Initial D Arcade Stage 4–8: the program that plays the in-store
//! server. It is started by hand from the game row, and while the network is not
//! simulated it needs an address of its own on the PC's adapter.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use rust_i18n::t;

use super::{LaunchInput, gfx_patch};
use crate::loaders::mice;
use crate::netinfo;
use crate::storage::GameRecord;
use crate::win_util::{self, CREATE_NO_WINDOW};

/// The folder of the game's ServerBox, if one is set and exists.
pub fn dir_of(record: &GameRecord) -> Option<PathBuf> {
    let sb = record.serverbox.as_deref().map(str::trim).filter(|s| !s.is_empty())?;
    let p = Path::new(sb);
    if !p.is_file() {
        return None;
    }
    let dir = p.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    Some(if dir.as_os_str().is_empty() { PathBuf::from(".") } else { dir })
}

/// The file name of the game's ServerBox, the process to end when the game exits.
pub fn image_name(record: &GameRecord) -> Option<String> {
    let path = Path::new(record.serverbox.as_deref()?);
    path.file_name().map(|n| n.to_string_lossy().into_owned())
}

/// The game's record turned into its ServerBox's: the same game, with the ServerBox as the
/// file to run and never in test mode.
pub fn record_of(record: &GameRecord) -> Result<GameRecord, String> {
    let sb_path = record
        .serverbox
        .as_deref()
        .ok_or_else(|| t!("launch.this_game_has_no_serverbox").to_string())?;
    if !Path::new(sb_path).is_file() {
        return Err(t!("launch.serverbox_file_does_not_exist", sb_path = sb_path).to_string());
    }
    let mut sb_rec = record.clone();
    sb_rec.path = sb_path.to_string();
    sb_rec.test_mode = None;
    Ok(sb_rec)
}

/// Puts the graphics patch into the ServerBox folder (or takes it out, by the game row's
/// switch) and returns the ServerBox's record and folder for the loader to fill in.
pub fn prepare(record: &GameRecord) -> Result<(GameRecord, PathBuf), String> {
    let sb_rec = record_of(record)?;
    let sb_dir = dir_of(&sb_rec).unwrap_or_else(|| PathBuf::from("."));
    let enabled = sb_rec.gfx_patch.unwrap_or(true);
    if enabled {
        let tools = crate::update::tools_dir();
        if gfx_patch::find_asi_loader(&tools).is_none() {
            let dll = tools.join("asi-loader").join("dinput8.dll");
            return Err(t!("launch.ultimate_asi_loader_is_not", path = dll.display()).to_string());
        }
    }
    gfx_patch::deploy(&sb_dir, enabled)?;
    Ok((sb_rec, sb_dir))
}

/// The ServerBox address added to an adapter for one game session.
#[derive(Clone, Debug)]
pub struct StaticIp {
    pub adapter: String,
    pub ip: String,
}

impl StaticIp {
    /// Takes the address off the adapter again.
    pub fn remove(&self) -> Result<(), String> {
        remove_static_ip(&self.adapter, &self.ip)
    }
}

/// Gives the chosen adapter the ServerBox address when the game needs it: the network is
/// not simulated, this PC is not in a room, and the adapter has fewer than two addresses.
pub fn add_static_ip_if_needed(input: &LaunchInput) -> Result<Option<StaticIp>, String> {
    let serverbox_enabled = input.net.simulate == Some(false) && input.room.is_none();
    if !serverbox_enabled {
        return Ok(None);
    }
    let ip = input.net.serverbox_ip.as_deref().map(str::trim).filter(|s| !s.is_empty());
    let (Some(ip), Some(adapter)) = (ip, input.serverbox_adapter.as_deref()) else {
        return Ok(None);
    };
    if adapter == crate::awl::AWL_IFACE_NAME {
        return Ok(None);
    }
    let Some(current) = netinfo::list_adapters().into_iter().find(|a| a.friendly == adapter) else {
        return Ok(None);
    };
    if current.ips.len() >= 2 {
        return Ok(None);
    }
    let mask = current
        .ips
        .first()
        .map(|ip| mice::format_ipv4(mice::mask_of(ip.prefix)))
        .unwrap_or_else(|| "255.255.255.0".to_string());
    add_static_ip(adapter, ip, &mask)?;
    Ok(Some(StaticIp { adapter: adapter.to_string(), ip: ip.to_string() }))
}

/// Gives the adapter `friendly` the address `ip` as a second, static address next to the
/// one DHCP gave it. An address that is there already counts as added.
fn add_static_ip(friendly: &str, ip: &str, mask: &str) -> Result<(), String> {
    let interface = format!("interface=\"{friendly}\"");
    let coexistence =
        run_netsh(&["interface", "ipv4", "set", "interface", &interface, "dhcpstaticipcoexistence=enabled"])?;
    if !coexistence.success {
        return Err(t!("launch.failed_to_enable_dhcp_static", output = coexistence.output).to_string());
    }

    let add = run_netsh(&["interface", "ipv4", "add", "address", &format!("\"{friendly}\""), ip, mask])?;
    // netsh also fails when the address is there already. Its message is in the user's
    // display language, so look at the adapter instead.
    if add.success || netinfo::adapter_has_ip(friendly, ip) {
        Ok(())
    } else {
        Err(t!("launch.failed_to_add_static_ip", output = add.output).to_string())
    }
}

/// Takes the address `ip` off the adapter `friendly`. An address (or an adapter) that is
/// gone already counts as removed.
fn remove_static_ip(friendly: &str, ip: &str) -> Result<(), String> {
    let delete = run_netsh(&["interface", "ipv4", "delete", "address", &format!("\"{friendly}\""), ip])?;
    if delete.success || !netinfo::adapter_has_ip(friendly, ip) {
        Ok(())
    } else {
        Err(t!("launch.failed_to_remove_static_ip", output = delete.output).to_string())
    }
}

/// What a netsh run reported: whether it succeeded, and what it wrote.
struct NetshResult {
    success: bool,
    output: String,
}

/// Runs netsh without opening a console window.
fn run_netsh(args: &[&str]) -> Result<NetshResult, String> {
    let out = Command::new("netsh")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| t!("launch.failed_to_run_netsh", error = e).to_string())?;
    let output = win_util::decode_oem(&out.stdout) + &win_util::decode_oem(&out.stderr);
    Ok(NetshResult { success: out.status.success(), output: output.trim().to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_util::test_dir::{fresh, remove, write};
    use crate::storage::Loader;

    fn record_with_serverbox(serverbox: Option<String>) -> GameRecord {
        GameRecord {
            id: "gfx".to_string(),
            path: String::new(),
            name: "gfx".to_string(),
            crc: None,
            version: None,
            arch: None,
            loader: Loader::Mice,
            platform: None,
            image: None,
            cover: None,
            serverbox,
            reader: None,
            amdaemon: None,
            game_exe: None,
            game_id: None,
            gfx_patch: None,
            test_mode: None,
            card: None,
            region: None,
            play_count: 0,
            last_played: None,
            added_at: 0,
        }
    }

    #[test]
    fn dir_of_variants() {
        assert!(dir_of(&record_with_serverbox(None)).is_none());
        assert!(dir_of(&record_with_serverbox(Some(String::new()))).is_none());
        assert!(dir_of(&record_with_serverbox(Some("  ".to_string()))).is_none());
        let sb = fresh("sb_variants");

        let missing = sb.join("nope.exe").to_string_lossy().into_owned();
        assert!(dir_of(&record_with_serverbox(Some(missing))).is_none());

        write(&sb.join("serverbox.exe"), b"sb");
        let ok = sb.join("serverbox.exe").to_string_lossy().into_owned();
        assert_eq!(dir_of(&record_with_serverbox(Some(ok))), Some(sb.clone()));
        remove(&sb);
    }

    #[test]
    fn image_name_is_the_file_name() {
        let rec = record_with_serverbox(Some(r"D:\games\id6\ServerBoxD6.exe".to_string()));
        assert_eq!(image_name(&rec).as_deref(), Some("ServerBoxD6.exe"));
        assert_eq!(image_name(&record_with_serverbox(None)), None);
    }
}
