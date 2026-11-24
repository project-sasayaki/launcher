use std::fs;
use std::path::Path;

use rust_i18n::t;

use super::render_util::{bool_str, copy_tool_files, deadzone_value, push_xi_button, Sections, split_resolution};
use crate::storage::{
    CoinConfig, DPAD_CODE_BASE, GraphicsConfig, InputConfig, INPUT_RATIO_DEFAULT, NetworkConfig,
    SystemConfig,
};

#[derive(Clone, Debug)]
pub struct DeploySnapshot {
    pub net: NetworkConfig,
    pub system: SystemConfig,
    pub coin: CoinConfig,
    pub graphics: GraphicsConfig,
    pub input: InputConfig,

    pub card: Option<String>,

    pub room: Option<RoomNetOverride>,
}

#[derive(Clone, Debug)]
pub struct RoomNetOverride {

    pub cidr: String,

    pub my_game_ip: Option<String>,

    pub server_ip: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MiceTarget {

    Game,

    ServerBox,
}

impl MiceTarget {
    fn ip_offset(self) -> u32 {
        match self {
            MiceTarget::Game => 101,
            MiceTarget::ServerBox => 102,
        }
    }
}

fn log_level_num(v: &str) -> Option<u32> {
    match v.trim().to_ascii_lowercase().as_str() {
        "error" => Some(2),
        "warn" => Some(3),
        "info" => Some(4),
        "debug" => Some(5),
        "trace" => Some(6),
        _ => None,
    }
}

fn region_num(v: &str) -> Option<u32> {
    match v.trim().to_ascii_uppercase().as_str() {
        "JPN" => Some(1),
        "EXP" => Some(4),
        "CHN" => Some(8),
        _ => None,
    }
}

fn log_level_value(v: Option<&str>) -> u32 {
    v.and_then(log_level_num).unwrap_or(4)
}

pub(crate) fn format_ipv4(v: u32) -> String {
    let b = v.to_be_bytes();
    format!("{}.{}.{}.{}", b[0], b[1], b[2], b[3])
}

pub(crate) fn parse_ipv4(s: &str) -> Option<u32> {
    let parts: Vec<&str> = s.split('.').collect();
    if parts.len() != 4 {
        return None;
    }
    let mut bytes = [0u8; 4];
    for (b, p) in bytes.iter_mut().zip(&parts) {
        *b = p.trim().parse().ok()?;
    }
    Some(u32::from_be_bytes(bytes))
}

pub(crate) fn mask_of(prefix: u8) -> u32 {
    if prefix == 0 {
        0
    } else {
        u32::MAX << (32 - prefix.min(32) as u32)
    }
}

pub(crate) fn parse_cidr(s: &str) -> Option<(u32, u8)> {
    let s = s.trim();
    let (ip_part, prefix_part) = match s.split_once('/') {
        Some((ip, p)) => (ip, p.trim().parse::<u8>().ok()?),
        None => (s, 32),
    };
    if prefix_part > 32 {
        return None;
    }
    let ip = parse_ipv4(ip_part)?;
    Some((ip & mask_of(prefix_part), prefix_part))
}

pub(crate) fn last_host(base: u32, prefix: u8) -> Option<u32> {
    if prefix >= 31 {
        return None;
    }
    Some((base | !mask_of(prefix)).wrapping_sub(1))
}

pub fn render_config(dep: &DeploySnapshot, target: MiceTarget) -> String {
    let mut s = Sections::new();

    s.push("mice", "apply_patches", "false");
    s.push("mice", "log_level", &log_level_value(dep.system.loglevel.as_deref()).to_string());

    if let Some(k) = dep.system.keychip.as_deref().map(str::trim).filter(|k| !k.is_empty()) {
        s.push("sysconf", "keychip_serial", k);
    }
    if let Some(r) = dep.system.region.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        if let Some(n) = region_num(r) {
            s.push("sysconf", "region", &n.to_string());
        }
    }
    if let Some(fp) = dep.coin.freeplay {
        s.push("sysconf", "freeplay", bool_str(fp));
    }

    let room = dep.room.as_ref();
    let cidr = room.map(|r| r.cidr.as_str()).or(dep.net.ip.as_deref());
    if let Some(cidr) = cidr {
        if let Some((base, prefix)) = parse_cidr(cidr) {
            s.push("sysconf", "keychip_subnet", cidr.trim());

            let ip_override = match (room, target) {
                (Some(r), MiceTarget::Game) => r.my_game_ip.clone(),
                _ => None,
            };
            match ip_override {
                Some(ip) => s.push("network", "ip_address", &ip),
                None => s.push("network", "ip_address", &format_ipv4(base + target.ip_offset())),
            }
            s.push("network", "subnet_mask", &format_ipv4(mask_of(prefix)));
            if let Some(last) = last_host(base, prefix) {
                s.push("network", "gateway", &format_ipv4(last));
                s.push("network", "primary_dns", &format_ipv4(last));
            }
        }
    }
    s.push("network", "naominet_jp", dep.net.server_host());
    s.push("network", "aime_naominet_jp", dep.net.server_host());
    s.push("network", "ib_naominet_jp", "127.0.0.1");

    if dep.graphics.default != Some(true) {
        if let Some(b) = dep.graphics.borderless {
            s.push("window", "borderless", bool_str(b));
        }
        if let Some((w, h)) = dep.graphics.resolution.as_deref().and_then(split_resolution) {

            s.push("window", "dipsw", "false");
            s.push("window", "w", &w.to_string());
            s.push("window", "h", &h.to_string());
        }
    }

    s.push("ffb", "enable", bool_str(dep.input.ffb_enable.unwrap_or(true)));
    s.push("ffb", "constantForceStrength", &dep.input.ffb_constant_force.unwrap_or(100).to_string());
    s.push("ffb", "damperStrength", &dep.input.ffb_damper.unwrap_or(100).to_string());
    s.push("ffb", "rumbleStrength", &dep.input.ffb_rumble.unwrap_or(100).to_string());
    s.push("ffb", "rumbleDuration", &dep.input.ffb_rumble_duration.unwrap_or(100).to_string());
    s.push("ffb", "baseDamperStrength", &dep.input.ffb_base_damper.unwrap_or(20).to_string());
    s.push("ffb", "deadBand", &dep.input.ffb_dead_band.unwrap_or(20).to_string());

    let bind_ip = if let Some(r) = dep.room.as_ref() {
        Some(r.server_ip.clone())
    } else {
        trimmed(&dep.net.serverbox_ip).map(str::to_string)
    };
    if target == MiceTarget::ServerBox && dep.net.simulate == Some(false) {
        if let Some(ip) = bind_ip {
            s.push("ServerBox", "bind_ip", &ip);
        }
    }

    s.to_string()
}

fn trimmed(v: &Option<String>) -> Option<&str> {
    v.as_deref().map(str::trim).filter(|s| !s.is_empty())
}

fn render_scale_num(v: &str) -> Option<u32> {
    let digits: String = v.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
    match digits.as_str() {
        "2" => Some(2),
        "4" => Some(3),
        "8" => Some(4),
        _ => None,
    }
}

pub fn render_idas(dep: &DeploySnapshot) -> String {
    let mut s = Sections::new();

    let sim = dep.net.simulate.unwrap_or(true);
    s.push("ServerBox", "offlineMode", "false");
    s.push("ServerBox", "simulateNetwork", bool_str(sim));

    s.push(
        "ServerBox",
        "skipInitialization",
        bool_str(dep.system.skip_reader_init == Some(true)),
    );

    s.push(
        "CardReader",
        "auto_insert",
        bool_str(dep.system.auto_insert_card == Some(true)),
    );

    if dep.net.simulate == Some(false) {

        if let Some(ip) = dep.net.serverbox_ip.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            s.push("ServerBox", "bind_ip", ip);
        }

        if let Some(ip) = dep.net.host_ip.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            s.push("ServerBox", "game_ip", ip);
        }
    }

    if let Some(n) = dep.coin.start_coin {
        s.push("Coins", "start", &n.to_string());
    }
    if let Some(n) = dep.coin.continue_coin {
        s.push("Coins", "continues", &n.to_string());
    }
    if let Some(n) = dep.coin.card_coin {
        s.push("Coins", "card", &n.to_string());
    }
    if let Some(n) = dep.coin.car_coin {
        s.push("Coins", "car", &n.to_string());
    }

    if dep.graphics.render_scale == Some(true) {
        if let Some(n) = dep.graphics.render_scale_level.as_deref().and_then(render_scale_num) {
            s.push("Graphics", "render_scale", &n.to_string());
        }
    }

    let kind_lower = dep
        .input
        .kind
        .as_deref()
        .map(|v| v.trim().to_ascii_lowercase());
    match kind_lower.as_deref() {
        Some("xinput") => s.push("io3", "mode", "xinput"),
        Some("dinput") => s.push("io3", "mode", "dinput"),
        _ => {}
    }

    s.push(
        "io3",
        "restrict_",
        &dep.input.ratio.unwrap_or(INPUT_RATIO_DEFAULT).to_string(),
    );

    if kind_lower.as_deref() == Some("xinput") {

        s.push("xinput", "deviceIndex", &dep.input.pad_index.unwrap_or(0).to_string());

        let dual = dep.input.dual_stick.unwrap_or(false);
        s.push("xinput", "singleStickSteering", bool_str(!dual));

        let linear = dep.input.linear_steering.unwrap_or(true);
        s.push("xinput", "linearSteering", bool_str(linear));

        let ldz = dep.input.left_deadzone.unwrap_or(20);
        let rdz = dep.input.right_deadzone.unwrap_or(20);
        s.push("xinput", "leftStickDeadzone", &deadzone_value(ldz).to_string());
        s.push("xinput", "rightStickDeadzone", &deadzone_value(rdz).to_string());

        push_xi_button(&mut s, "xinput", "start", &dep.input.xi_start);
        push_xi_button(&mut s, "xinput", "viewChg", &dep.input.xi_view);
        push_xi_button(&mut s, "xinput", "up", &dep.input.xi_up);
        push_xi_button(&mut s, "xinput", "down", &dep.input.xi_down);
        push_xi_button(&mut s, "xinput", "left", &dep.input.xi_left);
        push_xi_button(&mut s, "xinput", "right", &dep.input.xi_right);
        push_xi_button(&mut s, "xinput", "shiftDn", &dep.input.xi_shift_dn);
        push_xi_button(&mut s, "xinput", "shiftUp", &dep.input.xi_shift_up);
        push_xi_button(&mut s, "xinput", "coin", &dep.input.xi_coin);
        push_xi_button(&mut s, "xinput", "card", &dep.input.xi_card);

        push_xi_button(&mut s, "xinput", "accel", &dep.input.xi_accel);
        push_xi_button(&mut s, "xinput", "brake", &dep.input.xi_brake);
    }

    if kind_lower.as_deref() == Some("dinput") {
        if let Some(v) = trimmed(&dep.input.wheel_device) {
            s.push("dinput", "deviceName", v);
        }
        if let Some(v) = trimmed(&dep.input.pedal_device) {
            s.push("dinput", "pedalsName", v);
        }
        if let Some(v) = trimmed(&dep.input.shifter_device) {
            s.push("dinput", "shifterName", v);
        }
        if let Some(n) = dep.input.start_button {
            s.push("dinput", "start", &n.to_string());
        }
        if let Some(n) = dep.input.view_button {
            s.push("dinput", "viewChg", &n.to_string());
        }
        if let Some(n) = dep.input.shift_up {
            s.push("dinput", "shiftUp", &n.to_string());
        }
        if let Some(n) = dep.input.shift_down {
            s.push("dinput", "shiftDn", &n.to_string());
        }

        if let Some(n) = dep.input.coin_button {
            s.push("dinput", "coin", &n.to_string());
        }
        if let Some(n) = dep.input.card_button {
            s.push("dinput", "card", &n.to_string());
        }

        let is_di_button = |n: u32| n < DPAD_CODE_BASE;
        if let Some(n) = dep.input.up_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "up", &n.to_string());
        }
        if let Some(n) = dep.input.down_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "down", &n.to_string());
        }
        if let Some(n) = dep.input.left_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "left", &n.to_string());
        }
        if let Some(n) = dep.input.right_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "right", &n.to_string());
        }

        if let Some(ref n) = dep.input.throttle_axis {
            s.push("dinput", "accelAxis", n);
        }
        if let Some(ref n) = dep.input.brake_axis {
            s.push("dinput", "brakeAxis", n);
        }

        s.push(
            "dinput",
            "reverseAccelAxis",
            bool_str(dep.input.reverse_accel_axis.unwrap_or(false)),
        );
        s.push(
            "dinput",
            "reverseBrakeAxis",
            bool_str(dep.input.reverse_brake_axis.unwrap_or(false)),
        );
    }

    if let Some(c) = dep.card.as_deref().map(str::trim).filter(|c| !c.is_empty()) {
        s.push("CardReader", "cardfile", c);
    }

    s.to_string()
}

const EXCLUDE_FILES: &[&str] = &["config.ini", "idas.ini", "sram.bin", "eeprom.bin"];

pub fn deploy(
    dir: &Path,
    tool_dir: &Path,
    dep: &DeploySnapshot,
    target: MiceTarget,
) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(t!("mice.target_directory_does_not_exist", dir = dir.display()).to_string());
    }
    if !tool_dir.is_dir() {
        return Err(t!("mice.mice_tool_directory_does_not", tool_dir = tool_dir.display()).to_string());
    }

    copy_tool_files(tool_dir, dir, EXCLUDE_FILES, "mice")?;

    let cfg_dir = dir.join("mice");
    fs::create_dir_all(&cfg_dir).map_err(|e| {
        t!("mice.failed_to_create_mice_config", cfg_dir = cfg_dir.display(), error = e).to_string()
    })?;
    write_atomic(&cfg_dir.join("config.ini"), &render_config(dep, target))?;
    write_atomic(&cfg_dir.join("idas.ini"), &render_idas(dep))?;
    Ok(())
}

pub(crate) fn write_atomic(target: &Path, text: &str) -> Result<(), String> {
    if fs::read_to_string(target).is_ok_and(|old| old == text) {
        return Ok(());
    }
    let tmp = target.with_extension("ini.tmp");
    fs::write(&tmp, text).map_err(|e| {
        t!("mice.failed_to_write_temporary_config", tmp = tmp.display(), error = e).to_string()
    })?;
    fs::rename(&tmp, target).map_err(|e| {
        let _ = fs::remove_file(&tmp);
        t!("mice.failed_to_replace_config", target = target.display(), error = e).to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn snapshot() -> DeploySnapshot {
        DeploySnapshot {
            net: NetworkConfig::default(),
            system: SystemConfig::default(),
            coin: CoinConfig::default(),
            graphics: GraphicsConfig::default(),
            input: InputConfig::default(),
            card: None,
            room: None,
        }
    }

    #[test]
    fn parse_cidr_and_derivations() {
        assert_eq!(parse_cidr("192.168.100.0/24"), Some((0xC0A8_6400, 24)));
        assert_eq!(parse_cidr("192.168.100.114/24"), Some((0xC0A8_6400, 24)));
        assert_eq!(parse_cidr("10.0.0.5"), Some((0x0A00_0005, 32)));
        assert_eq!(parse_cidr("bad"), None);
        assert_eq!(parse_cidr("1.2.3.4/33"), None);

        assert_eq!(mask_of(24), 0xFFFF_FF00);
        assert_eq!(format_ipv4(mask_of(24)), "255.255.255.0");
        assert_eq!(format_ipv4(mask_of(16)), "255.255.0.0");
        assert_eq!(format_ipv4(mask_of(8)), "255.0.0.0");

        let (base, prefix) = parse_cidr("192.168.100.0/24").unwrap();
        assert_eq!(last_host(base, prefix).map(format_ipv4).as_deref(), Some("192.168.100.254"));

        assert_eq!(last_host(0xC0A8_6400, 32), None);
        assert_eq!(last_host(0xC0A8_6400, 31), None);

        let (b30, p30) = parse_cidr("192.168.1.4/30").unwrap();
        assert_eq!(last_host(b30, p30).map(format_ipv4).as_deref(), Some("192.168.1.6"));
    }

    #[test]
    fn render_config_fixed_keys_and_defaults() {
        let text = render_config(&snapshot(), MiceTarget::Game);
        assert_eq!(
            text,
            "[mice]\n\
             apply_patches = false\n\
             log_level = 4\n\
             \n\
             [network]\n\
             naominet_jp = sdgt.arcadehub.xyz\n\
             aime_naominet_jp = sdgt.arcadehub.xyz\n\
             ib_naominet_jp = 127.0.0.1\n\
             \n\
             [ffb]\n\
             enable = true\n\
             constantForceStrength = 100\n\
             damperStrength = 100\n\
             rumbleStrength = 100\n\
             rumbleDuration = 100\n\
             baseDamperStrength = 20\n\
             deadBand = 20\n"
        );
    }

    #[test]
    fn render_config_ffb_default_and_custom() {
        let text = render_config(&snapshot(), MiceTarget::Game);
        assert!(text.contains("[ffb]"), "config.ini should write the [ffb] force feedback section");
        assert!(text.contains("enable = true"));
        assert!(text.contains("constantForceStrength = 100"));
        assert!(text.contains("damperStrength = 100"));
        assert!(text.contains("rumbleStrength = 100"));
        assert!(text.contains("rumbleDuration = 100"));
        assert!(text.contains("baseDamperStrength = 20"));
        assert!(text.contains("deadBand = 20"));

        let mut dep = snapshot();
        dep.input.ffb_enable = Some(false);
        dep.input.ffb_constant_force = Some(50);
        dep.input.ffb_damper = Some(40);
        dep.input.ffb_rumble = Some(30);
        dep.input.ffb_rumble_duration = Some(60);
        dep.input.ffb_base_damper = Some(10);
        dep.input.ffb_dead_band = Some(5);
        let text = render_config(&dep, MiceTarget::Game);
        assert!(text.contains("enable = false"));
        assert!(text.contains("constantForceStrength = 50"));
        assert!(text.contains("damperStrength = 40"));
        assert!(text.contains("rumbleStrength = 30"));
        assert!(text.contains("rumbleDuration = 60"));
        assert!(text.contains("baseDamperStrength = 10"));
        assert!(text.contains("deadBand = 5"));
    }

    #[test]
    fn render_config_subnet_game_vs_serverbox() {
        let mut dep = snapshot();
        dep.net.ip = Some("192.168.100.0/24".into());
        let game = render_config(&dep, MiceTarget::Game);
        let sb = render_config(&dep, MiceTarget::ServerBox);
        for (text, ip) in [(game, "192.168.100.101"), (sb, "192.168.100.102")] {
            assert!(text.contains("keychip_subnet = 192.168.100.0/24"));
            assert!(text.contains(&format!("ip_address = {ip}")));
            assert!(text.contains("subnet_mask = 255.255.255.0"));
            assert!(text.contains("gateway = 192.168.100.254"));
            assert!(text.contains("primary_dns = 192.168.100.254"));
        }

        let game = render_config(&dep, MiceTarget::Game);
        assert!(game.find("[sysconf]").unwrap() < game.find("[network]").unwrap());
    }

    #[test]
    fn render_config_unset_subnet_omits_network_derived() {
        let mut dep = snapshot();
        dep.net.ip = Some("not-a-cidr".into());
        let text = render_config(&dep, MiceTarget::Game);
        assert!(!text.contains("keychip_subnet"));
        assert!(!text.contains("ip_address"));
        assert!(!text.contains("gateway"));
        assert!(!text.contains("primary_dns"));

        assert!(text.contains("naominet_jp = sdgt.arcadehub.xyz"));
    }

    #[test]
    fn render_config_loglevel_mapping() {
        let mut dep = snapshot();
        for (raw, want) in [
            ("error", 2),
            ("warn", 3),
            ("info", 4),
            ("debug", 5),
            ("trace", 6),
            ("unknown", 4),
        ] {
            dep.system.loglevel = Some(raw.into());
            assert!(
                render_config(&dep, MiceTarget::Game).contains(&format!("log_level = {want}")),
                "{raw} → {want}"
            );
        }
        dep.system.loglevel = None;
        assert!(render_config(&dep, MiceTarget::Game).contains("log_level = 4"));
    }

    #[test]
    fn render_config_region_mapping() {
        let mut dep = snapshot();
        for (raw, want) in [("JPN", 1), ("EXP", 4), ("CHN", 8)] {
            dep.system.region = Some(raw.into());
            assert!(
                render_config(&dep, MiceTarget::Game).contains(&format!("region = {want}")),
                "{raw} → {want}"
            );
        }

        dep.system.region = Some("USA".into());
        assert!(!render_config(&dep, MiceTarget::Game).contains("region ="));
        dep.system.region = Some(" jpn ".into());
        assert!(render_config(&dep, MiceTarget::Game).contains("region = 1"));
        dep.system.region = None;
        assert!(!render_config(&dep, MiceTarget::Game).contains("region ="));
    }

    #[test]
    fn render_config_keychip_freeplay_and_window() {
        let mut dep = snapshot();
        dep.system.keychip = Some(" A69E-01A99999999 ".into());
        dep.coin.freeplay = Some(true);
        dep.graphics.default = Some(false);
        dep.graphics.borderless = Some(false);
        dep.graphics.resolution = Some("1920x1080".into());
        let text = render_config(&dep, MiceTarget::Game);
        assert!(text.contains("keychip_serial = A69E-01A99999999"));
        assert!(text.contains("freeplay = true"));
        assert!(text.contains("[window]\nborderless = false\ndipsw = false\nw = 1920\nh = 1080\n"));
    }

    #[test]
    fn render_config_window_omitted_when_default() {
        let mut dep = snapshot();
        dep.graphics.default = Some(true);
        dep.graphics.resolution = Some("1920x1080".into());
        let text = render_config(&dep, MiceTarget::Game);
        assert!(!text.contains("[window]"));
        assert!(!text.contains("dipsw"));
    }

    #[test]
    fn render_idas_serverbox_and_coins_skip_unset() {
        let mut dep = snapshot();
        dep.coin.start_coin = Some(2);
        dep.coin.car_coin = Some(1);
        let text = render_idas(&dep);

        assert_eq!(
            text,
            "[ServerBox]\n\
             offlineMode = false\n\
             simulateNetwork = true\n\
             skipInitialization = false\n\
             \n\
             [CardReader]\n\
             auto_insert = false\n\
             \n\
             [Coins]\n\
             start = 2\n\
             car = 1\n\
             \n\
             [io3]\n\
             restrict_ = 128\n"
        );
    }

    #[test]
    fn render_idas_skip_initialization_same_direction_always_written() {
        for (skip, want) in [(None, "false"), (Some(false), "false"), (Some(true), "true")] {
            let mut dep = snapshot();
            dep.system.skip_reader_init = skip;
            let text = render_idas(&dep);
            assert!(
                text.contains(&format!("skipInitialization = {want}")),
                "skip_reader_init={skip:?} should write skipInitialization = {want}:\n{text}"
            );
        }
    }

    #[test]
    fn render_idas_offline_mode_always_false() {

        for (sim, sim_net) in [(None, "true"), (Some(true), "true"), (Some(false), "false")] {
            let mut dep = snapshot();
            dep.net.simulate = sim;
            let text = render_idas(&dep);
            assert!(
                text.contains("offlineMode = false"),
                "simulate={sim:?} should write offlineMode = false:\n{text}"
            );
            assert!(
                text.contains(&format!("simulateNetwork = {sim_net}")),
                "simulate={sim:?} should write simulateNetwork = {sim_net}:\n{text}"
            );
        }
    }

    #[test]
    fn render_idas_render_scale_mapping() {
        let mut dep = snapshot();
        dep.graphics.render_scale = Some(true);
        let cases: &[(&str, Option<u32>)] = &[
            ("2x", Some(2)),
            ("4x", Some(3)),
            ("8x", Some(4)),
            ("16x", None),
            ("bad", None),
        ];
        for (raw, want) in cases {
            dep.graphics.render_scale_level = Some((*raw).into());
            let text = render_idas(&dep);
            match want {
                Some(n) => assert!(text.contains(&format!("render_scale = {n}"))),
                None => assert!(!text.contains("render_scale"), "{raw} should not be written"),
            }
        }

        dep.graphics.render_scale = Some(false);
        dep.graphics.render_scale_level = Some("2x".into());
        assert!(!render_idas(&dep).contains("render_scale"));
    }

    #[test]
    fn render_idas_input_mappings() {
        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        dep.input.ratio = Some(128);
        dep.input.dual_stick = Some(true);
        dep.input.linear_steering = Some(false);
        dep.input.left_deadzone = Some(20);
        dep.input.right_deadzone = Some(20);
        let text = render_idas(&dep);
        assert!(text.contains("[io3]\nmode = xinput\nrestrict_ = 128\n"));
        assert!(text.contains("singleStickSteering = false"));
        assert!(text.contains("linearSteering = false"));

        assert!(text.contains("leftStickDeadzone = 6553"));
        assert!(text.contains("rightStickDeadzone = 6553"));
        assert!(!text.contains("[dinput]"), "xinput mode does not write the dinput section");

        let mut dep2 = snapshot();
        dep2.input.kind = Some("dinput".into());
        dep2.input.wheel_device = Some("Wireless Controller".into());
        dep2.input.start_button = Some(2);
        dep2.input.view_button = Some(3);
        dep2.input.shift_up = Some(6);
        dep2.input.shift_down = Some(5);
        dep2.input.coin_button = Some(11);
        dep2.input.card_button = Some(12);
        dep2.input.throttle_axis = Some("Y".into());
        dep2.input.brake_axis = Some("RZ".into());
        let text2 = render_idas(&dep2);
        assert!(text2.contains("mode = dinput"));
        assert!(text2.contains("deviceName = Wireless Controller"));
        assert!(text2.contains("start = 2"));
        assert!(text2.contains("viewChg = 3"));
        assert!(text2.contains("shiftUp = 6"));
        assert!(text2.contains("shiftDn = 5"));

        assert!(text2.contains("coin = 11"));
        assert!(text2.contains("card = 12"));

        assert!(text2.contains("accelAxis = Y"));
        assert!(text2.contains("brakeAxis = RZ"));

        assert!(text2.contains("reverseAccelAxis = false"));
        assert!(text2.contains("reverseBrakeAxis = false"));
        assert!(!text2.contains("[xinput]"), "dinput mode does not write the xinput section");
    }

    #[test]
    fn render_idas_dinput_direction_buttons_skip_dpad_pseudo_codes() {
        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        dep.input.up_button = Some(13);
        dep.input.down_button = Some(14);
        dep.input.left_button = Some(15);
        dep.input.right_button = Some(16);
        let text = render_idas(&dep);
        assert!(text.contains("up = 13"));
        assert!(text.contains("down = 14"));
        assert!(text.contains("left = 15"));
        assert!(text.contains("right = 16"));

        let mut dep2 = snapshot();
        dep2.input.kind = Some("dinput".into());
        dep2.input.up_button = Some(DPAD_CODE_BASE + 1);
        dep2.input.left_button = Some(DPAD_CODE_BASE + 4);
        let text2 = render_idas(&dep2);
        assert!(!text2.contains("up = "), "D-pad pseudo codes should not be written to dinput");
        assert!(!text2.contains("left = "), "D-pad pseudo codes should not be written to dinput");
    }

    #[test]
    fn render_idas_xinput_device_index_and_buttons() {

        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        let text = render_idas(&dep);
        assert!(text.contains("[xinput]"));

        let block = text.split("[xinput]").nth(1).unwrap();
        let line = block.lines().find(|l| l.starts_with("deviceIndex")).unwrap();
        assert_eq!(line, "deviceIndex = 0");

        assert!(!text.contains("start ="));
        assert!(!text.contains("viewChg ="));
        assert!(!text.contains("coin ="));
        assert!(!text.contains("accel ="));
        assert!(!text.contains("brake ="));

        let mut dep2 = snapshot();
        dep2.input.kind = Some("xinput".into());
        dep2.input.pad_index = Some(2);
        dep2.input.xi_start = Some("A".into());
        dep2.input.xi_coin = Some("Y".into());
        dep2.input.xi_card = Some("START".into());
        dep2.input.xi_accel = Some("RT".into());
        dep2.input.xi_brake = Some("LT".into());
        dep2.input.xi_up = Some(" ".into());
        let text2 = render_idas(&dep2);
        assert!(text2.contains("deviceIndex = 2"));
        assert!(text2.contains("start = A"));
        assert!(!text2.contains("viewChg ="));
        assert!(text2.contains("coin = Y"));
        assert!(text2.contains("card = START"));
        assert!(text2.contains("accel = RT"), "the throttle should be the captured button name");
        assert!(text2.contains("brake = LT"), "the brake should be the captured button name");

        assert!(!text2.contains("up ="));

        assert!(!text2.contains("shiftDn ="));
        assert!(!text2.contains("down ="));
    }

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            static SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
            let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "mice_test_{}_{tag}_{seq}",
                std::process::id()
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            TempDir(dir)
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn deploy_copies_tools_and_writes_configs() {
        let tmp = TempDir::new("deploy");
        let tool = tmp.0.join("tool");
        let game = tmp.0.join("game");
        fs::create_dir_all(&tool).unwrap();
        fs::create_dir_all(&game).unwrap();

        fs::write(tool.join("mice.exe"), "mice-bin").unwrap();
        fs::write(tool.join("mice.dll"), "dll-bin").unwrap();
        fs::write(tool.join("config.ini"), "[old]\nk = v\n").unwrap();
        fs::write(tool.join("idas.ini"), "[old]\nk = v\n").unwrap();
        fs::write(tool.join("mice.log"), "log").unwrap();
        fs::write(tool.join("sram.bin"), "sram").unwrap();
        fs::write(tool.join("eeprom.bin"), "eeprom").unwrap();

        let mut dep = snapshot();
        dep.net.ip = Some("192.168.100.0/24".into());
        dep.system.keychip = Some("A69E-01A99999999".into());

        mice_deploy(&game, &tool, &dep).unwrap();

        assert_eq!(fs::read(game.join("mice.exe")).unwrap(), b"mice-bin");
        assert_eq!(fs::read(game.join("mice.dll")).unwrap(), b"dll-bin");
        assert!(!game.join("config.ini").exists(), "the tool's own config.ini should not be copied");
        assert!(!game.join("idas.ini").exists());
        assert!(!game.join("mice.log").exists());
        assert!(!game.join("sram.bin").exists());
        assert!(!game.join("eeprom.bin").exists());

        let cfg = fs::read_to_string(game.join("mice/config.ini")).unwrap();
        assert_eq!(cfg, render_config(&dep, MiceTarget::Game));
        assert!(cfg.contains("ip_address = 192.168.100.101"));
        assert_eq!(
            fs::read_to_string(game.join("mice/idas.ini")).unwrap(),
            render_idas(&dep)
        );

        let mtime_before = fs::metadata(game.join("mice/config.ini"))
            .unwrap()
            .modified()
            .unwrap();
        mice_deploy(&game, &tool, &dep).unwrap();
        let mtime_after = fs::metadata(game.join("mice/config.ini"))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(mtime_before, mtime_after, "an unchanged config should not be rewritten");
    }

    #[test]
    fn deploy_serverbox_target_uses_102() {
        let tmp = TempDir::new("sb");
        let tool = tmp.0.join("tool");
        let sb = tmp.0.join("sb");
        fs::create_dir_all(&tool).unwrap();
        fs::create_dir_all(&sb).unwrap();
        fs::write(tool.join("mice.exe"), "x").unwrap();
        let mut dep = snapshot();
        dep.net.ip = Some("192.168.100.0/24".into());
        deploy(&sb, &tool, &dep, MiceTarget::ServerBox).unwrap();
        let cfg = fs::read_to_string(sb.join("mice/config.ini")).unwrap();
        assert!(cfg.contains("ip_address = 192.168.100.102"));
    }

    #[test]
    fn deploy_missing_dirs_error() {
        let tmp = TempDir::new("missing");
        let tool = tmp.0.join("tool");
        let game = tmp.0.join("game");
        fs::create_dir_all(&tool).unwrap();
        fs::write(tool.join("mice.exe"), "x").unwrap();

        let err = mice_deploy(&game, &tool, &snapshot()).unwrap_err();
        assert_eq!(err, t!("mice.target_directory_does_not_exist", dir = game.display()));

        fs::create_dir_all(&game).unwrap();
        let missing_tool = tmp.0.join("nope");
        let err = mice_deploy(&game, &missing_tool, &snapshot()).unwrap_err();
        assert_eq!(err, t!("mice.mice_tool_directory_does_not", tool_dir = missing_tool.display()));
    }

    fn mice_deploy(
        dir: &Path,
        tool_dir: &Path,
        dep: &DeploySnapshot,
    ) -> Result<(), String> {
        deploy(dir, tool_dir, dep, MiceTarget::Game)
    }

    #[test]
    fn render_idas_cardreader_cardfile() {
        let mut dep = snapshot();

        let text = render_idas(&dep);
        assert!(text.contains("[CardReader]\nauto_insert = false"));
        assert!(!text.contains("cardfile"));

        dep.card = Some("InidCrd12345.png".into());
        let text = render_idas(&dep);
        assert!(
            text.contains("[CardReader]\nauto_insert = false\ncardfile = InidCrd12345.png"),
            "cardfile should join the existing [CardReader] section:\n{text}"
        );
    }

    #[test]
    fn render_idas_auto_insert_same_direction_always_written() {
        for (v, want) in [(None, "false"), (Some(false), "false"), (Some(true), "true")] {
            let mut dep = snapshot();
            dep.system.auto_insert_card = v;
            let text = render_idas(&dep);
            assert!(
                text.contains(&format!("auto_insert = {want}")),
                "auto_insert_card={v:?} should write auto_insert = {want}:\n{text}"
            );
        }
    }

    #[test]
    fn render_config_naominet_uses_server_setting() {
        let mut dep = snapshot();
        dep.net.server = Some(" my.server.net ".into());
        let text = render_config(&dep, MiceTarget::Game);
        assert!(text.contains("naominet_jp = my.server.net"));
        assert!(text.contains("aime_naominet_jp = my.server.net"));
        assert!(!text.contains("sdgt.arcadehub.xyz"), "with a custom server the default domain is not written");
    }

    #[test]
    fn render_idas_serverbox_bind_ip() {

        let text = render_idas(&snapshot());
        assert!(!text.contains("bind_ip"));

        let mut dep = snapshot();
        dep.net.simulate = Some(false);
        dep.net.serverbox_ip = Some("  192.168.100.101  ".into());
        let text = render_idas(&dep);
        assert!(
            text.contains("bind_ip = 192.168.100.101"),
            "[ServerBox] bind_ip should be written:\n{text}"
        );
        assert!(
            text.find("bind_ip = 192.168.100.101").unwrap() > text.find("[ServerBox]").unwrap(),
            "bind_ip must be inside the [ServerBox] section:\n{text}"
        );
        assert!(!text.contains("[serverbox]"), "there should be no separate [serverbox] section");
    }

    #[test]
    fn render_idas_host_game_ip() {

        let text = render_idas(&snapshot());
        assert!(!text.contains("game_ip"));

        let mut dep = snapshot();
        dep.net.host_ip = Some("  192.168.100.50  ".into());
        assert!(
            !render_idas(&dep).contains("game_ip"),
            "game_ip should not be written while the network is simulated"
        );
        dep.net.simulate = Some(true);
        assert!(
            !render_idas(&dep).contains("game_ip"),
            "game_ip should not be written while the network is simulated"
        );

        dep.net.simulate = Some(false);
        let text = render_idas(&dep);
        assert!(
            text.contains("game_ip = 192.168.100.50"),
            "[ServerBox] game_ip should be written:\n{text}"
        );
        assert!(
            text.find("game_ip = 192.168.100.50").unwrap() > text.find("[ServerBox]").unwrap(),
            "game_ip must be inside the [ServerBox] section:\n{text}"
        );
    }

    #[test]
    fn render_config_serverbox_bind_ip() {

        let mut dep = snapshot();
        dep.net.simulate = Some(false);
        dep.net.serverbox_ip = Some("192.168.100.101".into());
        assert!(
            !render_config(&dep, MiceTarget::Game).contains("[ServerBox]"),
            "the config.ini of the Game target should not contain [ServerBox]"
        );

        let mut dep = snapshot();
        dep.net.simulate = Some(false);
        dep.net.serverbox_ip = Some("  192.168.100.101  ".into());
        let text = render_config(&dep, MiceTarget::ServerBox);
        assert!(
            text.contains("[ServerBox]\nbind_ip = 192.168.100.101\n"),
            "the ServerBox config.ini should write [ServerBox] bind_ip:\n{text}"
        );

        let text = render_config(&snapshot(), MiceTarget::ServerBox);
        assert!(!text.contains("[ServerBox]"));

        for sim in [None, Some(true)] {
            let mut dep = snapshot();
            dep.net.simulate = sim;
            dep.net.serverbox_ip = Some("192.168.100.101".into());
            assert!(
                !render_config(&dep, MiceTarget::ServerBox).contains("[ServerBox]"),
                "simulate={sim:?} should not write [ServerBox]"
            );
        }
    }
}
