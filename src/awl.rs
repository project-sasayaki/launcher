use std::io::Read;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use serde_json::json;

use crate::roomdir::{JoinInfo, RoomDirClient};
use crate::storage::RoomConfig;
use rust_i18n::t;

use crate::win_util::CREATE_NO_WINDOW;
const LOCAL_API_HOST: &str = "127.0.0.1";
const LOCAL_API_PORT: u16 = 8639;
const AWL_VERSION_EXPECT: &str = "v0.19.0-mp2";

const LOCAL_API_READY_TIMEOUT: Duration = Duration::from_secs(60);
const LINK_TIMEOUT: Duration = Duration::from_secs(30);

const VERSION_CHECK_TIMEOUT: Duration = Duration::from_secs(10);

pub const HOST_GAME_IP: &str = "172.10.1.101";
pub const HOST_SERVERBOX_IP: &str = "172.10.1.102";

pub const ROOM_CIDR: &str = "172.10.1.0/24";

pub const ROOM_SYNC_PORT: u16 = 27510;

pub const AWL_IFACE_NAME: &str = "awl0";
const AWL_IFACE: &str = AWL_IFACE_NAME;

pub fn room_host_needs_serverbox_ip(game_id: &str) -> bool {
    !matches!(game_id, "SDDF" | "SDER" | "SDGT")
}

#[derive(Clone, PartialEq, Debug)]
pub enum RoomStatus {

    Idle,

    Creating { name: String },

    Hosting { room_id: String, name: String },

    Joining { room_id: String },

    Joined { room_id: String, is_host: bool },
}

#[derive(Clone, Debug)]
pub struct PeerView {
    pub peer_id: String,
    pub alias: String,
    pub ip: String,
    pub connected: bool,
    pub confirmed: bool,
    pub ping_ms: f64,
    pub through_relay: bool,
}

pub struct AwlHandle {
    child: Child,
    job: Option<usize>,
}

const QUIT_WAIT_TIMEOUT: Duration = Duration::from_secs(5);

impl Drop for AwlHandle {
    fn drop(&mut self) {

        let quit_sent = AwlManager::quit_request();
        if quit_sent {
            let deadline = Instant::now() + QUIT_WAIT_TIMEOUT;
            while Instant::now() < deadline {
                if matches!(self.child.try_wait(), Ok(Some(_))) {
                    break;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
        crate::vpn::close_job(self.job);
    }
}

enum SpawnWaitFail {
    Exited(String),
    NotReady,
}

pub struct AwlManager;

impl AwlManager {

    pub fn exe_path(cfg: &RoomConfig) -> PathBuf {
        if let Some(p) = &cfg.awl_path {
            let p = PathBuf::from(p);
            if p.is_file() {
                return p;
            }
        }
        crate::storage::launcher_dir().join("tools").join("awl").join("awl-tray.exe")
    }

    fn data_dir() -> PathBuf {
        crate::storage::data_dir().join("awl")
    }

    fn config_path() -> PathBuf {
        Self::data_dir().join("config_awl.json")
    }

    fn read_json_config() -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(Self::config_path()).ok()?;
        serde_json::from_str(&text).ok()
    }

    pub fn my_peer_id() -> Result<String, String> {
        let v = Self::read_json_config().ok_or_else(|| {
            t!("awl.awl_identity_not_initialized").to_string()
        })?;
        v.pointer("/p2pNode/peerId")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| t!("awl.awl_identity_missing").to_string())
    }

    fn has_identity() -> bool {
        Self::read_json_config()
            .and_then(|v| {
                v.pointer("/p2pNode/identity")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
            })
            .unwrap_or(false)
    }

    fn write_config(ip_net: &str, ip_net2: Option<&str>, disable_tun: bool) -> Result<(), String> {
        let dir = Self::data_dir();
        std::fs::create_dir_all(&dir).map_err(|e| {
            t!("awl.failed_to_create_awl_data", error = e).to_string()
        })?;
        let old = Self::read_json_config();
        let identity = old
            .as_ref()
            .and_then(|v| v.pointer("/p2pNode/identity").and_then(|v| v.as_str()))
            .unwrap_or("");
        let peer_id = old
            .as_ref()
            .and_then(|v| v.pointer("/p2pNode/peerId").and_then(|v| v.as_str()))
            .unwrap_or("");

        let mut p2p = json!({
            "name": "sasayaki",
            "autoAcceptAuthRequests": true,
            "bootstrapPeers": [],
            "listenAddresses": [],
            "reconnectionIntervalSec": 20,
        });
        if !identity.is_empty() {
            p2p["identity"] = json!(identity);
        }
        if !peer_id.is_empty() {
            p2p["peerId"] = json!(peer_id);
        }
        let mut vpn = json!({
            "interfaceName": AWL_IFACE,
            "ipNet": ip_net,
            "disableVPNInterface": disable_tun,
        });
        if let Some(ip_net2) = ip_net2 {
            vpn["ipNet2"] = json!(ip_net2);
        }
        let cfg = json!({
            "loggerLevel": "info",
            "httpListenAddress": format!("{LOCAL_API_HOST}:{LOCAL_API_PORT}"),
            "p2pNode": p2p,
            "vpn": vpn,

            "socks5": {
                "listenerEnabled": false,
                "proxyingEnabled": false,
                "listenAddress": "127.0.0.66:8080",
            },
            "dns": { "disableDNS": true },
        });
        std::fs::write(Self::config_path(), serde_json::to_string_pretty(&cfg).unwrap())
            .map_err(|e| t!("awl.failed_to_write_awl_config", error = e).to_string())
    }

    fn local_request(verb: &str, path: &str, body: Option<&str>) -> Result<(u16, String), String> {
        let mut stream = TcpStream::connect((LOCAL_API_HOST, LOCAL_API_PORT))
            .map_err(|e| t!("awl.cannot_reach_awl_api", error = e).to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let body = body.unwrap_or("");
        let req = format!(
            "{verb} {path} HTTP/1.0\r\nHost: {LOCAL_API_HOST}:{LOCAL_API_PORT}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
            body.len()
        );
        use std::io::Write;
        stream
            .write_all(req.as_bytes())
            .map_err(|e| t!("awl.failed_to_send_awl_api", error = e).to_string())?;
        let mut resp = Vec::new();
        stream
            .read_to_end(&mut resp)
            .map_err(|e| t!("awl.failed_to_read_awl_api", error = e).to_string())?;
        let text = String::from_utf8_lossy(&resp);
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let status: u16 = head
            .split_whitespace()
            .nth(1)
            .and_then(|s| s.parse().ok())
            .ok_or_else(|| t!("awl.unexpected_awl_api_response").to_string())?;
        Ok((status, body.to_string()))
    }

    fn quit_request() -> bool {
        matches!(
            Self::local_request("POST", "/api/v0/app/quit", None),
            Ok((200, _))
        )
    }

    pub fn version_check(exe: &Path) -> Result<(), String> {
        let mut child = Command::new(exe)
            .arg("-v")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| t!("awl.cannot_execute_awl_tray_exe", error = e).to_string())?;
        let deadline = Instant::now() + VERSION_CHECK_TIMEOUT;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(100));
                }
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(t!("awl.awl_tray_exe_v_did")
                    .to_string());
                }
                Err(e) => {
                    return Err(t!("awl.cannot_execute_awl_tray_exe", error = e).to_string())
                }
            }
        }
        let out = child
            .wait_with_output()
            .map_err(|e| t!("awl.cannot_execute_awl_tray_exe", error = e).to_string())?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if text.contains(AWL_VERSION_EXPECT) {
            Ok(())
        } else {
            Err(t!("awl.awl_tray_exe_version_mismatch", awl_version_expect = AWL_VERSION_EXPECT, text = text.trim()).to_string())
        }
    }

    fn ensure_firewall(exe: &Path) {
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule", "name=Sasayaki Awl"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule", "name=Sasayaki Awl",
                "dir=in", "action=allow", "enable=yes",
                &format!("program={}", exe.display()),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("netsh")
            .args(["advfirewall", "firewall", "delete", "rule", "name=Sasayaki Awl ICMP"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        let _ = Command::new("netsh")
            .args([
                "advfirewall", "firewall", "add", "rule", "name=Sasayaki Awl ICMP",
                "protocol=icmpv4:8,any", "dir=in", "action=allow",
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }

    fn ps_ip_present(ip: &str, iface: &str) -> bool {
        let out = Command::new("powershell")
            .args([
                "-NoProfile",
                "-Command",
                &format!(
                    "if(Get-NetIPAddress -IPAddress {ip} -InterfaceAlias {iface} -ErrorAction SilentlyContinue){{'1'}}else{{'0'}}"
                ),
            ])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        matches!(&out, Ok(o) if String::from_utf8_lossy(&o.stdout).trim() == "1")
    }

    fn add_unicast_ip(iface: &str, ip: &str, prefix_len: u8) -> bool {
        use windows::Win32::NetworkManagement::IpHelper::{
            ConvertInterfaceAliasToLuid, CreateUnicastIpAddressEntry, InitializeUnicastIpAddressEntry,
            MIB_UNICASTIPADDRESS_ROW,
        };
        use windows::Win32::NetworkManagement::Ndis::NET_LUID_LH;
        use windows::core::PCWSTR;

        let octets: [u8; 4] = match ip.split('.').map(|o| o.parse::<u8>()).collect::<Result<Vec<_>, _>>() {
            Ok(v) if v.len() == 4 => [v[0], v[1], v[2], v[3]],
            _ => return false,
        };
        unsafe {
            let alias_w = crate::win_util::to_wide_null(iface);
            let mut luid = NET_LUID_LH::default();
            if ConvertInterfaceAliasToLuid(PCWSTR::from_raw(alias_w.as_ptr()), &mut luid).is_err() {
                return false;
            }
            let mut row = MIB_UNICASTIPADDRESS_ROW::default();
            InitializeUnicastIpAddressEntry(&mut row);
            row.InterfaceLuid = luid;

            let mut sinet = windows::Win32::Networking::WinSock::SOCKADDR_INET::default();
            let base = &mut sinet as *mut _ as *mut u8;
            *base.add(0) = 2;
            *base.add(1) = 0;
            core::ptr::copy_nonoverlapping(octets.as_ptr(), base.add(4), 4);
            row.Address = sinet;
            row.OnLinkPrefixLength = prefix_len;
            CreateUnicastIpAddressEntry(&row).0 == 0
        }
    }

    pub fn ensure_serverbox_ip() -> bool {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !Self::ps_ip_present(HOST_GAME_IP, AWL_IFACE_NAME) {
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(300));
        }
        if Self::ps_ip_present(HOST_SERVERBOX_IP, AWL_IFACE_NAME) {
            return true;
        }
        for _ in 0..3 {
            if Self::add_unicast_ip(AWL_IFACE_NAME, HOST_SERVERBOX_IP, 24)
                && Self::ps_ip_present(HOST_SERVERBOX_IP, AWL_IFACE_NAME)
            {
                return true;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        false
    }

    pub fn ensure_identity(exe: &Path) -> Result<(), String> {
        if Self::has_identity() {
            return Ok(());
        }

        Self::write_config(&format!("{HOST_GAME_IP}/24"), None, true)?;
        let handle = Self::spawn_awl(exe)?;
        let deadline = Instant::now() + LOCAL_API_READY_TIMEOUT;
        loop {
            if Self::has_identity() {
                drop(handle);

                std::thread::sleep(Duration::from_millis(500));
                return Ok(());
            }
            if Instant::now() >= deadline {
                drop(handle);
                return Err(t!("awl.awl_failed_to_mint_identity").to_string());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
    }

    fn spawn_awl(exe: &Path) -> Result<AwlHandle, String> {
        for attempt in 0..2 {
            match Self::spawn_and_wait_ready(exe) {
                Ok(handle) => return Ok(handle),
                Err(SpawnWaitFail::Exited(msg)) => return Err(msg),
                Err(SpawnWaitFail::NotReady) => {
                    if attempt == 1 {
                        return Err(t!("awl.awl_api_did_not_become")
                        .to_string());
                    }
                }
            }
        }
        unreachable!("retry loop always returns")
    }

    fn spawn_and_wait_ready(exe: &Path) -> Result<AwlHandle, SpawnWaitFail> {
        let mut cmd = Command::new(exe);
        cmd.env("AWL_DATA_DIR", Self::data_dir());
        cmd.stdout(Stdio::null()).stderr(Stdio::null());
        #[cfg(windows)]
        cmd.creation_flags(CREATE_NO_WINDOW);
        let mut child = cmd.spawn().map_err(|e| {
            SpawnWaitFail::Exited(t!("awl.failed_to_start_awl", exe = exe.display(), error = e).to_string())
        })?;
        let job = crate::vpn::attach_job(&child);

        let deadline = Instant::now() + LOCAL_API_READY_TIMEOUT;
        loop {
            if let Ok(Some(_)) = child.try_wait() {
                crate::vpn::close_job(job);
                let _ = child.wait();
                return Err(SpawnWaitFail::Exited(t!("awl.awl_exited_immediately")
                .to_string()));
            }
            match Self::local_request("GET", "/api/v0/peers/get_known", None) {
                Ok((200, _)) => return Ok(AwlHandle { child, job }),
                _ => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        crate::vpn::close_job(job);
                        return Err(SpawnWaitFail::NotReady);
                    }
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
    }

    pub fn known_peers() -> Vec<PeerView> {
        let Ok((200, body)) = Self::local_request("GET", "/api/v0/peers/get_known", None) else {
            return Vec::new();
        };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&body) else {
            return Vec::new();
        };
        let Some(arr) = v.as_array() else { return Vec::new() };
        arr.iter()
            .map(|p| PeerView {
                peer_id: p.get("PeerID").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                alias: p.get("Alias").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                ip: p.get("IpAddr").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                connected: p.get("Connected").and_then(|v| v.as_bool()).unwrap_or(false),
                confirmed: p.get("Confirmed").and_then(|v| v.as_bool()).unwrap_or(false),
                ping_ms: p.get("Ping").and_then(|v| v.as_f64()).unwrap_or(0.0) / 1e6,
                through_relay: p
                    .get("Connections")
                    .and_then(|v| v.as_array())
                    .map(|cs| cs.iter().any(|c| c.get("ThroughRelay").and_then(|v| v.as_bool()).unwrap_or(false)))
                    .unwrap_or(false),
            })
            .collect()
    }

    pub fn link_member_to_host(
        host_peer_id: &str,
        host_game_ip: &str,
        host_serverbox_ip: &str,
    ) -> Result<(), String> {
        let invite = json!({
            "peerId": host_peer_id,
            "ipAddr": host_game_ip,
            "ipAddr2": host_serverbox_ip,
            "alias": "host",
        });
        let (status, body) =
            Self::local_request("POST", "/api/v0/peers/invite_peer", Some(&invite.to_string()))?;

        if status != 200 && !body.contains("already been added") {
            return Err(t!("awl.failed_to_invite_host", body = body).to_string());
        }

        let deadline = Instant::now() + LINK_TIMEOUT;
        loop {
            let linked = Self::known_peers()
                .iter()
                .any(|p| p.peer_id == host_peer_id && p.confirmed && p.connected);
            if linked {
                break;
            }
            if Instant::now() >= deadline {
                return Err(t!("awl.timed_out_waiting_for_host").to_string());
            }
            std::thread::sleep(Duration::from_secs(1));
        }

        let update = json!({
            "peerId": host_peer_id,
            "ipAddr": host_game_ip,
            "ipAddr2": host_serverbox_ip,
            "alias": "host",
            "domainName": "host",
        });
        let (status, body) = Self::local_request(
            "POST",
            "/api/v0/peers/update_settings",
            Some(&update.to_string()),
        )?;
        if status != 200 {
            return Err(t!("awl.failed_to_update_host_ip", body = body).to_string());
        }
        Ok(())
    }

    pub fn invite_member(member_peer_id: &str, seat_ip: &str) -> Result<(), String> {
        let alias = format!("join-{}", seat_ip.rsplit('.').next().unwrap_or("m"));
        let invite = json!({
            "peerId": member_peer_id,
            "ipAddr": seat_ip,
            "alias": alias,
        });
        let (status, body) =
            Self::local_request("POST", "/api/v0/peers/invite_peer", Some(&invite.to_string()))?;

        if status != 200 && !body.contains("already been added") {
            return Err(t!("awl.failed_to_invite_member", body = body).to_string());
        }
        let update = json!({
            "peerId": member_peer_id,
            "ipAddr": seat_ip,
            "alias": alias,
            "domainName": alias,
        });
        let (status, body) = Self::local_request(
            "POST",
            "/api/v0/peers/update_settings",
            Some(&update.to_string()),
        )?;
        if status != 200 {
            return Err(t!("awl.failed_to_correct_member_seat", body = body).to_string());
        }
        Ok(())
    }

    pub fn remove_all_peers() {
        for p in Self::known_peers() {
            let body = json!({ "peerId": p.peer_id });
            let _ = Self::local_request("POST", "/api/v0/peers/remove", Some(&body.to_string()));
        }
    }

    pub fn host_flow(
        room_cfg: &RoomConfig,
        name: &str,
        game: &str,
        game_id: &str,
        password: &str,
        keychip: &str,
        dir: &RoomDirClient,
    ) -> Result<(AwlHandle, String), String> {
        let exe = Self::exe_path(room_cfg);
        if !exe.is_file() {
            return Err(t!("awl.awl_tray_exe_not_found", exe = exe.display()).to_string());
        }
        Self::version_check(&exe)?;
        Self::ensure_identity(&exe)?;

        let need_sb = room_host_needs_serverbox_ip(game_id);
        Self::write_config(
            &format!("{HOST_GAME_IP}/24"),
            need_sb.then(|| format!("{HOST_SERVERBOX_IP}/24")).as_deref(),
            false,
        )?;
        Self::ensure_firewall(&exe);
        let handle = Self::spawn_awl(&exe)?;
        let my_peer_id = match Self::my_peer_id() {
            Ok(id) => id,
            Err(e) => {
                drop(handle);
                return Err(e);
            }
        };
        let room_id = match dir.create_room(name, game, game_id, password, &my_peer_id, keychip) {
            Ok(id) => id,
            Err(e) => {
                drop(handle);
                return Err(e);
            }
        };
        if need_sb && !Self::ensure_serverbox_ip() {

            eprintln!("warn: failed to add serverbox ip {HOST_SERVERBOX_IP}");
        }
        Ok((handle, room_id))
    }

    pub fn host_resume_flow(
        room_cfg: &RoomConfig,
        game_id: Option<&str>,
    ) -> Result<AwlHandle, String> {
        let exe = Self::exe_path(room_cfg);
        if !exe.is_file() {
            return Err(t!("awl.awl_tray_exe_not_found", exe = exe.display()).to_string());
        }
        Self::version_check(&exe)?;
        Self::ensure_identity(&exe)?;
        let need_sb = game_id.map(room_host_needs_serverbox_ip).unwrap_or(true);
        Self::write_config(
            &format!("{HOST_GAME_IP}/24"),
            need_sb.then(|| format!("{HOST_SERVERBOX_IP}/24")).as_deref(),
            false,
        )?;
        Self::ensure_firewall(&exe);
        let handle = Self::spawn_awl(&exe)?;
        if need_sb {
            let _ = Self::ensure_serverbox_ip();
        }
        Ok(handle)
    }

    pub fn join_flow(
        room_cfg: &RoomConfig,
        room_id: &str,
        password: &str,
        dir: &RoomDirClient,
    ) -> Result<(AwlHandle, JoinInfo), String> {
        let exe = Self::exe_path(room_cfg);
        if !exe.is_file() {
            return Err(t!("awl.awl_tray_exe_not_found", exe = exe.display()).to_string());
        }
        Self::version_check(&exe)?;
        Self::ensure_identity(&exe)?;
        let my_peer_id = Self::my_peer_id()?;
        let join = dir.join(room_id, password, &my_peer_id).map_err(|e| e.message())?;

        Self::write_config(&format!("{}/24", join.your_ip), None, false)?;
        Self::ensure_firewall(&exe);
        let handle = Self::spawn_awl(&exe)?;
        if let Err(e) = Self::link_member_to_host(&join.host_peer_id, &join.host_ip, &join.host_ip2) {
            drop(handle);
            return Err(e);
        }
        Ok((handle, join))
    }
}

#[cfg(test)]
mod tests {
    use super::room_host_needs_serverbox_ip;

    #[test]
    fn idz_idac_hosts_single_ip() {

        assert!(!room_host_needs_serverbox_ip("SDDF"));
        assert!(!room_host_needs_serverbox_ip("SDER"));
        assert!(!room_host_needs_serverbox_ip("SDGT"));

        assert!(room_host_needs_serverbox_ip("SBZZ"));
        assert!(room_host_needs_serverbox_ip("SBML"));
        assert!(room_host_needs_serverbox_ip("SBUU"));
        assert!(room_host_needs_serverbox_ip(""));
    }
}
