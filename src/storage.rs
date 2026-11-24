use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::i18n::{self, Lang};
use crate::ini;

fn text(v: Option<&str>) -> &str {
    v.unwrap_or("")
}

fn num(v: Option<u32>) -> String {
    v.map(|n| n.to_string()).unwrap_or_default()
}

fn boolv(v: Option<bool>) -> &'static str {
    v.map(|b| if b { "true" } else { "false" })
        .unwrap_or_default()
}

fn flag(v: Option<bool>) -> &'static str {
    match v {
        Some(true) => "1",
        Some(false) => "0",
        None => "",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Loader {
    Kitten,
    Mice,

    Segatools,
}

impl Loader {

    pub fn platform(&self) -> &'static str {
        match self {
            Loader::Kitten => "Lindbergh",
            Loader::Mice => "RingEdge",
            Loader::Segatools => "ALLS",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameRecord {
    pub id: String,
    pub path: String,
    pub name: String,

    pub crc: Option<String>,
    pub version: Option<String>,

    pub arch: Option<String>,
    pub loader: Loader,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub platform: Option<String>,

    pub image: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serverbox: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reader: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amdaemon: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_exe: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub game_id: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gfx_patch: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub test_mode: Option<bool>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub card: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<String>,
    pub play_count: u32,
    pub last_played: Option<u64>,
    pub added_at: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Library {
    pub version: u32,
    pub games: Vec<GameRecord>,
}

impl Default for Library {
    fn default() -> Self {
        Library {
            version: 1,
            games: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {

    pub kitten_path: Option<String>,
    pub mice_path: Option<String>,
    pub mice64_path: Option<String>,

    #[serde(default)]
    pub segatools_path: Option<String>,

    #[serde(default = "default_true")]
    pub auto_check_updates: bool,

    #[serde(default)]
    pub last_kitten_version: Option<String>,
    #[serde(default)]
    pub last_mice_version: Option<String>,
    #[serde(default)]
    pub last_asiloader_version: Option<String>,
    #[serde(default)]
    pub last_shaderfix_version: Option<String>,
    #[serde(default)]
    pub last_segatools_version: Option<String>,

    #[serde(default)]
    pub last_awl_version: Option<String>,
}

fn default_true() -> bool {
    true
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            kitten_path: None,
            mice_path: None,
            mice64_path: None,
            segatools_path: None,
            auto_check_updates: true,
            last_kitten_version: None,
            last_mice_version: None,
            last_asiloader_version: None,
            last_shaderfix_version: None,
            last_segatools_version: None,
            last_awl_version: None,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NetworkConfig {

    pub adapter: Option<String>,

    pub ip: Option<String>,

    pub simulate: Option<bool>,

    pub server: Option<String>,

    pub host_ip: Option<String>,

    pub serverbox_ip: Option<String>,
}

impl NetworkConfig {

    pub fn server_host(&self) -> &str {
        self.server
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .unwrap_or(crate::server::SERVER_HOST)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SystemConfig {

    pub loglevel: Option<String>,

    pub region: Option<String>,

    pub keychip: Option<String>,

    pub skip_reader_init: Option<bool>,

    pub auto_insert_card: Option<bool>,

    pub seat: Option<String>,

    pub master: Option<bool>,
}

fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_lowercase().as_str(),
        "1" | "true" | "on" | "yes"
    )
}

fn load_card_from(path: &Path) -> Option<String> {
    ini::read_value(path, "card", "cardfile").filter(|s| !s.is_empty())
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CoinConfig {

    pub freeplay: Option<bool>,

    pub start_coin: Option<u32>,

    pub continue_coin: Option<u32>,

    pub card_coin: Option<u32>,

    pub car_coin: Option<u32>,
}

fn load_coin_from(path: &Path) -> CoinConfig {
    let r = ini::SectionReader::load(path, "coin");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    CoinConfig {
        freeplay: read("freeplay").map(|v| truthy(&v)),
        start_coin: read("start_coin").and_then(|v| v.trim().parse().ok()),
        continue_coin: read("continue_coin").and_then(|v| v.trim().parse().ok()),
        card_coin: read("card_coin").and_then(|v| v.trim().parse().ok()),
        car_coin: read("car_coin").and_then(|v| v.trim().parse().ok()),
    }
}

fn save_coin_to(path: &Path, cfg: &CoinConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }

    let mut w = ini::writer(path);
    w.set("coin", "freeplay", match cfg.freeplay { Some(true) => "1", Some(false) => "0", None => "", });
    w.set("coin", "start_coin", &num(cfg.start_coin));
    w.set("coin", "continue_coin", &num(cfg.continue_coin));
    w.set("coin", "card_coin", &num(cfg.card_coin));
    w.set("coin", "car_coin", &num(cfg.car_coin));
    w.flush()
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct GraphicsConfig {

    pub default: Option<bool>,

    pub borderless: Option<bool>,

    pub resolution: Option<String>,

    pub render_scale: Option<bool>,

    pub render_scale_level: Option<String>,

    pub msaa: Option<bool>,

    pub msaa_level: Option<String>,

    pub af: Option<bool>,

    pub af_level: Option<String>,
}

fn load_graphics_from(path: &Path) -> GraphicsConfig {
    let r = ini::SectionReader::load(path, "graphics");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    GraphicsConfig {
        default: read("default").map(|v| truthy(&v)),
        borderless: read("borderless").map(|v| truthy(&v)),
        resolution: read("resolution"),
        render_scale: read("render_scale").map(|v| truthy(&v)),
        render_scale_level: read("render_scale_level"),
        msaa: read("msaa").map(|v| truthy(&v)),
        msaa_level: read("msaa_level"),
        af: read("af").map(|v| truthy(&v)),
        af_level: read("af_level"),
    }
}

fn save_graphics_to(path: &Path, cfg: &GraphicsConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }

    let mut w = ini::writer(path);
    w.set("graphics", "default", flag(cfg.default));
    w.set("graphics", "borderless", flag(cfg.borderless));
    w.set("graphics", "resolution", text(cfg.resolution.as_deref()));
    w.set("graphics", "render_scale", flag(cfg.render_scale));
    w.set("graphics", "render_scale_level", text(cfg.render_scale_level.as_deref()));
    w.set("graphics", "msaa", flag(cfg.msaa));
    w.set("graphics", "msaa_level", text(cfg.msaa_level.as_deref()));
    w.set("graphics", "af", flag(cfg.af));
    w.set("graphics", "af_level", text(cfg.af_level.as_deref()));
    w.flush()
}

pub(crate) const DPAD_CODE_BASE: u32 = 1000;

pub const INPUT_RATIO_DEFAULT: u32 = 128;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InputConfig {

    pub kind: Option<String>,

    pub wheel_device: Option<String>,

    pub pedal_device: Option<String>,

    pub shifter_device: Option<String>,

    pub wheel_device_path: Option<String>,

    pub pedal_device_path: Option<String>,

    pub shifter_device_path: Option<String>,

    pub throttle_axis: Option<String>,

    pub brake_axis: Option<String>,

    pub reverse_accel_axis: Option<bool>,

    pub reverse_brake_axis: Option<bool>,

    pub shift_up: Option<u32>,

    pub shift_down: Option<u32>,

    pub start_button: Option<u32>,

    pub view_button: Option<u32>,

    pub coin_button: Option<u32>,

    pub card_button: Option<u32>,

    pub up_button: Option<u32>,

    pub down_button: Option<u32>,

    pub left_button: Option<u32>,

    pub right_button: Option<u32>,

    pub ratio: Option<u32>,

    pub shifter_type: Option<String>,

    pub gear1_button: Option<u32>,

    pub gear2_button: Option<u32>,

    pub gear3_button: Option<u32>,

    pub gear4_button: Option<u32>,

    pub gear5_button: Option<u32>,

    pub gear6_button: Option<u32>,

    pub reverse_button: Option<u32>,

    pub pad_index: Option<u32>,

    pub dual_stick: Option<bool>,

    pub linear_steering: Option<bool>,

    pub left_deadzone: Option<u32>,

    pub right_deadzone: Option<u32>,

    pub xi_start: Option<String>,
    pub xi_view: Option<String>,
    pub xi_up: Option<String>,
    pub xi_down: Option<String>,
    pub xi_left: Option<String>,
    pub xi_right: Option<String>,
    pub xi_shift_dn: Option<String>,
    pub xi_shift_up: Option<String>,
    pub xi_coin: Option<String>,
    pub xi_card: Option<String>,

    pub xi_accel: Option<String>,
    pub xi_brake: Option<String>,

    pub ffb_enable: Option<bool>,

    pub ffb_constant_force: Option<u32>,

    pub ffb_damper: Option<u32>,

    pub ffb_rumble: Option<u32>,

    pub ffb_rumble_duration: Option<u32>,

    pub ffb_base_damper: Option<u32>,

    pub ffb_dead_band: Option<u32>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VpnConfig {

    pub config_path: Option<String>,

    pub passphrase: Option<String>,

    pub openvpn_path: Option<String>,
}

pub fn normalize_device_name(saved: &str) -> &str {
    let trimmed = saved.trim_end_matches(|c: char| c.is_whitespace() || c.is_control());
    for (open, close) in [('（', '）'), ('(', ')')] {
        if trimmed.ends_with(close) {
            if let Some(i) = trimmed.rfind(open) {
                return trimmed[..i].trim_end();
            }
        }
    }
    trimmed
}

fn load_input_from(path: &Path) -> InputConfig {
    let r = ini::SectionReader::load(path, "input");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    let num = |key: &str| read(key).and_then(|v| v.trim().parse().ok());

    let read_device = |key: &str| read(key).map(|v| normalize_device_name(v.trim()).trim().to_string());

    let read_axis = |key: &str| -> Option<String> {
        let raw = read(key)?;
        let up = raw.trim().to_ascii_uppercase();
        let known = ["X", "Y", "Z", "RX", "RY", "RZ", "SLIDER", "DIAL", "WHEEL"];
        if known.contains(&up.as_str()) {
            Some(up)
        } else {
            Some(raw.trim().to_string())
        }
    };
    InputConfig {
        kind: read("type"),
        wheel_device: read_device("wheel_device"),
        pedal_device: read_device("pedal_device"),
        shifter_device: read_device("shifter_device"),

        wheel_device_path: read("wheel_device_path"),
        pedal_device_path: read("pedal_device_path"),
        shifter_device_path: read("shifter_device_path"),

        throttle_axis: read_axis("throttle_axis").or_else(|| read_axis("throttle_button")),
        brake_axis: read_axis("brake_axis").or_else(|| read_axis("brake_button")),
        reverse_accel_axis: read("reverseAccelAxis").map(|v| v.trim() == "true" || v.trim() == "1"),
        reverse_brake_axis: read("reverseBrakeAxis").map(|v| v.trim() == "true" || v.trim() == "1"),
        shift_up: num("shift_up"),
        shift_down: num("shift_down"),
        start_button: num("start_button"),
        view_button: num("view_button"),
        coin_button: num("coin_button"),
        card_button: num("card_button"),
        up_button: num("up_button"),
        down_button: num("down_button"),
        left_button: num("left_button"),
        right_button: num("right_button"),
        ratio: num("ratio"),
        pad_index: num("pad_index"),
        shifter_type: read("shifter_type"),
        gear1_button: num("gear1_button"),
        gear2_button: num("gear2_button"),
        gear3_button: num("gear3_button"),
        gear4_button: num("gear4_button"),
        gear5_button: num("gear5_button"),
        gear6_button: num("gear6_button"),
        reverse_button: num("reverse_button"),
        dual_stick: read("dual_stick").map(|v| v.trim() == "true" || v.trim() == "1"),
        linear_steering: read("linear_steering").map(|v| v.trim() == "true" || v.trim() == "1"),
        left_deadzone: num("left_deadzone"),
        right_deadzone: num("right_deadzone"),
        xi_start: read("xi_start"),
        xi_view: read("xi_view"),
        xi_up: read("xi_up"),
        xi_down: read("xi_down"),
        xi_left: read("xi_left"),
        xi_right: read("xi_right"),
        xi_shift_dn: read("xi_shift_dn"),
        xi_shift_up: read("xi_shift_up"),
        xi_coin: read("xi_coin"),
        xi_card: read("xi_card"),
        xi_accel: read("xi_accel"),
        xi_brake: read("xi_brake"),
        ffb_enable: read("ffb_enable").map(|v| v.trim() == "true" || v.trim() == "1"),
        ffb_constant_force: num("ffb_constant_force"),
        ffb_damper: num("ffb_damper"),
        ffb_rumble: num("ffb_rumble"),
        ffb_rumble_duration: num("ffb_rumble_duration"),
        ffb_base_damper: num("ffb_base_damper"),
        ffb_dead_band: num("ffb_dead_band"),
    }
}

fn save_input_to(path: &Path, cfg: &InputConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }

    let mut w = ini::writer(path);
    w.set("input", "type", text(cfg.kind.as_deref()));
    w.set("input", "wheel_device", text(cfg.wheel_device.as_deref()));
    w.set("input", "pedal_device", text(cfg.pedal_device.as_deref()));
    w.set("input", "shifter_device", text(cfg.shifter_device.as_deref()));

    w.set("input", "wheel_device_path", text(cfg.wheel_device_path.as_deref()));
    w.set("input", "pedal_device_path", text(cfg.pedal_device_path.as_deref()));
    w.set("input", "shifter_device_path", text(cfg.shifter_device_path.as_deref()));
    w.set("input", "throttle_axis", text(cfg.throttle_axis.as_deref()));
    w.set("input", "brake_axis", text(cfg.brake_axis.as_deref()));
    w.set("input", "reverseAccelAxis", boolv(cfg.reverse_accel_axis));
    w.set("input", "reverseBrakeAxis", boolv(cfg.reverse_brake_axis));
    w.set("input", "shift_up", &num(cfg.shift_up));
    w.set("input", "shift_down", &num(cfg.shift_down));
    w.set("input", "start_button", &num(cfg.start_button));
    w.set("input", "view_button", &num(cfg.view_button));
    w.set("input", "coin_button", &num(cfg.coin_button));
    w.set("input", "card_button", &num(cfg.card_button));
    w.set("input", "up_button", &num(cfg.up_button));
    w.set("input", "down_button", &num(cfg.down_button));
    w.set("input", "left_button", &num(cfg.left_button));
    w.set("input", "right_button", &num(cfg.right_button));
    w.set("input", "ratio", &num(cfg.ratio));
    w.set("input", "pad_index", &num(cfg.pad_index));
    w.set("input", "shifter_type", text(cfg.shifter_type.as_deref()));
    w.set("input", "gear1_button", &num(cfg.gear1_button));
    w.set("input", "gear2_button", &num(cfg.gear2_button));
    w.set("input", "gear3_button", &num(cfg.gear3_button));
    w.set("input", "gear4_button", &num(cfg.gear4_button));
    w.set("input", "gear5_button", &num(cfg.gear5_button));
    w.set("input", "gear6_button", &num(cfg.gear6_button));
    w.set("input", "dual_stick", boolv(cfg.dual_stick));
    w.set("input", "linear_steering", boolv(cfg.linear_steering));
    w.set("input", "left_deadzone", &num(cfg.left_deadzone));
    w.set("input", "right_deadzone", &num(cfg.right_deadzone));
    w.set("input", "ffb_enable", boolv(cfg.ffb_enable));
    w.set("input", "ffb_constant_force", &num(cfg.ffb_constant_force));
    w.set("input", "ffb_damper", &num(cfg.ffb_damper));
    w.set("input", "ffb_rumble", &num(cfg.ffb_rumble));
    w.set("input", "ffb_rumble_duration", &num(cfg.ffb_rumble_duration));
    w.set("input", "ffb_base_damper", &num(cfg.ffb_base_damper));
    w.set("input", "ffb_dead_band", &num(cfg.ffb_dead_band));
    w.set("input", "xi_start", text(cfg.xi_start.as_deref()));
    w.set("input", "xi_view", text(cfg.xi_view.as_deref()));
    w.set("input", "xi_up", text(cfg.xi_up.as_deref()));
    w.set("input", "xi_down", text(cfg.xi_down.as_deref()));
    w.set("input", "xi_left", text(cfg.xi_left.as_deref()));
    w.set("input", "xi_right", text(cfg.xi_right.as_deref()));
    w.set("input", "xi_shift_dn", text(cfg.xi_shift_dn.as_deref()));
    w.set("input", "xi_shift_up", text(cfg.xi_shift_up.as_deref()));
    w.set("input", "xi_coin", text(cfg.xi_coin.as_deref()));
    w.set("input", "xi_card", text(cfg.xi_card.as_deref()));
    w.set("input", "xi_accel", text(cfg.xi_accel.as_deref()));
    w.set("input", "xi_brake", text(cfg.xi_brake.as_deref()));
    w.set("input", "reverse_button", &num(cfg.reverse_button));
    w.flush()
}

fn load_vpn_from(path: &Path) -> VpnConfig {
    let r = ini::SectionReader::load(path, "vpn");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    VpnConfig {
        config_path: read("config"),
        passphrase: read("passphrase").map(|s| crate::win_util::unprotect_secret(&s)),
        openvpn_path: read("openvpn"),
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RoomConfig {

    pub directory: Option<String>,

    pub awl_path: Option<String>,

    pub last_room_id: Option<String>,

    pub last_room_password: Option<String>,

    pub nickname: Option<String>,
}

fn load_room_from(path: &Path) -> RoomConfig {
    let r = ini::SectionReader::load(path, "room");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    RoomConfig {
        directory: read("directory"),
        awl_path: read("awl"),
        last_room_id: read("last_room"),
        last_room_password: read("last_password").map(|s| crate::win_util::unprotect_secret(&s)),
        nickname: read("nickname"),
    }
}

fn save_room_to(path: &Path, cfg: &RoomConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }
    let mut w = ini::writer(path);
    w.set("room", "directory", text(cfg.directory.as_deref()));
    w.set("room", "awl", text(cfg.awl_path.as_deref()));
    w.set("room", "last_room", text(cfg.last_room_id.as_deref()));
    w.set("room", "nickname", text(cfg.nickname.as_deref()));
    w.set("room", "last_password", text( cfg.last_room_password .as_deref() .map(crate::win_util::protect_secret) .as_deref(), ));
    w.flush()
}

fn save_vpn_to(path: &Path, cfg: &VpnConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }
    let mut w = ini::writer(path);
    w.set("vpn", "config", text(cfg.config_path.as_deref()));
    w.set("vpn", "passphrase", text( cfg.passphrase .as_deref() .map(crate::win_util::protect_secret) .as_deref(), ));
    w.set("vpn", "openvpn", text(cfg.openvpn_path.as_deref()));
    w.flush()
}

fn load_network_from(path: &Path) -> NetworkConfig {
    let r = ini::SectionReader::load(path, "network");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    NetworkConfig {
        adapter: read("adapter"),
        ip: read("ip"),
        simulate: read("simulate").map(|v| truthy(&v)),
        server: read("server"),
        host_ip: read("host_ip"),
        serverbox_ip: read("serverbox_ip"),
    }
}

fn save_network_to(path: &Path, cfg: &NetworkConfig) -> io::Result<()> {
    if !path.exists() {
        fs::write(path, "; Sasayaki Launcher settings\n")?;
    }
    let mut w = ini::writer(path);
    w.set("network", "adapter", cfg.adapter.as_deref().unwrap_or(""));
    w.set("network", "ip", cfg.ip.as_deref().unwrap_or(""));
    w.set("network", "simulate", match cfg.simulate { Some(true) => "1", Some(false) => "0", None => "", });
    w.set("network", "server", cfg.server.as_deref().unwrap_or(""));
    w.set("network", "host_ip", cfg.host_ip.as_deref().unwrap_or(""));
    w.set("network", "serverbox_ip", cfg.serverbox_ip.as_deref().unwrap_or(""));
    w.flush()
}

fn load_system_from(path: &Path) -> SystemConfig {
    let r = ini::SectionReader::load(path, "system");
    let read = |key: &str| r.get(key).filter(|s| !s.is_empty());
    SystemConfig {
        loglevel: read("loglevel"),
        region: read("region"),
        keychip: read("keychip"),
        skip_reader_init: read("skip_reader_init").map(|v| truthy(&v)),
        auto_insert_card: read("auto_insert_card").map(|v| truthy(&v)),
        seat: read("seat"),
        master: read("master").map(|v| truthy(&v)),
    }
}

/// Writes the `[system]` keys that belong to a game ([`GAME_SYSTEM_KEYS`]).
fn save_game_system_to(path: &Path, cfg: &SystemConfig) -> io::Result<()> {
    let mut w = ini::writer(path);
    w.set("system", "loglevel", cfg.loglevel.as_deref().unwrap_or(""));
    w.set("system", "skip_reader_init", flag(cfg.skip_reader_init));
    w.set("system", "auto_insert_card", flag(cfg.auto_insert_card));
    w.set("system", "seat", cfg.seat.as_deref().unwrap_or(""));
    w.set("system", "master", flag(cfg.master));
    w.flush()
}

/// The `[system]` keys that belong to a game. The others (keychip, region) stay in
/// `config.ini`: the keychip also names this PC in the rooms.
const GAME_SYSTEM_KEYS: [&str; 5] = ["loglevel", "skip_reader_init", "auto_insert_card", "seat", "master"];

/// What a game's settings file holds, which `config.ini` kept for every game before each
/// game had a file of its own: whole sections (`None`), or the keys listed of a section.
const GAME_SETTINGS: [(&str, Option<&[&str]>); 5] = [
    ("graphics", None),
    ("coin", None),
    ("input", None),
    ("wmmt3", None),
    ("system", Some(&GAME_SYSTEM_KEYS)),
];

/// The settings a game keeps in its own file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameSettings {
    /// Only the [`GAME_SYSTEM_KEYS`] are set.
    pub system: SystemConfig,
    pub coin: CoinConfig,
    pub graphics: GraphicsConfig,
    pub input: InputConfig,
}

fn load_game_settings_from(path: &Path) -> GameSettings {
    let mut system = load_system_from(path);
    system.keychip = None;
    system.region = None;
    GameSettings {
        system,
        coin: load_coin_from(path),
        graphics: load_graphics_from(path),
        input: load_input_from(path),
    }
}

/// Creates a game's settings file, with a first line naming the game, when it does not exist.
fn create_game_settings_file(path: &Path, game_name: &str) -> io::Result<()> {
    if path.exists() {
        return Ok(());
    }
    if let Some(dir) = path.parent() {
        ensure_dir(dir)?;
    }
    fs::write(path, format!("; Sasayaki Launcher settings of {game_name}\n"))
}

/// Moves the game settings that `config.ini` kept for every game into the games' own files.
///
/// Each game without a file of its own gets one with those settings, so it launches as it
/// did before. Once every game has its file, the settings are taken out of `config.ini`.
/// With no game in the library they stay there, for the first game added.
fn move_shared_settings(config: &Path, games_dir: &Path, games: &[GameRecord]) -> io::Result<()> {
    let mut shared: Vec<(&str, String, String)> = Vec::new();
    for (section, keys) in GAME_SETTINGS {
        for (key, value) in ini::section_entries(config, section) {
            if keys.is_none_or(|keys| keys.iter().any(|k| k.eq_ignore_ascii_case(&key))) {
                shared.push((section, key, value));
            }
        }
    }
    if shared.is_empty() || games.is_empty() {
        return Ok(());
    }

    let entries: Vec<(&str, &str, &str)> = shared.iter().map(|(s, k, v)| (*s, k.as_str(), v.as_str())).collect();
    for game in games {
        let path = game_settings_file_in(games_dir, game);
        if path.exists() {
            continue;
        }
        create_game_settings_file(&path, &game.name)?;
        ini::set_values(&path, &entries)?;
    }

    for (section, keys) in GAME_SETTINGS {
        ini::remove_entries(config, section, keys)?;
    }
    Ok(())
}

/// The settings file of `game` in `games_dir`.
fn game_settings_file_in(games_dir: &Path, game: &GameRecord) -> PathBuf {
    games_dir.join(format!("{}.ini", game.id))
}

pub fn launcher_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

pub fn data_dir() -> PathBuf {
    launcher_dir().join("data")
}

pub fn images_dir() -> PathBuf {
    data_dir().join("images")
}

fn ensure_dir(p: &Path) -> io::Result<()> {
    fs::create_dir_all(p)
}

fn load_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Option<T> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str(&text) {
        Ok(v) => Some(v),

        Err(e) => {
            let bad = path.with_extension("json.bad");
            let _ = fs::rename(path, &bad);
            eprintln!("{} failed to parse (backed up as {}): {e}", path.display(), bad.display());
            None
        }
    }
}

fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        ensure_dir(dir)?;
    }
    let tmp = path.with_extension("tmp");
    let text = serde_json::to_string_pretty(value).map_err(io::Error::other)?;
    fs::write(&tmp, text)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

pub struct Store {
    dir: PathBuf,
}

impl Store {
    pub fn new() -> Store {
        Store { dir: data_dir() }
    }

    fn library_path(&self) -> PathBuf {
        self.dir.join("library.json")
    }

    fn settings_path(&self) -> PathBuf {
        self.dir.join("settings.json")
    }

    /// The launcher's `config.ini`.
    pub fn config_path(&self) -> PathBuf {
        launcher_dir().join("config.ini")
    }

    pub fn load_library(&self) -> Library {
        load_json(&self.library_path()).unwrap_or_default()
    }

    pub fn save_library(&self, lib: &Library) {
        if let Err(e) = save_json(&self.library_path(), lib) {
            eprintln!("failed to save library.json: {e}");
        }
    }

    pub fn load_settings(&self) -> Settings {
        load_json(&self.settings_path()).unwrap_or_default()
    }

    pub fn save_settings(&self, s: &Settings) {
        if let Err(e) = save_json(&self.settings_path(), s) {
            eprintln!("failed to save settings.json: {e}");
        }
    }

    pub fn load_network(&self) -> NetworkConfig {
        load_network_from(&self.config_path())
    }

    pub fn save_network(&self, cfg: &NetworkConfig) -> io::Result<()> {
        save_network_to(&self.config_path(), cfg)
    }

    pub fn load_vpn(&self) -> VpnConfig {
        load_vpn_from(&self.config_path())
    }

    pub fn save_vpn(&self, cfg: &VpnConfig) -> io::Result<()> {
        save_vpn_to(&self.config_path(), cfg)
    }

    pub fn load_room(&self) -> RoomConfig {
        load_room_from(&self.config_path())
    }

    pub fn save_room(&self, cfg: &RoomConfig) -> io::Result<()> {
        save_room_to(&self.config_path(), cfg)
    }

    /// The `[system]` settings of `config.ini`: the keychip, and the region a game without one
    /// of its own falls back to.
    pub fn load_system(&self) -> SystemConfig {
        load_system_from(&self.config_path())
    }

    pub fn save_keychip(&self, keychip: Option<&str>) -> io::Result<()> {
        let path = self.config_path();
        if !path.exists() {
            fs::write(&path, "; Sasayaki Launcher settings\n")?;
        }
        ini::set_value(&path, "system", "keychip", keychip.unwrap_or(""))
    }

    /// The folder of the games' own settings files, next to `config.ini`.
    fn games_dir(&self) -> PathBuf {
        launcher_dir().join("games")
    }

    /// The file of a game's own settings: `games\<id>.ini`.
    pub fn game_settings_path(&self, game: &GameRecord) -> PathBuf {
        game_settings_file_in(&self.games_dir(), game)
    }

    pub fn load_game_settings(&self, game: &GameRecord) -> GameSettings {
        load_game_settings_from(&self.game_settings_path(game))
    }

    /// The game's settings file, created when it does not exist yet.
    pub fn ensure_game_settings_file(&self, game: &GameRecord) -> io::Result<PathBuf> {
        let path = self.game_settings_path(game);
        create_game_settings_file(&path, &game.name)?;
        Ok(path)
    }

    pub fn save_game_system(&self, game: &GameRecord, cfg: &SystemConfig) -> io::Result<()> {
        save_game_system_to(&self.ensure_game_settings_file(game)?, cfg)
    }

    pub fn save_game_coin(&self, game: &GameRecord, cfg: &CoinConfig) -> io::Result<()> {
        save_coin_to(&self.ensure_game_settings_file(game)?, cfg)
    }

    pub fn save_game_graphics(&self, game: &GameRecord, cfg: &GraphicsConfig) -> io::Result<()> {
        save_graphics_to(&self.ensure_game_settings_file(game)?, cfg)
    }

    pub fn save_game_input(&self, game: &GameRecord, cfg: &InputConfig) -> io::Result<()> {
        save_input_to(&self.ensure_game_settings_file(game)?, cfg)
    }

    /// Deletes a game's settings file, for a game taken out of the library.
    pub fn remove_game_settings(&self, game: &GameRecord) {
        let _ = fs::remove_file(self.game_settings_path(game));
    }

    /// Gives each game in `games` the settings `config.ini` kept for every game, in a file of
    /// its own (see [`move_shared_settings`]).
    pub fn move_shared_settings(&self, games: &[GameRecord]) -> io::Result<()> {
        move_shared_settings(&self.config_path(), &self.games_dir(), games)
    }

    /// The interface language chosen in `config.ini [ui] lang`, or the system's language
    /// when none (or an unknown one) is stored.
    pub fn load_lang(&self) -> Lang {
        ini::read_value(&self.config_path(), "ui", "lang")
            .as_deref()
            .and_then(Lang::from_ini_value)
            .unwrap_or_else(i18n::detect_system)
    }

    pub fn save_lang(&self, lang: Lang) -> io::Result<()> {
        let path = self.config_path();
        if !path.exists() {
            fs::write(&path, "; Sasayaki Launcher settings\n")?;
        }
        ini::set_value(&path, "ui", "lang", lang.ini_value())
    }

    pub fn load_card(&self) -> Option<String> {
        load_card_from(&self.config_path())
    }

    pub fn remove_image(&self, id: &str) {
        if let Ok(entries) = fs::read_dir(images_dir()) {
            for e in entries.flatten() {
                if e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("{id}."))
                {
                    let _ = fs::remove_file(e.path());
                }
            }
        }
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_config(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join("dnet_storage_test");
        fs::create_dir_all(&dir).unwrap();
        dir.join(name)
    }

    #[test]
    fn device_name_suffix_is_stripped_on_load() {
        assert_eq!(
            normalize_device_name("Thrustmaster Advance Racer（VID_044F&PID_B696&MI_00）"),
            "Thrustmaster Advance Racer"
        );
        assert_eq!(
            normalize_device_name("Thrustmaster Advance Racer(VID_044F&PID_B696&MI_00)"),
            "Thrustmaster Advance Racer"
        );

        assert_eq!(normalize_device_name("Wireless Controller"), "Wireless Controller");
        assert_eq!(normalize_device_name("R3 Racing Wheel and Pedals\u{90}"), "R3 Racing Wheel and Pedals");
        assert_eq!(normalize_device_name("  Wheel  "), "  Wheel");
    }

    #[test]
    fn legacy_suffixed_device_name_normalized_from_ini() {
        let path = temp_config("input_legacy.ini");
        let _ = fs::remove_file(&path);
        ini::set_value(&path, "input", "type", "dinput").unwrap();
        ini::set_value(
            &path,
            "input",
            "wheel_device",
            "Thrustmaster Advance Racer（VID_044F&PID_B696&MI_00）",
        )
        .unwrap();
        let cfg = load_input_from(&path);
        assert_eq!(cfg.wheel_device.as_deref(), Some("Thrustmaster Advance Racer"));
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn game_system_config_roundtrip() {
        let path = temp_config("system_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_game_settings_from(&path), GameSettings::default());

        let cfg = SystemConfig {
            loglevel: Some("debug".into()),
            skip_reader_init: Some(true),
            auto_insert_card: Some(true),
            seat: Some("A1".into()),
            master: Some(false),
            ..Default::default()
        };
        save_game_system_to(&path, &cfg).unwrap();
        assert_eq!(load_game_settings_from(&path).system, cfg);

        save_game_system_to(&path, &SystemConfig { seat: Some("B2".into()), ..Default::default() }).unwrap();
        let loaded = load_game_settings_from(&path).system;
        assert_eq!(loaded.seat, Some("B2".into()));
        assert_eq!(loaded.loglevel, None);
        assert_eq!(loaded.skip_reader_init, None);

        // The keychip and the region are the launcher's, not the game's.
        fs::write(&path, "[system]\nkeychip = A69E-01A99999999\nregion = JPN\nskip_reader_init = true\n").unwrap();
        let loaded = load_game_settings_from(&path).system;
        assert_eq!(loaded.skip_reader_init, Some(true));
        assert_eq!(loaded.keychip, None);
        assert_eq!(loaded.region, None);

        fs::remove_file(&path).ok();
    }

    /// A fresh folder with a `config.ini` holding settings shared by every game, as the
    /// launcher kept them before each game had a file of its own.
    fn shared_settings_folder(name: &str) -> (PathBuf, PathBuf, PathBuf) {
        let dir = temp_config(name);
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let config = dir.join("config.ini");
        fs::write(
            &config,
            "; Sasayaki Launcher settings\n[system]\nloglevel = debug\nkeychip = A69E-01A99999999\nseat = A1\n\
             [input]\ntype = dinput\nratio = 200\n[network]\nip = 192.168.1.0/24\n[graphics]\nborderless = 1\n",
        )
        .unwrap();
        (dir.clone(), config, dir.join("games"))
    }

    fn game(id: &str) -> GameRecord {
        GameRecord { id: id.into(), ..crate::games::test_input::record(Loader::Kitten, Some("SBQZ")) }
    }

    #[test]
    fn shared_settings_move_into_each_game() {
        let (dir, config, games_dir) = shared_settings_folder("move_shared");
        let games = [game("one"), game("two")];
        move_shared_settings(&config, &games_dir, &games).unwrap();

        for g in &games {
            let saved = load_game_settings_from(&game_settings_file_in(&games_dir, g));
            assert_eq!(saved.input.kind.as_deref(), Some("dinput"));
            assert_eq!(saved.input.ratio, Some(200));
            assert_eq!(saved.graphics.borderless, Some(true));
            assert_eq!(saved.system.loglevel.as_deref(), Some("debug"));
            assert_eq!(saved.system.seat.as_deref(), Some("A1"));
        }
        let left = fs::read_to_string(&config).unwrap();
        assert_eq!(
            left,
            "; Sasayaki Launcher settings\n[system]\nkeychip = A69E-01A99999999\n[network]\nip = 192.168.1.0/24\n",
            "only the launcher's own settings stay"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_game_with_its_own_file_keeps_it() {
        let (dir, config, games_dir) = shared_settings_folder("move_keeps_own");
        let own = game("own");
        fs::create_dir_all(&games_dir).unwrap();
        fs::write(game_settings_file_in(&games_dir, &own), "[input]\ntype = xinput\n").unwrap();

        move_shared_settings(&config, &games_dir, &[own.clone(), game("new")]).unwrap();
        assert_eq!(load_game_settings_from(&game_settings_file_in(&games_dir, &own)).input.kind.as_deref(), Some("xinput"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn without_games_the_shared_settings_wait_for_the_first() {
        let (dir, config, games_dir) = shared_settings_folder("move_no_games");
        let before = fs::read_to_string(&config).unwrap();
        move_shared_settings(&config, &games_dir, &[]).unwrap();
        assert_eq!(fs::read_to_string(&config).unwrap(), before);
        assert!(!games_dir.exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn coin_config_roundtrip() {
        let path = temp_config("coin_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_coin_from(&path), CoinConfig::default());

        let cfg = CoinConfig {
            freeplay: Some(true),
            start_coin: Some(1),
            continue_coin: Some(2),
            card_coin: Some(3),
            car_coin: Some(5),
        };
        save_coin_to(&path, &cfg).unwrap();
        assert_eq!(load_coin_from(&path), cfg);

        save_coin_to(&path, &CoinConfig::default()).unwrap();
        assert_eq!(load_coin_from(&path), CoinConfig::default());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn graphics_config_roundtrip() {
        let path = temp_config("graphics_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_graphics_from(&path), GraphicsConfig::default());

        let cfg = GraphicsConfig {
            default: Some(false),
            borderless: Some(true),
            resolution: Some("1920x1080".into()),
            render_scale: Some(true),
            render_scale_level: Some("2x".into()),
            msaa: Some(true),
            msaa_level: Some("4x".into()),
            af: Some(false),
            af_level: Some("16x".into()),
        };
        save_graphics_to(&path, &cfg).unwrap();
        assert_eq!(load_graphics_from(&path), cfg);

        save_graphics_to(&path, &GraphicsConfig::default()).unwrap();
        assert_eq!(load_graphics_from(&path), GraphicsConfig::default());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn input_config_roundtrip() {
        let path = temp_config("input_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_input_from(&path), InputConfig::default());

        let cfg = InputConfig {
            kind: Some("dinput".into()),
            wheel_device: Some(r"\\?\HID#VID_044F".into()),
            pedal_device: Some(r"\\?\HID#VID_044F&MI_01".into()),
            shifter_device: Some(r"\\?\HID#VID_044F&MI_02".into()),
            wheel_device_path: Some(r"\\?\HID#VID_044F&PID_B696&MI_00".into()),
            pedal_device_path: Some(r"\\?\HID#VID_044F&PID_B696&MI_01".into()),
            shifter_device_path: Some(r"\\?\HID#VID_044F&PID_B696&MI_02".into()),
            throttle_axis: Some("Y".into()),
            brake_axis: Some("RZ".into()),
            shift_up: Some(5),
            shift_down: Some(6),
            start_button: Some(9),
            view_button: Some(10),
            coin_button: Some(11),
            card_button: Some(12),
            up_button: Some(13),
            down_button: Some(14),
            left_button: Some(15),
            right_button: Some(16),
            ratio: Some(128),
            pad_index: Some(2),
            shifter_type: Some("h".into()),
            gear1_button: Some(17),
            gear2_button: Some(18),
            gear3_button: Some(19),
            gear4_button: Some(20),
            gear5_button: Some(21),
            gear6_button: Some(22),
            reverse_button: Some(23),
            reverse_accel_axis: Some(true),
            reverse_brake_axis: Some(false),
            dual_stick: Some(true),
            linear_steering: Some(false),
            left_deadzone: Some(15),
            right_deadzone: Some(25),
            ffb_enable: Some(true),
            ffb_constant_force: Some(80),
            ffb_damper: Some(70),
            ffb_rumble: Some(60),
            ffb_rumble_duration: Some(90),
            ffb_base_damper: Some(10),
            ffb_dead_band: Some(15),
            xi_start: Some("A".into()),
            xi_view: Some("B".into()),
            xi_up: Some("DPAD_UP".into()),
            xi_down: Some("DPAD_DOWN".into()),
            xi_left: Some("DPAD_LEFT".into()),
            xi_right: Some("DPAD_RIGHT".into()),
            xi_shift_dn: Some("LB".into()),
            xi_shift_up: Some("RB".into()),
            xi_coin: Some("Y".into()),
            xi_card: Some("START".into()),
            xi_accel: Some("RT".into()),
            xi_brake: Some("LT".into()),
        };
        save_input_to(&path, &cfg).unwrap();
        assert_eq!(load_input_from(&path), cfg);

        save_input_to(&path, &InputConfig::default()).unwrap();
        assert_eq!(load_input_from(&path), InputConfig::default());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn room_config_nickname_roundtrip() {
        let path = temp_config("room_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_room_from(&path), RoomConfig::default());

        let cfg = RoomConfig {
            directory: Some("https://dir.example/roomapi".into()),
            awl_path: Some("C:/tools/awl-tray.exe".into()),
            last_room_id: Some("ABCD1234".into()),
            last_room_password: Some("pw".into()),
            nickname: Some("玩家一号".into()),
        };
        save_room_to(&path, &cfg).unwrap();
        assert_eq!(load_room_from(&path), cfg);

        save_room_to(&path, &RoomConfig::default()).unwrap();
        assert_eq!(load_room_from(&path), RoomConfig::default());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn network_config_roundtrip() {
        let path = temp_config("network_roundtrip.ini");
        fs::remove_file(&path).ok();

        assert_eq!(load_network_from(&path), NetworkConfig::default());

        let cfg = NetworkConfig {
            adapter: Some("{GUID}".into()),
            ip: Some("192.168.100.0/24".into()),
            simulate: Some(false),
            server: Some("my.server.net".into()),
            host_ip: Some("192.168.100.50".into()),
            serverbox_ip: Some("192.168.100.101".into()),
        };
        save_network_to(&path, &cfg).unwrap();
        assert_eq!(load_network_from(&path), cfg);

        save_network_to(&path, &NetworkConfig::default()).unwrap();
        assert_eq!(load_network_from(&path), NetworkConfig::default());

        fs::remove_file(&path).ok();
    }

    #[test]
    fn game_record_legacy_and_roundtrip() {

        let legacy = r#"{"id":"a","path":"C:/g/game.exe","name":"D8 JPN","crc":"0x1fd6136a","version":"2.01","arch":"x64","loader":"mice","image":null,"play_count":0,"last_played":null,"added_at":1700000000}"#;
        let rec: GameRecord = serde_json::from_str(legacy).expect("an old record without the new fields should parse");
        assert_eq!(rec.serverbox, None);
        assert_eq!(rec.reader, None);

        let json = serde_json::to_string(&rec).unwrap();
        assert!(!json.contains("serverbox"));
        assert!(!json.contains("reader"));

        let mut rec2 = rec.clone();
        rec2.serverbox = Some("C:/sb/server.exe".into());
        rec2.reader = Some("C:/g/picodaemon.exe".into());
        let json2 = serde_json::to_string(&rec2).unwrap();
        assert!(json2.contains("serverbox"));
        let back: GameRecord = serde_json::from_str(&json2).expect("the configured record should read back");
        assert_eq!(back.serverbox.as_deref(), Some("C:/sb/server.exe"));
        assert_eq!(back.reader.as_deref(), Some("C:/g/picodaemon.exe"));
    }

    #[test]
    fn game_record_card_is_per_game_and_legacy_safe() {

        let legacy = r#"{"id":"a","path":"C:/g/game.exe","name":"D8 JPN","crc":"0x1fd6136a","version":"2.01","arch":"x64","loader":"mice","play_count":0,"last_played":null,"added_at":1700000000}"#;
        let rec: GameRecord = serde_json::from_str(legacy).expect("an old record without card should parse");
        assert_eq!(rec.card, None);

        let json = serde_json::to_string(&rec).unwrap();
        assert!(!json.contains("\"card\""), "card=None should not be serialized");

        let mut rec2 = rec.clone();
        rec2.card = Some("InidCrd01234.png".into());
        let json2 = serde_json::to_string(&rec2).unwrap();
        assert!(json2.contains("\"card\":\"InidCrd01234.png\""));
        let back: GameRecord = serde_json::from_str(&json2).expect("the record with a card should read back");
        assert_eq!(back.card.as_deref(), Some("InidCrd01234.png"));
    }
}
