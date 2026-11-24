use std::fs;
use std::path::{Path, PathBuf};

use rust_i18n::t;

use super::mice;
use super::render_util::{bool_str, copy_tool_files, deadzone_value, nx_num, push_xi_button, Sections, split_resolution};
use crate::storage::{
    CoinConfig, DPAD_CODE_BASE, GameRecord, GraphicsConfig, InputConfig, INPUT_RATIO_DEFAULT,
    NetworkConfig, SystemConfig,
};

#[derive(Clone, Debug)]
pub struct DeploySnapshot {
    pub net: NetworkConfig,
    pub system: SystemConfig,
    pub coin: CoinConfig,
    pub graphics: GraphicsConfig,
    pub input: InputConfig,

    pub game_id: Option<String>,
}

/// kitten's `[sysconf] log_level` for a launcher log level.
pub fn log_level_num(v: &str) -> Option<u32> {
    match v.trim().to_ascii_lowercase().as_str() {
        "error" => Some(5),
        "warn" => Some(4),
        "info" => Some(3),
        "debug" => Some(1),
        "trace" => Some(0),
        _ => None,
    }
}

fn region_num(v: &str) -> Option<u32> {
    match v.trim().to_ascii_uppercase().as_str() {
        "JPN" => Some(0),
        "EXP" | "CHN" => Some(2),
        _ => None,
    }
}

pub fn render(dep: &DeploySnapshot) -> String {
    let mut s = Sections::new();

    if dep.graphics.default != Some(true) {
        if let Some((w, h)) = dep.graphics.resolution.as_deref().and_then(split_resolution) {

            s.push("display", "dipsw", "false");
            s.push("display", "width", &w.to_string());
            s.push("display", "height", &h.to_string());
        }
        if let Some(b) = dep.graphics.borderless {
            s.push("display", "borderless", bool_str(b));
        }
        if dep.graphics.render_scale == Some(true) {
            if let Some(n) = dep.graphics.render_scale_level.as_deref().and_then(nx_num) {
                s.push("display", "render_scale", &n.to_string());
            }
        }
        if dep.graphics.msaa == Some(true) {
            if let Some(n) = dep.graphics.msaa_level.as_deref().and_then(nx_num) {
                s.push("display", "msaa_samples", &n.to_string());
            }
        }
        if dep.graphics.af == Some(true) {
            if let Some(n) = dep.graphics.af_level.as_deref().and_then(nx_num) {
                s.push("display", "anisotropic", &n.to_string());
            }
        }
    }

    s.push("network", "redirect", dep.net.server_host());
    if let Some(ip) = &dep.net.ip {
        s.push("network", "subnet", ip);
    }

    let sim = dep.net.simulate.unwrap_or(true);
    s.push("network", "simulateNetwork", bool_str(sim));

    if dep.net.simulate == Some(false) {

        if let Some(ip) = dep.net.serverbox_ip.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            s.push("network", "bind_ip", ip);
        }

        if let Some(ip) = dep.net.host_ip.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            s.push("network", "game_ip", ip);
        }
    }

    if let Some(lv) = &dep.system.loglevel {
        if let Some(n) = log_level_num(lv) {
            s.push("sysconf", "log_level", &n.to_string());
        }
    }
    if let Some(r) = &dep.system.region {
        if let Some(n) = region_num(r) {
            s.push("sysconf", "region", &n.to_string());

            if r.trim().eq_ignore_ascii_case("CHN") {
                s.push("sysconf", "dipsw", "0x3A");
            }
        }
    }
    if let Some(id) = &dep.game_id {
        if !id.is_empty() {
            s.push("sysconf", "game_id", id);
        }
    }
    if let Some(k) = &dep.system.keychip {
        if !k.trim().is_empty() {
            s.push("sysconf", "keychip_serial", k.trim());
        }
    }
    if let Some(v) = dep.system.skip_reader_init {
        s.push("sysconf", "skipInitCard", bool_str(v));
    }

    if let Some(fp) = dep.coin.freeplay {
        s.push("Coins", "freeplay", bool_str(fp));
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

    match dep.input.kind.as_deref().map(|v| v.to_ascii_lowercase()).as_deref() {
        Some("xinput") => s.push("io3", "mode", "1"),
        Some("dinput") => s.push("io3", "mode", "2"),
        _ => {}
    }

    s.push(
        "io3",
        "restrict_",
        &dep.input.ratio.unwrap_or(INPUT_RATIO_DEFAULT).to_string(),
    );

    if dep
        .input
        .kind
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("xinput"))
        .unwrap_or(false)
    {

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

        push_xi_button(&mut s, "xinput", "accel", &dep.input.xi_accel);
        push_xi_button(&mut s, "xinput", "brake", &dep.input.xi_brake);
    }

    if dep
        .input
        .kind
        .as_deref()
        .map(|v| v.eq_ignore_ascii_case("dinput"))
        .unwrap_or(false)
    {
        if let Some(v) = dep.input.wheel_device.as_deref() {
            s.push("dinput", "deviceName", v.trim_matches(|c: char| c.is_whitespace() || c.is_control()));
        }
        if let Some(v) = dep.input.pedal_device.as_deref() {
            s.push("dinput", "pedalsName", v.trim_matches(|c: char| c.is_whitespace() || c.is_control()));
        }
        if let Some(v) = dep.input.shifter_device.as_deref() {
            s.push("dinput", "shifterName", v.trim_matches(|c: char| c.is_whitespace() || c.is_control()));
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
            s.push("dinput", "accelAxis", &n.trim().to_ascii_uppercase());
        }
        if let Some(ref n) = dep.input.brake_axis {
            s.push("dinput", "brakeAxis", &n.trim().to_ascii_uppercase());
        }

        s.push("dinput", "reverseAccelAxis", bool_str(dep.input.reverse_accel_axis.unwrap_or(false)));
        s.push("dinput", "reverseBrakeAxis", bool_str(dep.input.reverse_brake_axis.unwrap_or(false)));
    }

    s.push("ffb", "enable", bool_str(dep.input.ffb_enable.unwrap_or(true)));
    s.push("ffb", "constantForceStrength", &dep.input.ffb_constant_force.unwrap_or(100).to_string());
    s.push("ffb", "damperStrength", &dep.input.ffb_damper.unwrap_or(100).to_string());
    s.push("ffb", "rumbleStrength", &dep.input.ffb_rumble.unwrap_or(100).to_string());
    s.push("ffb", "rumbleDuration", &dep.input.ffb_rumble_duration.unwrap_or(100).to_string());
    s.push("ffb", "baseDamperStrength", &dep.input.ffb_base_damper.unwrap_or(20).to_string());
    s.push("ffb", "deadBand", &dep.input.ffb_dead_band.unwrap_or(20).to_string());

    s.to_string()
}

const EXCLUDE_FILES: &[&str] = &["config.ini", "sram.bin", "eeprom.bin"];

/// kitten's config file in a game folder.
pub fn config_path(game_dir: &Path) -> PathBuf {
    game_dir.join("kitten").join("config.ini")
}

/// Copies kitten's files from `tool_dir` into the folder of the game file `game`; returns
/// the kitten.exe there, which starts the game.
pub fn install(tool_dir: &Path, game: &Path) -> Result<PathBuf, String> {
    if !game.is_file() {
        return Err(t!("common.game_file_does_not_exist", game = game.display()).to_string());
    }
    let Some(game_dir) = game.parent() else {
        return Err(t!("kitten.cannot_determine_game_directory", game = game.display()).to_string());
    };
    if !game_dir.is_dir() {
        return Err(t!("common.game_directory_does_not_exist", game_dir = game_dir.display()).to_string());
    }
    copy_tool_files(tool_dir, game_dir, EXCLUDE_FILES, "kitten")?;
    Ok(game_dir.join("kitten.exe"))
}

/// Installs kitten for an Initial D game and writes its `kitten\config.ini` and the
/// `mice\config.ini` its card reader reads.
pub fn deploy(
    record: &GameRecord,
    tool_dir: &Path,
    dep: &DeploySnapshot,
) -> Result<PathBuf, String> {
    let game = Path::new(&record.path);
    let kitten_exe = install(tool_dir, game)?;
    let game_dir = kitten_exe.parent().unwrap_or(Path::new("."));

    let cfg_dir = game_dir.join("kitten");
    fs::create_dir_all(&cfg_dir)
        .map_err(|e| t!("kitten.failed_to_create_kitten_config", cfg_dir = cfg_dir.display(), error = e).to_string())?;
    write_config(&cfg_dir, dep)?;

    let mice_dep = mice::DeploySnapshot {
        net: dep.net.clone(),
        system: dep.system.clone(),
        coin: dep.coin.clone(),
        graphics: dep.graphics.clone(),
        input: dep.input.clone(),
        card: None,
        room: None,
    };
    let mice_dir = game_dir.join("mice");
    fs::create_dir_all(&mice_dir)
        .map_err(|e| t!("kitten.failed_to_create_mice_config", mice_dir = mice_dir.display(), error = e).to_string())?;
    mice::write_atomic(&mice_dir.join("config.ini"), &mice::render_config(&mice_dep, mice::MiceTarget::Game))?;

    Ok(kitten_exe)
}

fn write_config(cfg_dir: &Path, dep: &DeploySnapshot) -> Result<(), String> {
    mice::write_atomic(&cfg_dir.join("config.ini"), &render(dep))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Loader;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TMP_SEQ: AtomicU32 = AtomicU32::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> Self {
            let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
            let dir = std::env::temp_dir().join(format!(
                "kitten_test_{}_{tag}_{seq}",
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

    fn snapshot() -> DeploySnapshot {
        DeploySnapshot {
            net: NetworkConfig::default(),
            system: SystemConfig::default(),
            coin: CoinConfig::default(),
            graphics: GraphicsConfig::default(),
            input: InputConfig::default(),
            game_id: None,
        }
    }

    fn record_at(game_dir: &Path) -> GameRecord {
        GameRecord {
            id: "test".into(),
            path: game_dir.join("id5.elf").to_string_lossy().into_owned(),
            name: "Test game".into(),
            crc: Some("0x12345678".into()),
            version: Some("REV A".into()),
            arch: Some("x86".into()),
            loader: Loader::Kitten,
            platform: None,
            image: None,
            cover: None,
            serverbox: None,
            reader: None,
            amdaemon: None,
            game_exe: None,
            gfx_patch: None,
            game_id: None,
            test_mode: None,
            card: None,
            region: None,
            play_count: 0,
            last_played: None,
            added_at: 0,
        }
    }

    #[test]
    fn render_minimal_redirect_and_simulate() {
        let text = render(&snapshot());

        assert_eq!(
            text,
            "[network]\nredirect = sdgt.arcadehub.xyz\nsimulateNetwork = true\n\n[io3]\nrestrict_ = 128\n\n[ffb]\nenable = true\nconstantForceStrength = 100\ndamperStrength = 100\nrumbleStrength = 100\nrumbleDuration = 100\nbaseDamperStrength = 20\ndeadBand = 20\n"
        );
    }

    #[test]
    fn render_simulate_defaults_true() {

        let mut dep = snapshot();
        dep.net.simulate = None;
        assert!(render(&dep).contains("simulateNetwork = true"));

        dep.net.simulate = Some(true);
        assert!(render(&dep).contains("simulateNetwork = true"));

        dep.net.simulate = Some(false);
        assert!(render(&dep).contains("simulateNetwork = false"));
    }

    #[test]
    fn render_off_loglevel_treated_as_unset() {
        let mut dep = snapshot();
        dep.system.loglevel = Some("off".into());
        let text = render(&dep);
        assert!(!text.contains("log_level"), "the legacy value off should not write log_level");
        assert!(text.contains("[network]"));
    }

    #[test]
    fn render_full_mapping_and_chn_dipsw() {
        let mut dep = snapshot();
        dep.net.ip = Some("192.168.100.0/24".into());
        dep.net.simulate = Some(false);
        dep.system.loglevel = Some("debug".into());
        dep.system.region = Some("CHN".into());
        dep.system.keychip = Some("A69E-01A99999999".into());
        dep.system.skip_reader_init = Some(true);
        dep.game_id = Some("SBTS".into());
        dep.graphics.default = Some(false);
        dep.graphics.borderless = Some(false);
        dep.graphics.resolution = Some("1920x1080".into());
        dep.graphics.render_scale = Some(true);
        dep.graphics.render_scale_level = Some("2x".into());
        dep.graphics.msaa = Some(true);
        dep.graphics.msaa_level = Some("4x".into());
        dep.graphics.af = Some(true);
        dep.graphics.af_level = Some("16x".into());
        dep.coin.freeplay = Some(false);
        dep.coin.start_coin = Some(2);
        dep.coin.continue_coin = Some(2);
        dep.coin.card_coin = Some(3);
        dep.coin.car_coin = Some(1);
        dep.input.kind = Some("dinput".into());
        dep.input.ratio = Some(128);

        let text = render(&dep);

        assert!(text.contains("[display]\ndipsw = false\nwidth = 1920\nheight = 1080\n"));
        assert!(text.contains("borderless = false"));
        assert!(text.contains("render_scale = 2"));
        assert!(text.contains("msaa_samples = 4"));
        assert!(text.contains("anisotropic = 16"));
        assert!(text.contains("[network]\nredirect = sdgt.arcadehub.xyz\n"));
        assert!(text.contains("subnet = 192.168.100.0/24"));
        assert!(text.contains("simulateNetwork = false"));
        assert!(text.contains("log_level = 1"));
        assert!(text.contains("region = 2"));
        assert!(text.contains("dipsw = 0x3A"), "CHN should also write dipsw=0x3A");
        assert!(text.contains("game_id = SBTS"));
        assert!(text.contains("keychip_serial = A69E-01A99999999"));
        assert!(text.contains("skipInitCard = true"));
        assert!(text.contains("[Coins]"));
        assert!(text.contains("freeplay = false"));
        assert!(text.contains("start = 2"));
        assert!(text.contains("continues = 2"));
        assert!(text.contains("card = 3"));
        assert!(text.contains("car = 1"));
        assert!(text.contains("mode = 2"));
        assert!(text.contains("restrict_ = 128"));

        assert!(text.contains("[dinput]"), "dinput mode should write the [dinput] section");
        assert!(text.contains("reverseAccelAxis = false"));
        assert!(text.contains("reverseBrakeAxis = false"));
        assert!(!text.contains(';'), "the generated file must not contain comments");
    }

    #[test]
    fn render_dinput_reverse_axes_true() {
        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        dep.input.wheel_device = Some("Thrustmaster Wheel\u{90}".into());
        dep.input.throttle_axis = Some("Y".into());
        dep.input.brake_axis = Some("Rz".into());
        dep.input.reverse_accel_axis = Some(true);
        dep.input.reverse_brake_axis = Some(true);

        let text = render(&dep);

        assert!(text.contains("[dinput]"));
        assert!(text.contains("deviceName = Thrustmaster Wheel\n"));
        assert!(!text.contains('\u{90}'));
        assert!(text.contains("accelAxis = Y"));
        assert!(text.contains("brakeAxis = RZ"));
        assert!(text.contains("reverseAccelAxis = true"));
        assert!(text.contains("reverseBrakeAxis = true"));
    }

    #[test]
    fn render_ffb_section() {
        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());

        let text = render(&dep);
        assert!(text.contains("[ffb]"), "the [ffb] force feedback section should be written");
        assert!(text.contains("enable = true"));
        assert!(text.contains("constantForceStrength = 100"));
        assert!(text.contains("damperStrength = 100"));
        assert!(text.contains("rumbleStrength = 100"));
        assert!(text.contains("rumbleDuration = 100"));
        assert!(text.contains("baseDamperStrength = 20"));
        assert!(text.contains("deadBand = 20"));

        dep.input.ffb_enable = Some(false);
        dep.input.ffb_constant_force = Some(50);
        dep.input.ffb_damper = Some(40);
        dep.input.ffb_rumble = Some(30);
        dep.input.ffb_rumble_duration = Some(60);
        dep.input.ffb_base_damper = Some(10);
        dep.input.ffb_dead_band = Some(5);
        let text = render(&dep);
        assert!(text.contains("enable = false"));
        assert!(text.contains("constantForceStrength = 50"));
        assert!(text.contains("damperStrength = 40"));
        assert!(text.contains("rumbleStrength = 30"));
        assert!(text.contains("rumbleDuration = 60"));
        assert!(text.contains("baseDamperStrength = 10"));
        assert!(text.contains("deadBand = 5"));
    }

    #[test]
    fn render_no_chn_no_dipsw() {
        let mut dep = snapshot();
        dep.system.region = Some("JPN".into());
        dep.system.loglevel = Some("info".into());
        let text = render(&dep);
        assert!(text.contains("region = 0"));
        assert!(text.contains("log_level = 3"));
        assert!(!text.contains("dipsw"), "only CHN writes dipsw");
    }

    #[test]
    fn render_display_default_true_omits_section() {
        let mut dep = snapshot();
        dep.graphics.default = Some(true);
        dep.graphics.resolution = Some("1920x1080".into());
        dep.graphics.render_scale = Some(true);
        dep.graphics.render_scale_level = Some("2x".into());
        let text = render(&dep);
        assert!(!text.contains("[display]"));
        assert!(!text.contains("width"));
    }

    #[test]
    fn render_xinput_mode_and_unset_ratio() {
        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        dep.input.ratio = None;
        let text = render(&dep);
        assert!(text.contains("mode = 1"));

        assert!(text.contains("restrict_ = 128"), "without a ratio the default 128 should be written");
    }

    #[test]
    fn render_xinput_button_mappings() {

        let mut dep = snapshot();
        dep.input.kind = Some("xinput".into());
        let text = render(&dep);
        assert!(!text.contains("start ="));
        assert!(!text.contains("viewChg ="));
        assert!(!text.contains("coin ="));
        assert!(!text.contains("accel ="));
        assert!(!text.contains("brake ="));

        let mut dep2 = snapshot();
        dep2.input.kind = Some("xinput".into());
        dep2.input.xi_start = Some("A".into());
        dep2.input.xi_coin = Some("Y".into());
        dep2.input.xi_up = Some(" ".into());
        dep2.input.xi_card = Some("START".into());
        dep2.input.xi_accel = Some("RT".into());
        dep2.input.xi_brake = Some("LT".into());
        let text2 = render(&dep2);
        assert!(text2.contains("start = A"));
        assert!(text2.contains("coin = Y"));
        assert!(text2.contains("accel = RT"), "the throttle should be the captured button name");
        assert!(text2.contains("brake = LT"), "the brake should be the captured button name");
        assert!(!text2.contains("viewChg ="), "an unset key is not written");
        assert!(!text2.contains("up ="), "a blank value is not written");
        assert!(!text2.contains("card ="), "kitten [xinput] has no card key");
        assert!(!text2.contains("shiftDn ="));
        assert!(!text2.contains("down ="));
    }

    #[test]
    fn render_dinput_direction_buttons_skip_dpad_pseudo_codes() {
        let mut dep = snapshot();
        dep.input.kind = Some("dinput".into());
        dep.input.up_button = Some(13);
        dep.input.down_button = Some(14);
        dep.input.left_button = Some(15);
        dep.input.right_button = Some(16);
        dep.input.coin_button = Some(10);
        let text = render(&dep);
        assert!(text.contains("up = 13"));
        assert!(text.contains("down = 14"));
        assert!(text.contains("left = 15"));
        assert!(text.contains("right = 16"));
        assert!(text.contains("coin = 10"));

        let mut dep2 = snapshot();
        dep2.input.kind = Some("dinput".into());
        dep2.input.up_button = Some(DPAD_CODE_BASE + 1);
        dep2.input.left_button = Some(DPAD_CODE_BASE + 4);
        let text2 = render(&dep2);
        assert!(!text2.contains("up = "), "D-pad pseudo codes should not be written to dinput");
        assert!(!text2.contains("left = "), "D-pad pseudo codes should not be written to dinput");
    }

    #[test]
    fn render_region_exp_equal_two() {
        let mut dep = snapshot();
        dep.system.region = Some("EXP".into());
        let text = render(&dep);
        assert!(text.contains("region = 2"));
    }

    #[test]
    fn deploy_copies_tools_and_writes_config() {
        let tmp = TempDir::new("deploy");
        let tool = tmp.0.join("tool");
        let game = tmp.0.join("game");
        fs::create_dir_all(&tool).unwrap();
        fs::create_dir_all(&game).unwrap();

        fs::write(tool.join("kitten.exe"), "kitten-bin").unwrap();
        fs::write(tool.join("mice.dll"), "dll-bin").unwrap();
        fs::write(tool.join("config.ini"), "[sample]\nkey = value\n").unwrap();
        fs::write(tool.join("kitten.log"), "log").unwrap();
        fs::write(tool.join("sram.bin"), "sram").unwrap();

        fs::write(game.join("id5.elf"), "elf").unwrap();

        let mut dep = snapshot();
        dep.system.region = Some("CHN".into());
        dep.game_id = Some("SBTS".into());

        let exe = deploy(&record_at(&game), &tool, &dep).unwrap();
        assert_eq!(exe, game.join("kitten.exe"));
        assert!(exe.is_file());

        assert_eq!(fs::read(game.join("kitten.exe")).unwrap(), b"kitten-bin");
        assert_eq!(fs::read(game.join("mice.dll")).unwrap(), b"dll-bin");
        assert!(!game.join("config.ini").exists(), "the tool's own config.ini should not be copied");
        assert!(!game.join("kitten.log").exists(), "*.log should not be copied");
        assert!(!game.join("sram.bin").exists(), "sram.bin should not be copied");

        let cfg = fs::read_to_string(game.join("kitten/config.ini")).unwrap();
        assert_eq!(cfg, render(&dep));
        assert!(cfg.contains("region = 2"));
        assert!(cfg.contains("dipsw = 0x3A"));

        let mice_cfg = fs::read_to_string(game.join("mice/config.ini")).unwrap();
        assert!(mice_cfg.contains("naominet_jp = sdgt.arcadehub.xyz"));
        assert!(mice_cfg.contains("apply_patches = false"));

        deploy(&record_at(&game), &tool, &dep).unwrap();
        assert_eq!(
            fs::read(game.join("kitten.exe")).unwrap(),
            b"kitten-bin"
        );
    }

    #[test]
    fn deploy_missing_game_file_errors() {
        let tmp = TempDir::new("missing");
        let tool = tmp.0.join("tool");
        let game = tmp.0.join("game");
        fs::create_dir_all(&tool).unwrap();
        fs::create_dir_all(&game).unwrap();
        fs::write(tool.join("kitten.exe"), "x").unwrap();

        let rec = record_at(&game);
        let err = deploy(&rec, &tool, &snapshot()).unwrap_err();
        let expected = t!("common.game_file_does_not_exist", game = Path::new(&rec.path).display());
        assert_eq!(err, expected);
    }

    #[test]
    fn render_redirect_uses_server_setting() {
        let mut snapshot = snapshot();
        snapshot.net.server = Some(" my.server.net ".into());
        let text = render(&snapshot);
        assert!(text.contains("redirect = my.server.net"));
        assert!(!text.contains("sdgt.arcadehub.xyz"), "with a custom server the default domain is not written");
    }

    #[test]
    fn render_serverbox_bind_ip() {

        let text = render(&snapshot());
        assert!(!text.contains("bind_ip"));
        assert!(!text.contains("[serverbox]"));

        let mut dep = snapshot();
        dep.net.serverbox_ip = Some("192.168.100.101".into());
        assert!(!render(&dep).contains("bind_ip = 192.168.100.101"));
        dep.net.simulate = Some(true);
        assert!(!render(&dep).contains("bind_ip = 192.168.100.101"));

        let mut dep = snapshot();
        dep.net.simulate = Some(false);
        dep.net.serverbox_ip = Some("  192.168.100.101  ".into());
        let text = render(&dep);
        assert!(
            text.contains("bind_ip = 192.168.100.101"),
            "[network] bind_ip should be written:\n{text}"
        );
        assert!(
            text.contains("[network]")
                && text.find("bind_ip = 192.168.100.101").unwrap() > text.find("[network]").unwrap(),
            "bind_ip must be inside the [network] section:\n{text}"
        );
        assert!(!text.contains("[serverbox]"), "there should be no separate [serverbox] section");
    }

    #[test]
    fn render_host_game_ip() {

        let text = render(&snapshot());
        assert!(!text.contains("game_ip"));

        let mut dep = snapshot();
        dep.net.host_ip = Some("  192.168.100.50  ".into());
        assert!(
            !render(&dep).contains("game_ip"),
            "game_ip should not be written while the network is simulated"
        );
        dep.net.simulate = Some(true);
        assert!(
            !render(&dep).contains("game_ip"),
            "game_ip should not be written while the network is simulated"
        );

        dep.net.simulate = Some(false);
        let text = render(&dep);
        assert!(
            text.contains("game_ip = 192.168.100.50"),
            "[network] game_ip should be written:\n{text}"
        );
        assert!(
            text.find("game_ip = 192.168.100.50").unwrap() > text.find("[network]").unwrap(),
            "game_ip must be inside the [network] section:\n{text}"
        );
    }
}
