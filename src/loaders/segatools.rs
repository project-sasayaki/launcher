use std::fs;
use std::path::{Path, PathBuf};

use rust_i18n::t;

use super::mice::{format_ipv4, parse_cidr, write_atomic, RoomNetOverride};
use super::render_util::{deadzone_value, push_xi_button, Sections};
use crate::storage::{
    CoinConfig, DPAD_CODE_BASE, GraphicsConfig, InputConfig, INPUT_RATIO_DEFAULT, NetworkConfig,
    SystemConfig,
};

#[derive(Clone)]
pub struct DeploySnapshot {
    pub net: NetworkConfig,
    pub system: SystemConfig,
    pub coin: CoinConfig,
    pub input: InputConfig,

    pub graphics: GraphicsConfig,

    pub version: Option<String>,

    pub game_id: Option<String>,

    pub aime_card: Option<String>,

    pub room: Option<RoomNetOverride>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameKind {

    Idz,

    Idac,
}

pub fn game_kind(game_id: Option<&str>) -> GameKind {
    match game_id {
        Some("SDDF") | Some("SDER") => GameKind::Idz,
        _ => GameKind::Idac,
    }
}

pub fn idz_serverbox_image(game_id: Option<&str>) -> &'static str {
    match game_id {
        Some("SDER") => "ServerBoxD8_Nu_x64_CHN.exe",
        _ => "ServerBoxD8_Nu_x64.exe",
    }
}

fn hook_dll(kind: GameKind) -> &'static str {
    match kind {
        GameKind::Idz => "idzhook.dll",
        GameKind::Idac => "idachook.dll",
    }
}

fn random_digits(n: usize) -> String {
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9E37_79B9_7F4A_7C15)
        ^ ((std::process::id() as u64) << 32);
    let mut next = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) as u32
    };
    (0..n)
        .map(|_| char::from(b'0' + (next() % 10) as u8))
        .collect()
}

pub fn pcbid_serial_random() -> String {
    format!("ACAE01A{}", random_digits(8))
}

pub fn ds_serial_random() -> String {
    format!("AAVE-01A{}", random_digits(8))
}

fn region_code(region: Option<&str>) -> Option<&'static str> {
    match region.map(str::trim) {
        Some("JPN") => Some("1"),
        Some("EXP") => Some("4"),
        Some("CHN") => Some("8"),
        _ => None,
    }
}

fn seat_dipsw(seat: Option<&str>) -> (u8, u8, u8) {
    match seat.map(str::trim) {
        Some("A1") => (0, 0, 0),
        Some("A2") => (0, 1, 0),
        Some("B1") => (0, 0, 1),
        Some("B2") => (0, 1, 1),
        _ => (1, 0, 0),
    }
}

const GFX_RESOLUTION_DEFAULT: (u32, u32) = (1920, 1080);

fn parse_resolution(v: &str) -> Option<(u32, u32)> {
    let (w, h) = v.trim().split_once('x')?;
    let w = w.trim().parse::<u32>().ok()?;
    let h = h.trim().parse::<u32>().ok()?;
    (w > 0 && h > 0).then_some((w, h))
}

pub fn culture_of(region: Option<&str>) -> &'static str {
    match region.map(str::trim) {
        Some("JPN") => "ja",
        _ => "en",
    }
}

pub fn spike_file(tool_dir: &Path, version: Option<&str>) -> Option<String> {
    let v = version.map(str::trim).filter(|v| !v.is_empty())?;
    let digits: String = v.chars().filter(|c| c.is_ascii_digit()).collect();
    let key = digits.get(0..3)?;
    let name = format!("indrun-{key}.txt");
    tool_dir
        .join("idac")
        .join("spike")
        .join(&name)
        .is_file()
        .then_some(name)
}

#[cfg(test)]
pub fn render_segatools_ini(dep: &DeploySnapshot, spike: Option<&str>) -> String {
    render_segatools_ini_with_pcbid(dep, spike, None, None)
}

pub fn render_segatools_ini_with_pcbid(
    dep: &DeploySnapshot,
    spike: Option<&str>,
    pcbid_serial: Option<&str>,
    ds_serial: Option<&str>,
) -> String {
    let kind = game_kind(dep.game_id.as_deref());
    let mut s = Sections::new();

    s.push("vfs", "amfs", "amfs");
    s.push("vfs", "option", "option");
    s.push("vfs", "appdata", "appdata");

    s.push("aime", "enable", "1");
    match dep
        .aime_card
        .as_deref()
        .map(str::trim)
        .filter(|c| !c.is_empty())
    {
        Some(code) => s.push("aime", "aimePath", &format!(r"DEVICE\{code}.txt")),
        None if kind == GameKind::Idz => s.push("aime", "aimePath", r"DEVICE\aime.txt"),
        None => {}
    }
    if kind == GameKind::Idz {
        s.push("aime", "felicaGen", "0");
        s.push("aime", "aimeGen", "1");
    }
    s.push("dns", "default", dep.net.server_host());

    s.push("dns", "router", "10.0.0.254");
    s.push("netenv", "enable", "1");

    if kind == GameKind::Idz {
        let suffix_ip = dep
            .room
            .as_ref()
            .map(|r| {
                r.my_game_ip
                    .clone()
                    .unwrap_or_else(|| crate::awl::HOST_GAME_IP.to_string())
            })
            .or_else(|| {
                dep.net
                    .host_ip
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            });
        if let Some(suffix) =
            suffix_ip.and_then(|ip| ip.rsplit('.').next().and_then(|s| s.parse::<u8>().ok()))
        {
            s.push("netenv", "addrSuffix", &suffix.to_string());
        }
    }

    if kind == GameKind::Idz {
        if let Some(sn) = pcbid_serial {
            s.push("pcbid", "serialNo", sn);
        }
    }

    if let Some(k) = dep
        .system
        .keychip
        .as_deref()
        .map(str::trim)
        .filter(|k| !k.is_empty())
    {
        s.push("keychip", "id", k);
    }

    if let Some(gid) = dep
        .game_id
        .as_deref()
        .map(str::trim)
        .filter(|g| !g.is_empty())
    {
        s.push("keychip", "gameId", gid);
    }

    let cidr = dep
        .room
        .as_ref()
        .map(|r| r.cidr.as_str())
        .or(dep.net.ip.as_deref().map(str::trim).filter(|v| !v.is_empty()));
    if let Some(cidr) = cidr.filter(|v| !v.is_empty()) {

        if let Some((base, _)) = parse_cidr(cidr) {
            s.push("keychip", "subnet", &format_ipv4(base));
        }
    }
    if let Some(r) = region_code(dep.system.region.as_deref()) {
        match kind {
            GameKind::Idz => s.push("ds", "region", r),
            GameKind::Idac => s.push("keychip", "region", r),
        }
    }

    if kind == GameKind::Idz {
        if let Some(sn) = ds_serial {
            s.push("ds", "serialNO", sn);
        }
    }

    s.push("system", "enable", "1");
    if let Some(fp) = dep.coin.freeplay {
        s.push("system", "freeplay", bool01(fp));
    }
    let master = dep
        .system
        .master
        .unwrap_or(dep.system.seat.as_deref().unwrap_or("SINGLE") == "SINGLE");
    match kind {
        GameKind::Idz => {
            s.push("gpio", "dipsw1", bool01(master));
        }
        GameKind::Idac => {
            let (d3, d4, d5) = seat_dipsw(dep.system.seat.as_deref());
            s.push("system", "dipsw1", bool01(master));
            s.push("system", "dipsw2", "0");
            s.push("system", "dipsw3", &d3.to_string());
            s.push("system", "dipsw4", &d4.to_string());
            s.push("system", "dipsw5", &d5.to_string());
        }
    }

    if kind == GameKind::Idac {
        s.push("indrun", "enable", "1");
        if let Some(f) = spike {
            s.push("indrun", "spike", f);
        }
    }

    s.push("led15070", "enable", "1");

    if kind == GameKind::Idz {

        s.push("gfx", "enable", "1");
        if dep.graphics.default == Some(false) {
            let (w, h) = dep
                .graphics
                .resolution
                .as_deref()
                .and_then(parse_resolution)
                .unwrap_or(GFX_RESOLUTION_DEFAULT);
            s.push("gfx", "width", &w.to_string());
            s.push("gfx", "height", &h.to_string());
            s.push(
                "gfx",
                "framed",
                if dep.graphics.borderless == Some(true) { "0" } else { "1" },
            );
        }
        s.push("gfx", "windowed", "1");
    }

    let io = match kind {
        GameKind::Idz => "io3",
        GameKind::Idac => "io4",
    };

    let kind_lower = dep
        .input
        .kind
        .as_deref()
        .map(|v| v.trim().to_ascii_lowercase());
    match kind_lower.as_deref() {
        Some("xinput") => s.push(io, "mode", "xinput"),
        Some("dinput") => s.push(io, "mode", "dinput"),
        _ => {}
    }
    s.push(
        io,
        "restrict",
        &dep.input.ratio.unwrap_or(INPUT_RATIO_DEFAULT).to_string(),
    );

    if kind_lower.as_deref() == Some("xinput") {
        s.push("xinput", "autoNeutral", "1");

        let dual = dep.input.dual_stick.unwrap_or(false);
        s.push("xinput", "singleStickSteering", bool01(!dual));

        let linear = dep.input.linear_steering.unwrap_or(true);
        s.push("xinput", "linearSteering", bool01(linear));
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

        push_xi_button(&mut s, "xinput", "accel", &dep.input.xi_accel);
        push_xi_button(&mut s, "xinput", "brake", &dep.input.xi_brake);
    }

    if kind_lower.as_deref() == Some("xinput") {
        if let Some(v) = dep.input.xi_card.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push("aime", "xiScan", v);
            if let Some(n) = dep.input.pad_index {
                s.push("aime", "xiDeviceIndex", &n.to_string());
            }
        }
    }

    let is_di_button = |n: u32| n < DPAD_CODE_BASE;
    if kind_lower.as_deref() == Some("dinput") {
        if let Some(v) = dep.input.wheel_device.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push("dinput", "deviceName", v);
        }
        if let Some(v) = dep.input.pedal_device.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push("dinput", "pedalsName", v);
        }
        if let Some(v) = dep.input.shifter_device.as_deref().map(str::trim).filter(|v| !v.is_empty()) {
            s.push("dinput", "shifterName", v);
        }
        if let Some(n) = dep.input.throttle_axis.as_deref() {
            s.push("dinput", "accelAxis", n);
        }
        if let Some(n) = dep.input.brake_axis.as_deref() {
            s.push("dinput", "brakeAxis", n);
        }
        if let Some(n) = dep.input.start_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "start", &n.to_string());
        }
        if let Some(n) = dep.input.view_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "viewChg", &n.to_string());
        }
        if let Some(n) = dep.input.left_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "left", &n.to_string());
        }
        if let Some(n) = dep.input.right_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "right", &n.to_string());
        }
        if let Some(n) = dep.input.up_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "up", &n.to_string());
        }
        if let Some(n) = dep.input.down_button.filter(|n| is_di_button(*n)) {
            s.push("dinput", "down", &n.to_string());
        }
        if let Some(n) = dep.input.shift_down.filter(|n| is_di_button(*n)) {
            s.push("dinput", "shiftDn", &n.to_string());
        }
        if let Some(n) = dep.input.shift_up.filter(|n| is_di_button(*n)) {
            s.push("dinput", "shiftUp", &n.to_string());
        }
        for (i, gear) in [
            dep.input.gear1_button,
            dep.input.gear2_button,
            dep.input.gear3_button,
            dep.input.gear4_button,
            dep.input.gear5_button,
            dep.input.gear6_button,
        ]
        .iter()
        .enumerate()
        {
            if let Some(n) = gear.filter(|n| is_di_button(*n)) {
                s.push("dinput", &format!("gear{}", i + 1), &n.to_string());
            }
        }
        s.push(
            "dinput",
            "reverseAccelAxis",
            bool01(dep.input.reverse_accel_axis.unwrap_or(false)),
        );
        s.push(
            "dinput",
            "reverseBrakeAxis",
            bool01(dep.input.reverse_brake_axis.unwrap_or(false)),
        );

        s.push(
            "dinput",
            "constantForceStrength",
            &dep.input.ffb_constant_force.unwrap_or(100).to_string(),
        );
        s.push(
            "dinput",
            "damperStrength",
            &dep.input.ffb_damper.unwrap_or(100).to_string(),
        );
        s.push(
            "dinput",
            "rumbleStrength",
            &dep.input.ffb_rumble.unwrap_or(100).to_string(),
        );
        s.push(
            "dinput",
            "rumbleDuration",
            &dep.input.ffb_rumble_duration.unwrap_or(100).to_string(),
        );
        s.push(
            "dinput",
            "baseDamperFraction",
            &dep.input.ffb_base_damper.unwrap_or(20).to_string(),
        );
        s.push(
            "dinput",
            "deadband",
            &dep.input.ffb_dead_band.unwrap_or(20).to_string(),
        );
    }

    s.push("ffb", "enable", bool01(dep.input.ffb_enable.unwrap_or(true)));

    s.to_string()
}

fn bool01(b: bool) -> &'static str {
    if b {
        "1"
    } else {
        "0"
    }
}

pub const AMDAEMON_CONFIGS: &[&str] = &[
    "config_common.json",
    "config_ex.json",
    "config_jp.json",
    "config_laninstall_client_ex.json",
    "config_laninstall_client_jp.json",
    "config_laninstall_server_ex.json",
    "config_laninstall_server_jp.json",
    "config_aime_high_ex.json",
    "config_aime_high_jp.json",
    "config_aime_normal_ex.json",
    "config_aime_normal_jp.json",
    "config_seat_1_ex.json",
    "config_seat_1_jp.json",
    "config_seat_2_ex.json",
    "config_seat_2_jp.json",
    "config_seat_3_ex.json",
    "config_seat_3_jp.json",
    "config_seat_4_ex.json",
    "config_seat_4_jp.json",
    "config_seat_single_ex.json",
    "config_seat_single_jp.json",
    "config_hook.json",
];

pub const IDZ_AMDAEMON_CONFIGS: &[&str] = &[
    "configDHCP_Final_Common.json",
    "configDHCP_Final_JP.json",
    "configDHCP_Final_JP_ST1.json",
    "configDHCP_Final_JP_ST2.json",
    "configDHCP_Final_EX.json",
    "configDHCP_Final_EX_ST1.json",
    "configDHCP_Final_EX_ST2.json",
];

pub const IDZ_AMDAEMON_CONFIGS_SDER: &[&str] = &[
    "configDHCP_Final_Common.json",
    "configDHCP_Final_ZH.json",
    "configDHCP_Final_ZH_ST1.json",
    "configDHCP_Final_ZH_ST2.json",
];

fn amdaemon_configs(kind: GameKind, game_id: Option<&str>) -> &'static [&'static str] {
    match (kind, game_id) {
        (GameKind::Idz, Some("SDER")) => IDZ_AMDAEMON_CONFIGS_SDER,
        (GameKind::Idz, _) => IDZ_AMDAEMON_CONFIGS,
        (GameKind::Idac, _) => AMDAEMON_CONFIGS,
    }
}

pub fn amdaemon_args(kind: GameKind, game_id: Option<&str>) -> Vec<String> {
    let mut args: Vec<String> = ["-d", "-k", hook_dll(kind), "amdaemon.exe", "-c"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    args.extend(amdaemon_configs(kind, game_id).iter().map(|s| s.to_string()));
    args
}

pub fn game_args(
    kind: GameKind,
    game_exe: &Path,
    culture: &str,
    user_dir: &Path,
) -> Vec<String> {
    let mut args: Vec<String> = vec!["-d".to_string()];
    args.push("-k".to_string());
    args.push(hook_dll(kind).to_string());
    args.push(game_exe.to_string_lossy().into_owned());
    match kind {
        GameKind::Idz => args.push("-m".to_string()),
        GameKind::Idac => {
            let user = user_dir.to_string_lossy().into_owned();
            args.push(format!("-culture={culture}"));
            args.push("launch=Cabinet".to_string());
            args.push(format!("ABSLOG={}\\GameProject.log", user));
            args.push("-Master".to_string());
            args.push(format!("UserDir={user}"));
            args.push("-NotInstalled".to_string());
            args.push("-UNATTENDED".to_string());
        }
    }
    args
}

pub fn find_userdata_dir(amdaemon_dir: &Path) -> PathBuf {
    let mut cur = Some(amdaemon_dir);
    for _ in 0..5 {
        let Some(p) = cur else { break };
        let cand = p.join("Userdata");
        if cand.is_dir() {
            return cand;
        }
        cur = p.parent();
    }
    amdaemon_dir.join(r"..\..\Userdata")
}

fn copy_if_changed(src: &Path, dest: &Path) -> Result<(), String> {
    if let (Ok(a), Ok(b)) = (fs::metadata(src), fs::metadata(dest)) {
        if a.len() == b.len() && a.modified().ok() == b.modified().ok() {
            return Ok(());
        }
    }
    fs::copy(src, dest)
        .map(|_| ())
        .map_err(|e| t!("common.failed_to_copy", src = src.display(), error = e).to_string())
}

pub fn deploy(dir: &Path, tool_dir: &Path, dep: &DeploySnapshot) -> Result<(), String> {
    if !dir.is_dir() {
        return Err(t!("segatools.amdaemon_directory_does_not_exist", dir = dir.display()).to_string());
    }
    if !tool_dir.is_dir() {
        return Err(t!("segatools.segatools_tool_directory_does_not", tool_dir = tool_dir.display()).to_string());
    }
    let kind = game_kind(dep.game_id.as_deref());

    let sub = match kind {
        GameKind::Idz => "idz",
        GameKind::Idac => "idac",
    };
    let sub_dir = tool_dir.join(sub);

    let spike = match kind {
        GameKind::Idac => spike_file(tool_dir, dep.version.as_deref()),
        GameKind::Idz => None,
    };

    let other_hook = match kind {
        GameKind::Idz => "idachook.dll",
        GameKind::Idac => "idzhook.dll",
    };

    let entries = fs::read_dir(tool_dir)
        .map_err(|e| t!("segatools.failed_to_read_tool_directory", tool_dir = tool_dir.display(), error = e).to_string())?;
    for e in entries.flatten() {
        let Ok(ft) = e.file_type() else { continue };
        if !ft.is_file() {
            continue;
        }
        let name = e.file_name().to_string_lossy().into_owned();
        if name.to_lowercase().ends_with(".log") || name.to_lowercase() == other_hook {
            continue;
        }
        copy_if_changed(&e.path(), &dir.join(&name))?;
    }

    let stale = dir.join(other_hook);
    if stale.is_file() {
        let _ = fs::remove_file(&stale);
    }

    if let Ok(entries) = fs::read_dir(&sub_dir) {
        for e in entries.flatten() {
            let Ok(ft) = e.file_type() else { continue };
            let name = e.file_name().to_string_lossy().into_owned();
            if ft.is_file() && name.to_lowercase().ends_with(".dll") {
                copy_if_changed(&e.path(), &dir.join(&name))?;
            }
        }
    }
    if kind == GameKind::Idac {
        let hook_json = sub_dir.join("config_hook.json");
        if !hook_json.is_file() {
            return Err(t!("segatools.segatools_tool_directory_is_missing", hook_json = hook_json.display()).to_string());
        }
        copy_if_changed(&hook_json, &dir.join("config_hook.json"))?;
    }

    let (pcbid, ds_serial) = match kind {
        GameKind::Idz => (Some(pcbid_serial_random()), Some(ds_serial_random())),
        GameKind::Idac => (None, None),
    };
    write_atomic(
        &dir.join("segatools.ini"),
        &render_segatools_ini_with_pcbid(
            dep,
            spike.as_deref(),
            pcbid.as_deref(),
            ds_serial.as_deref(),
        ),
    )?;

    for sub in ["option", "appdata"] {
        let d = dir.join(sub);
        if !d.is_dir() {
            fs::create_dir_all(&d).map_err(|e| {
                t!("segatools.failed_to_create_directory", sub = sub, d = d.display(), error = e).to_string()
            })?;
        }
    }
    if let Some(name) = &spike {
        copy_if_changed(
            &sub_dir.join("spike").join(name),
            &dir.join(name),
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::loaders::mice::{format_ipv4, last_host, mask_of, parse_cidr};

    fn snapshot() -> DeploySnapshot {
        DeploySnapshot {
            net: NetworkConfig::default(),
            system: SystemConfig::default(),
            coin: CoinConfig::default(),
            input: crate::storage::InputConfig::default(),
            graphics: crate::storage::GraphicsConfig::default(),
            version: None,
            game_id: None,
            aime_card: None,
            room: None,
        }
    }

    #[test]
    fn render_fixed_sections_only() {

        let text = render_segatools_ini(&snapshot(), None);
        assert!(text.contains("[vfs]\namfs = amfs\noption = option\nappdata = appdata\n"));
        assert!(text.contains("[aime]\nenable = 1"));
        assert!(text.contains("[dns]\ndefault = sdgt.arcadehub.xyz\nrouter = 10.0.0.254\n"));
        assert!(text.contains("[netenv]\nenable = 1"));
        assert!(text.contains("[system]\nenable = 1"));
        assert!(text.contains("[indrun]\nenable = 1"));
        assert!(text.contains("[led15070]\nenable = 1"));
        assert!(text.contains("[ffb]\nenable = 1"));
        assert!(!text.contains("[keychip]"), "without a keychip the whole section is left out");
        assert!(!text.contains("spike"), "without a spike file the spike key is not written");

        assert!(text.contains("[io4]\nrestrict = 128\n"));

        assert!(!text.contains("true") && !text.contains("false"));
    }

    #[test]
    fn render_keychip_and_freeplay() {
        let mut dep = snapshot();
        dep.system.keychip = Some(" A69E-01A88888888 ".into());
        dep.system.region = Some("CHN".into());
        dep.net.ip = Some("192.168.100.0/24".into());
        dep.coin.freeplay = Some(true);
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("id = A69E-01A88888888"));
        assert!(text.contains("subnet = 192.168.100.0"), "subnet is the base address without the /prefix");
        assert!(text.contains("region = 8"));
        assert!(text.contains("freeplay = 1"));
    }

    #[test]
    fn keychip_game_id_written_when_known() {

        let mut dep = snapshot();
        dep.game_id = Some(" SDGT ".into());
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[keychip]\ngameId = SDGT"),
            "game_id goes to [keychip]; with no other key set the section holds only gameId:\n{text}"
        );

        dep.system.keychip = Some("A69E-01A88888888".into());
        dep.net.ip = Some("192.168.100.0/24".into());
        let text = render_segatools_ini(&dep, None);
        let keychip = text.split("[keychip]").nth(1).expect("the [keychip] section should exist");
        assert!(keychip.contains("gameId = SDGT"), "gameId is in the same section as the serial:\n{text}");
        assert!(keychip.contains("id = A69E-01A88888888"));
        assert!(keychip.contains("subnet = 192.168.100.0"));

        let mut dep = snapshot();
        dep.game_id = Some("  ".into());
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("gameId"), "a blank game_id is not written:\n{text}");
        assert!(!text.contains("[keychip]"), "without keys the whole section is left out:\n{text}");
    }

    #[test]
    fn render_subnet_room_override() {

        let mut dep = snapshot();
        dep.net.ip = Some("192.168.100.0/24".into());
        dep.room = Some(RoomNetOverride {
            cidr: crate::awl::ROOM_CIDR.to_string(),
            my_game_ip: Some("172.10.1.103".into()),
            server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
        });
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("subnet = 172.10.1.0"),
            "in room mode subnet should be the room subnet base:\n{text}"
        );
        assert!(!text.contains("192.168.100.0"), "the original subnet should no longer be written:\n{text}");

        dep.room = None;
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("subnet = 192.168.100.0"), "outside room mode the original subnet is used:\n{text}");
    }

    #[test]
    fn restrict_always_written_from_ratio() {

        let text = render_segatools_ini(&snapshot(), None);
        assert!(text.contains("[io4]\nrestrict = 128\n"), "without a ratio the default 128 is written");

        let mut dep = snapshot();
        dep.input.ratio = Some(96);
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[io4]\nrestrict = 96\n"));

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        dep.version = None;
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[io3]\nrestrict = 128\n"), "IDZ writes the default 128 without a ratio");

        dep.input.ratio = Some(200);
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[io3]\nrestrict = 200\n"));
    }

    #[test]
    fn region_and_culture_mapping() {
        assert_eq!(region_code(Some("JPN")), Some("1"));
        assert_eq!(region_code(Some("EXP")), Some("4"));
        assert_eq!(region_code(Some("CHN")), Some("8"));
        assert_eq!(region_code(Some("XXX")), None);
        assert_eq!(region_code(None), None);
        assert_eq!(culture_of(Some("JPN")), "ja");
        assert_eq!(culture_of(Some("EXP")), "en");
        assert_eq!(culture_of(Some("CHN")), "en");
        assert_eq!(culture_of(None), "en");
    }

    #[test]
    fn spike_matching_by_version() {

        let digits: String = "3.21.00".chars().filter(|c| c.is_ascii_digit()).collect();
        assert_eq!(&digits[0..3], "321");

        let tool = Path::new("target/debug/tools/segatools");
        if tool.is_dir() {
            assert_eq!(
                spike_file(tool, Some("3.21.00")).as_deref(),
                Some("indrun-321.txt")
            );
            assert_eq!(
                spike_file(tool, Some("2.80.00")).as_deref(),
                Some("indrun-280.txt")
            );
            assert_eq!(spike_file(tool, Some("9.99.00")), None);
            assert_eq!(spike_file(tool, None), None);
        }
    }

    #[test]
    fn amdaemon_args_match_launch_bat() {

        let args = amdaemon_args(GameKind::Idac, None);
        assert_eq!(args.len(), 5 + AMDAEMON_CONFIGS.len());
        assert_eq!(&args[..5], &["-d", "-k", "idachook.dll", "amdaemon.exe", "-c"]);
        assert_eq!(args[5], "config_common.json");
        assert_eq!(*args.last().unwrap(), "config_hook.json");
        assert_eq!(AMDAEMON_CONFIGS.len(), 22);

        let args = amdaemon_args(GameKind::Idz, Some("SDDF"));
        assert_eq!(args.len(), 5 + IDZ_AMDAEMON_CONFIGS.len());
        assert_eq!(&args[..5], &["-d", "-k", "idzhook.dll", "amdaemon.exe", "-c"]);
        assert_eq!(args[5], "configDHCP_Final_Common.json");
        assert_eq!(*args.last().unwrap(), "configDHCP_Final_EX_ST2.json");
        assert_eq!(IDZ_AMDAEMON_CONFIGS.len(), 7);

        let args = amdaemon_args(GameKind::Idz, Some("SDER"));
        assert_eq!(args.len(), 5 + IDZ_AMDAEMON_CONFIGS_SDER.len());
        assert_eq!(&args[..5], &["-d", "-k", "idzhook.dll", "amdaemon.exe", "-c"]);
        assert_eq!(args[5], "configDHCP_Final_Common.json");
        assert_eq!(*args.last().unwrap(), "configDHCP_Final_ZH_ST2.json");
        assert_eq!(IDZ_AMDAEMON_CONFIGS_SDER.len(), 4);
    }

    #[test]
    fn game_args_use_absolute_userdir() {

        let args = game_args(
            GameKind::Idac,
            Path::new(r"D:\Game\GameProject-Win64-Shipping.exe"),
            "ja",
            Path::new(r"G:\idac\sdgt321\App\Userdata"),
        );
        assert_eq!(
            args,
            vec![
                "-d",
                "-k",
                "idachook.dll",
                r"D:\Game\GameProject-Win64-Shipping.exe",
                "-culture=ja",
                "launch=Cabinet",
                r"ABSLOG=G:\idac\sdgt321\App\Userdata\GameProject.log",
                "-Master",
                r"UserDir=G:\idac\sdgt321\App\Userdata",
                "-NotInstalled",
                "-UNATTENDED",
            ]
        );

        let args = game_args(
            GameKind::Idz,
            Path::new(r"D:\Game\InitialD0_DX11_Nu.exe"),
            "en",
            Path::new(r"D:\Game\Userdata"),
        );
        assert_eq!(
            args,
            vec![
                "-d",
                "-k",
                "idzhook.dll",
                r"D:\Game\InitialD0_DX11_Nu.exe",
                "-m"
            ]
        );
    }

    #[test]
    fn idz_ini_uses_gpio_ds_io3() {

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        dep.system.region = Some("JPN".into());
        dep.system.master = Some(true);
        dep.input.kind = Some("xinput".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[gpio]\ndipsw1 = 1"));
        assert!(text.contains("[ds]\nregion = 1"));
        assert!(text.contains("felicaGen = 0"));
        assert!(text.contains("aimeGen = 1"));
        assert!(text.contains(r"aimePath = DEVICE\aime.txt"), "without a card it falls back to the default");
        assert!(text.contains("[netenv]\nenable = 1"), "IDZ writes the netenv switch:\n{text}");
        assert!(!text.contains("addrSuffix"), "without a host IP addrSuffix is not written:\n{text}");
        assert!(text.contains("[io3]"), "the IDZ input section is io3");
        assert!(
            text.contains("[gfx]\nenable = 1"),
            "the IDZ graphics hook is always enabled:\n{text}"
        );
        assert!(text.contains("windowed = 1"), "IDZ is always windowed:\n{text}");
        assert!(!text.contains("[indrun]"), "IDZ has no indrun");
        assert!(!text.contains("dipsw3"), "the seat dipsw is SDGT only");
        assert!(!text.contains("[keychip]\nregion"), "the IDZ region is not in keychip");
    }

    #[test]
    fn idz_gfx_follows_default_toggle_and_resolution() {

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[gfx]\nenable = 1\nwindowed = 1"),
            "defaults not turned off: only enable / windowed are written:\n{text}"
        );
        assert!(!text.contains("width") && !text.contains("height"));

        dep.graphics.default = Some(false);
        dep.graphics.resolution = Some("1920x1080".into());
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[gfx]\nenable = 1\nwidth = 1920\nheight = 1080"),
            "defaults turned off + a resolution chosen:\n{text}"
        );
        assert!(
            text.contains("\nframed = 1\nwindowed = 1"),
            "without borderless, framed=1 and windowed=1 should be written:\n{text}"
        );

        let mut dep = snapshot();
        dep.game_id = Some("SDER".into());
        dep.graphics.default = Some(false);
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[gfx]\nenable = 1\nwidth = 1920\nheight = 1080"),
            "without a resolution, the default width and height should be written:\n{text}"
        );

        dep.graphics.borderless = Some(true);
        dep.graphics.render_scale = Some(true);
        dep.graphics.render_scale_level = Some("2x".into());
        dep.graphics.msaa = Some(true);
        dep.graphics.msaa_level = Some("4x".into());
        dep.graphics.af = Some(true);
        dep.graphics.af_level = Some("16x".into());
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[gfx]\nenable = 1\nwidth = 1920\nheight = 1080\nframed = 0\nwindowed = 1"),
            "with borderless, framed=0 should be written (windowed is always 1):\n{text}"
        );
        for key in ["fullscreen", "render_scale", "msaa", "af_level"] {
            assert!(!text.contains(key), "{key} should not be written to segatools.ini:\n{text}");
        }

        dep.graphics.borderless = Some(false);
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("\nframed = 1\nwindowed = 1"),
            "with borderless off, framed=1 and windowed=1 should be written:\n{text}"
        );

        let mut dep = snapshot();
        dep.game_id = Some("SDGT".into());
        dep.graphics.default = Some(false);
        dep.graphics.resolution = Some("1920x1080".into());
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("[gfx]"), "IDAC has no [gfx] section:\n{text}");

        let mut dep = snapshot();
        dep.game_id = Some("SDER".into());
        dep.graphics.default = Some(false);
        dep.graphics.resolution = Some("bogus".into());
        let text = render_segatools_ini(&dep, None);
        assert!(
            text.contains("[gfx]\nenable = 1\nwidth = 1920\nheight = 1080"),
            "an invalid resolution should fall back to the default width and height:\n{text}"
        );
    }

    #[test]
    fn idz_ini_addr_suffix_follows_room_ip() {

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        dep.room = Some(RoomNetOverride {
            cidr: crate::awl::ROOM_CIDR.to_string(),
            my_game_ip: Some("172.10.1.103".into()),
            server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
        });
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[netenv]\nenable = 1\naddrSuffix = 103"), "member .103 should be 103:\n{text}");

        dep.room = Some(RoomNetOverride {
            cidr: crate::awl::ROOM_CIDR.to_string(),
            my_game_ip: None,
            server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
        });
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[netenv]\nenable = 1\naddrSuffix = 101"), "host .101 should be 101:\n{text}");

        dep.game_id = Some("SDGT".into());
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("addrSuffix"), "IDAC does not write addrSuffix:\n{text}");

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        dep.net.host_ip = Some("  192.168.100.50  ".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[netenv]\nenable = 1\naddrSuffix = 50"), "outside room mode the last byte of the host IP, 50, should be written:\n{text}");

        dep.net.host_ip = None;
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("addrSuffix"), "without a host IP it is not written:\n{text}");
    }

    #[test]
    fn parse_resolution_accepts_wxh_only() {
        assert_eq!(parse_resolution("1920x1080"), Some((1920, 1080)));
        assert_eq!(parse_resolution(" 1360x768 "), Some((1360, 768)));
        assert_eq!(parse_resolution("1920x0"), None);
        assert_eq!(parse_resolution("1920"), None);
        assert_eq!(parse_resolution("axb"), None);
    }

    #[test]
    fn find_userdata_dir_walks_up() {
        let tmp = std::env::temp_dir().join(format!("dnet_segatools_{}", std::process::id()));
        let deep = tmp.join("App").join("DAC").join("AMDaemon");
        fs::create_dir_all(&deep).unwrap();

        let userdata = tmp.join("App").join("Userdata");
        fs::create_dir_all(&userdata).unwrap();
        assert_eq!(find_userdata_dir(&deep), userdata);

        let bare = tmp.join("bare");
        fs::create_dir_all(&bare).unwrap();
        assert_eq!(find_userdata_dir(&bare), bare.join(r"..\..\Userdata"));
        fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn ipv4_helpers_still_work_via_mice_reexports() {

        assert_eq!(parse_cidr("192.168.100.130/24"), Some((0xC0A86400, 24)));
        assert_eq!(format_ipv4(0xC0A86400), "192.168.100.0");
        assert_eq!(mask_of(24), 0xFFFFFF00);
        assert_eq!(last_host(0xC0A86400, 24), Some(0xC0A8_64FE));
    }

    #[test]
    fn dinput_dpad_pseudo_codes_are_skipped() {

        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        dep.input.up_button = Some(1001);
        dep.input.left_button = Some(1004);
        dep.input.start_button = Some(2);
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("start = 2"), "a real button number should be written as is");
        assert!(!text.contains("up = "), "D-pad pseudo codes should not be written to dinput");
        assert!(!text.contains("left = "), "D-pad pseudo codes should not be written to dinput");
    }

    #[test]
    fn xinput_button_mappings_written() {

        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        dep.input.pad_index = Some(2);
        dep.input.xi_start = Some(" A ".into());
        dep.input.xi_view = Some("BACK".into());
        dep.input.xi_up = Some("DPAD_UP".into());
        dep.input.xi_shift_up = Some("RB".into());
        dep.input.xi_coin = Some("X".into());
        dep.input.xi_card = Some("Y".into());
        dep.input.xi_accel = Some("RT".into());
        dep.input.xi_brake = Some("LT".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("start = A"), "surrounding blanks should be trimmed:\n{text}");
        assert!(text.contains("viewChg = BACK"));
        assert!(text.contains("up = DPAD_UP"));
        assert!(text.contains("shiftUp = RB"));
        assert!(text.contains("coin = X"));
        assert!(text.contains("accel = RT"));
        assert!(text.contains("brake = LT"));
        assert!(text.contains("xiScan = Y"), "the card scan button goes to [aime] xiScan:\n{text}");
        assert!(
            text.contains("xiDeviceIndex = 2"),
            "the controller slot used by xiScan:\n{text}"
        );
        assert!(!text.contains("card = "), "[xinput] has no card key (only mice has one):\n{text}");
    }

    #[test]
    fn xinput_button_bindings_skipped_when_unset_or_blank() {

        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        dep.input.xi_left = Some("  ".into());
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("left = "), "a blank value counts as unset:\n{text}");
        assert!(!text.contains("start = "), "a slot that was not captured is not written:\n{text}");
        assert!(!text.contains("xiScan"), "without a captured card scan button xiScan is not written:\n{text}");
        assert!(!text.contains("xiDeviceIndex"), "without xiScan the slot is not written:\n{text}");
    }

    #[test]
    fn xinput_button_bindings_ignored_in_dinput_mode() {

        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        dep.input.pad_index = Some(1);
        dep.input.xi_start = Some("A".into());
        dep.input.xi_card = Some("Y".into());
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("[xinput]"), "dinput mode has no [xinput] section:\n{text}");
        assert!(!text.contains("start = A"), "xi_* should not be written into the dinput config:\n{text}");
        assert!(!text.contains("xiScan"), "dinput mode does not write xiScan:\n{text}");
        assert!(!text.contains("xiDeviceIndex"), "dinput mode does not write the slot:\n{text}");
    }

    #[test]
    fn deploy_copies_idz_hook_dll() {
        let tool = Path::new("target/debug/tools/segatools");
        if !tool.join("idz").is_dir() {
            return;
        }
        let dir = std::env::temp_dir().join(format!("dnet_idz_deploy_{}", std::process::id()));
        fs::remove_dir_all(&dir).ok();
        fs::create_dir_all(&dir).unwrap();

        let mut dep = snapshot();
        dep.game_id = Some("SDDF".into());
        deploy(&dir, tool, &dep).expect("the IDZ deployment should succeed");

        assert!(dir.join("inject.exe").is_file(), "the top-level inject.exe of the tool should be copied");
        assert!(
            dir.join("idzhook.dll").is_file(),
            "idz\\idzhook.dll should be copied to the amdaemon folder"
        );
        assert!(dir.join("segatools.ini").is_file());
        assert!(dir.join("option").is_dir() && dir.join("appdata").is_dir());
        assert!(
            !dir.join("config_hook.json").exists(),
            "IDZ does not need config_hook.json"
        );

        let ini = fs::read_to_string(dir.join("segatools.ini")).unwrap();
        assert!(ini.contains("[gpio]"));
        assert!(!ini.contains("[indrun]"));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn aime_path_written_only_when_selected() {
        let mut dep = snapshot();
        dep.aime_card = Some("53314980123456789012".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains(r"aimePath = DEVICE\53314980123456789012.txt"));

        dep.aime_card = None;
        let text = render_segatools_ini(&dep, None);
        assert!(!text.contains("aimePath"), "without a card aimePath is not written");
        assert!(text.contains("enable = 1"));
    }

    #[test]
    fn ffb_strength_keys_live_in_dinput() {
        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        let text = render_segatools_ini(&dep, None);
        let dinput = text.split("[dinput]").nth(1).expect("the [dinput] section should exist");
        for key in [
            "constantForceStrength",
            "damperStrength",
            "rumbleStrength",
            "rumbleDuration",
            "baseDamperFraction",
            "deadband",
        ] {
            assert!(dinput.contains(key), "{key} should be in the [dinput] section");
        }
        let ffb = text.split("[ffb]").nth(1).expect("the [ffb] section should exist");
        assert!(ffb.contains("enable = 1"));
        assert!(!ffb.contains("Strength"), "[ffb] holds no strength keys");

        assert!(!text.contains("true") && !text.contains("false"));
    }

    #[test]
    fn seat_dipsw_mapping() {

        assert_eq!(seat_dipsw(None), (1, 0, 0));
        assert_eq!(seat_dipsw(Some("SINGLE")), (1, 0, 0));
        assert_eq!(seat_dipsw(Some("A1")), (0, 0, 0));
        assert_eq!(seat_dipsw(Some("A2")), (0, 1, 0));
        assert_eq!(seat_dipsw(Some("B1")), (0, 0, 1));
        assert_eq!(seat_dipsw(Some("B2")), (0, 1, 1));
        assert_eq!(seat_dipsw(Some("???")), (1, 0, 0), "an unknown value falls back to single");
    }

    #[test]
    fn sdgt_system_dipsw_only_for_sdgt() {

        let mut dep = snapshot();
        dep.game_id = Some("SDGT".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("dipsw1 = 1"));
        assert!(text.contains("dipsw2 = 0"));
        assert!(text.contains("dipsw3 = 1"));
        assert!(text.contains("dipsw4 = 0"));
        assert!(text.contains("dipsw5 = 0"));
        assert!(!text.contains("[network]"), "addrSuffix is IDZ only (SDDF / SDER)");

        dep.system.seat = Some("A2".into());
        dep.system.master = Some(false);
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("dipsw1 = 0"));
        assert!(text.contains("dipsw3 = 0"));
        assert!(text.contains("dipsw4 = 1"));
        assert!(text.contains("dipsw5 = 0"));

        dep.game_id = Some("SDDF".into());
        dep.system.seat = Some("B2".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("[system]"), "enable/freeplay are still there");
        assert!(text.contains("[gpio]
dipsw1"), "the SDDF host is in [gpio] dipsw1");
        assert!(!text.contains("dipsw2"), "dipsw2 is SDGT only");
        assert!(!text.contains("dipsw3"), "dipsw3 is SDGT only");
        assert!(!text.contains("dipsw4"), "dipsw4 is SDGT only");
        assert!(!text.contains("dipsw5"), "dipsw5 is SDGT only");
    }

    #[test]
    fn dns_uses_server_setting() {
        let mut dep = snapshot();
        dep.net.server = Some(" my.server.net ".into());
        let text = render_segatools_ini(&dep, None);
        assert!(text.contains("default = my.server.net"));
        assert!(!text.contains("sdgt.arcadehub.xyz"), "with a custom server the default domain is not written");
    }

    #[test]
    fn pcbid_serial_format() {

        for _ in 0..16 {
            let sn = pcbid_serial_random();
            assert_eq!(sn.len(), 15, "{sn}");
            assert!(sn.starts_with("ACAE01A"), "{sn}");
            assert!(
                sn[8..].bytes().all(|b| b.is_ascii_digit()),
                "the last 8 characters should be digits: {sn}"
            );
        }
    }

    #[test]
    fn ds_serial_format() {

        for _ in 0..16 {
            let sn = ds_serial_random();
            assert_eq!(sn.len(), 16, "{sn}");
            assert!(sn.starts_with("AAVE-01A"), "{sn}");
            assert!(
                sn[8..].bytes().all(|b| b.is_ascii_digit()),
                "the last 8 characters should be digits: {sn}"
            );
        }
    }

    #[test]
    fn idz_writes_pcbid_serial() {

        for gid in ["SDDF", "SDER"] {
            let mut dep = snapshot();
            dep.game_id = Some(gid.into());
            let text = render_segatools_ini_with_pcbid(
                &dep,
                None,
                Some("ACAE01A12345678"),
                Some("AAVE-01A12345678"),
            );
            assert!(
                text.contains("[pcbid]\nserialNo = ACAE01A12345678"),
                "{gid} should write [pcbid] serialNo:\n{text}"
            );
            assert!(
                text.contains("[ds]\nserialNO = AAVE-01A12345678"),
                "{gid} should write [ds] serialNO:\n{text}"
            );
        }
    }

    #[test]
    fn pcbid_omitted_for_idac_or_missing() {

        let text = render_segatools_ini_with_pcbid(
            &snapshot(),
            None,
            Some("ACAE01A12345678"),
            Some("AAVE-01A12345678"),
        );
        assert!(!text.contains("[pcbid]"), "an unknown game_id writes nothing:\n{text}");
        assert!(!text.contains("serialNO"), "an unknown game_id does not write the ds serial:\n{text}");
        let mut dep = snapshot();
        dep.game_id = Some("SDGT".into());
        let text = render_segatools_ini_with_pcbid(
            &dep,
            None,
            Some("ACAE01A12345678"),
            Some("AAVE-01A12345678"),
        );
        assert!(!text.contains("[pcbid]"), "SDGT does not write it:\n{text}");
        assert!(!text.contains("serialNO"), "SDGT does not write the ds serial:\n{text}");

        dep.game_id = Some("SDDF".into());
        let text = render_segatools_ini_with_pcbid(&dep, None, None, None);
        assert!(!text.contains("[pcbid]"), "without a serial the whole section is left out:\n{text}");
        assert!(!text.contains("serialNO"), "without a ds serial the whole section is left out:\n{text}");
    }
}
