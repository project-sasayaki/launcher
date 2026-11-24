//! The settings of Wangan Midnight Maximum Tune 3 / 3DX / 3DX+ that the launcher offers.
//!
//! They are keys of the `[wmmt3]` section of KittenTools' `kitten\config.ini`, under the same
//! names and with the same defaults as KittenTools' `src/games/wmmt3/wmmt3Config.def`. When
//! KittenTools adds, renames or changes a setting, change [`INPUT_SECTIONS`],
//! [`GRAPHICS_SECTIONS`] or [`SYSTEM_SECTIONS`] to match.
//!
//! The launcher keeps the values each game was given in the `[wmmt3]` section of that game's
//! own settings file (`games\<id>.ini`), and writes them into the game's `kitten\config.ini`
//! when it starts. A setting never changed in the launcher is not written there, so the file
//! keeps its own value.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use crate::ini;

/// The section the settings live in, in the game's settings file and in kitten's `config.ini`.
pub const SECTION: &str = "wmmt3";

/// One setting of the `[wmmt3]` section.
pub struct Setting {
    /// The key in `[wmmt3]`.
    pub key: &'static str,
    /// Text key of the setting's name.
    pub label: &'static str,
    pub kind: Kind,
    /// KittenTools' default, as the file writes it.
    pub default: &'static str,
    /// When the setting has an effect, and so is shown; `None` for always.
    pub shown_if: Option<Condition>,
}

/// When a setting has an effect.
pub enum Condition {
    /// While the switch (a `Kind::Toggle` setting) with this key is on.
    On(&'static str),
    /// While the switch with this key is off.
    Off(&'static str),
    /// For the game with this database game id.
    Game(&'static str),
}

/// How a setting's value is chosen.
pub enum Kind {
    /// `true` / `false`.
    Toggle,
    /// A whole number from `min` to `max`.
    Number { min: i64, max: i64 },
    /// One of the listed values.
    Choice(&'static [(&'static str, Label)]),
    /// Free text.
    Text,
    /// A folder, typed or chosen in a folder dialog.
    Folder,
    /// A font file, typed or chosen in a file dialog. The family name the file gives its font
    /// is written into the setting with the key `name` (a `Kind::Text` setting), the name a
    /// program asks Windows for once it has loaded the file.
    FontFile { name: &'static str },
    /// A gamepad button by its XInput name, or `none`.
    XinputButton,
    /// A DirectInput button by its number from 1, or `0` for none.
    DinputButton,
    /// A DirectInput axis (X, Y, Z, RX, RY, RZ, or the sliders U and V), captured from the
    /// device; `reverse` turns it around, and is set on the same row.
    Axis { reverse: &'static Setting },
    /// A key by its Windows virtual-key code (`0x70` = F1), or `0x00` for none.
    Key,
    /// A DirectInput device, by any text that occurs in its name; empty for what `empty`
    /// (a text key) says KittenTools takes then.
    Device { empty: &'static str },
}

/// How a choice is shown.
pub enum Label {
    /// The value as it is (a number, a button name).
    Plain(&'static str),
    /// A text key.
    Text(&'static str),
}

/// A group of settings shown under one title.
pub struct Section {
    /// Text key of the title.
    pub title: &'static str,
    /// The input mode the section belongs to; `None` for every mode.
    pub input_mode: Option<&'static str>,
    /// A setting turned on and off by a switch in the section's heading; the section's
    /// settings are shown while it is on.
    pub switch: Option<&'static Setting>,
    pub settings: &'static [Setting],
}

/// `input_mode` for an XInput gamepad.
pub const XINPUT_MODE: &str = "1";
/// `input_mode` for a DirectInput steering wheel.
pub const DINPUT_MODE: &str = "2";

const fn setting(key: &'static str, label: &'static str, kind: Kind, default: &'static str) -> Setting {
    Setting { key, label, kind, default, shown_if: None }
}

impl Setting {
    /// The setting, shown only while `condition` holds.
    const fn when(self, condition: Condition) -> Setting {
        Setting { shown_if: Some(condition), ..self }
    }
}

/// Force feedback on or off; the switch in the force feedback section's heading.
const FFB_ENABLE: Setting = setting("ffb_enable", "page_input.force_feedback", Kind::Toggle, "true");

/// The game id of 3DX+, the one version with a second dongle.
const DX_PLUS: &str = "W3P";

/// Whether the accelerator axis is turned around; set on the accelerator axis's row.
const REVERSE_ACCEL_AXIS: Setting =
    setting("dinput_reverse_accel_axis", "page_input.reverse_axis", Kind::Toggle, "false");
/// Whether the brake axis is turned around; set on the brake axis's row.
const REVERSE_BRAKE_AXIS: Setting =
    setting("dinput_reverse_brake_axis", "page_input.reverse_axis", Kind::Toggle, "false");

/// The XInput button names KittenTools knows, `none` aside.
pub const XINPUT_BUTTONS: [&str; 16] = [
    "DPAD_UP", "DPAD_DOWN", "DPAD_LEFT", "DPAD_RIGHT", "START", "BACK", "LTHUMB", "RTHUMB",
    "LB", "RB", "LT", "RT", "A", "B", "X", "Y",
];

/// The keys offered for the cabinet's switches: none, or a function key.
pub const KEYS: [(&str, &str); 13] = [
    ("0x00", ""),
    ("0x70", "F1"), ("0x71", "F2"), ("0x72", "F3"), ("0x73", "F4"), ("0x74", "F5"), ("0x75", "F6"),
    ("0x76", "F7"), ("0x77", "F8"), ("0x78", "F9"), ("0x79", "F10"), ("0x7A", "F11"), ("0x7B", "F12"),
];

const INPUT_MODES: &[(&str, Label)] = &[
    (XINPUT_MODE, Label::Text("wmmt3.input_mode_xinput")),
    (DINPUT_MODE, Label::Text("wmmt3.input_mode_dinput")),
];
const PADS: &[(&str, Label)] =
    &[("0", Label::Plain("1")), ("1", Label::Plain("2")), ("2", Label::Plain("3")), ("3", Label::Plain("4"))];
const CURVES: &[(&str, Label)] = &[
    ("1", Label::Plain("1")), ("2", Label::Plain("2")), ("3", Label::Plain("3")),
    ("4", Label::Plain("4")), ("5", Label::Plain("5")),
];
const SCALES: &[(&str, Label)] = &[
    ("1", Label::Text("widgets.off")), ("2", Label::Plain("2x")), ("4", Label::Plain("4x")), ("8", Label::Plain("8x")),
];
const SAMPLES: &[(&str, Label)] = &[
    ("0", Label::Text("widgets.off")), ("2", Label::Plain("2x")), ("4", Label::Plain("4x")),
    ("8", Label::Plain("8x")), ("16", Label::Plain("16x")),
];

/// The settings on the Input tab of a WMMT3 game's settings page: the player's controls on
/// the cabinet (accelerator, brake, steering, view change, intrude and the shifter) and the
/// cabinet's other switches (coin, card, test, service), on the device of the input mode in
/// use and on the keyboard, and force feedback.
pub const INPUT_SECTIONS: &[Section] = &[
    Section {
        title: "wmmt3.section_controls",
        input_mode: None,
        switch: None,
        settings: &[
            setting("input_mode", "wmmt3.input_mode", Kind::Choice(INPUT_MODES), XINPUT_MODE),
            setting("steering_range", "wmmt3.steering_range", Kind::Number { min: 1, max: 128 }, "128"),
        ],
    },
    Section {
        title: "wmmt3.section_xinput",
        input_mode: Some(XINPUT_MODE),
        switch: None,
        settings: &[
            setting("xinput_device", "wmmt3.xinput_device", Kind::Choice(PADS), "0"),
            setting("xinput_accel", "wmmt3.accel", Kind::XinputButton, "RT"),
            setting("xinput_brake", "wmmt3.brake", Kind::XinputButton, "LT"),
            setting("xinput_view_change", "wmmt3.view_change", Kind::XinputButton, "B"),
            setting("xinput_intrude", "wmmt3.intrude", Kind::XinputButton, "X"),
            setting("xinput_shift_down", "wmmt3.shift_down", Kind::XinputButton, "LB"),
            setting("xinput_shift_up", "wmmt3.shift_up", Kind::XinputButton, "RB"),
            // KittenTools' xinput_lever_up .. _right are not offered: shifting one gear a
            // press does the job, and left as KittenTools sets them the D-pad moves the lever.
            setting("xinput_coin", "wmmt3.coin", Kind::XinputButton, "Y"),
            setting("xinput_single_stick_steering", "wmmt3.single_stick_steering", Kind::Toggle, "true"),
            setting("xinput_linear_steering", "page_input.linear_steering", Kind::Toggle, "true"),
            setting("xinput_steering_exponent", "wmmt3.steering_exponent", Kind::Choice(CURVES), "2")
                .when(Condition::Off("xinput_linear_steering")),
            setting("xinput_left_deadzone", "wmmt3.left_deadzone", Kind::Number { min: 0, max: 32767 }, "7849"),
            setting("xinput_right_deadzone", "wmmt3.right_deadzone", Kind::Number { min: 0, max: 32767 }, "8689"),
            setting("xinput_rumble", "wmmt3.rumble", Kind::Number { min: 0, max: 100 }, "100"),
            setting("xinput_rumble_ms", "wmmt3.rumble_ms", Kind::Number { min: 0, max: 1000 }, "80"),
        ],
    },
    Section {
        title: "wmmt3.section_dinput",
        input_mode: Some(DINPUT_MODE),
        switch: None,
        settings: &[
            setting("dinput_wheel", "page_input.steering_wheel", Kind::Device { empty: "wmmt3.device_first" }, ""),
            setting("dinput_pedals", "page_input.throttle_pedal", Kind::Device { empty: "wmmt3.device_with_wheel" }, ""),
            setting("dinput_shifter", "page_input.shifter", Kind::Device { empty: "wmmt3.device_with_wheel" }, ""),
            setting("dinput_accel_axis", "wmmt3.accel_axis", Kind::Axis { reverse: &REVERSE_ACCEL_AXIS }, "Y"),
            setting("dinput_brake_axis", "wmmt3.brake_axis", Kind::Axis { reverse: &REVERSE_BRAKE_AXIS }, "RZ"),
            setting("dinput_view_change", "wmmt3.view_change", Kind::DinputButton, "6"),
            setting("dinput_intrude", "wmmt3.intrude", Kind::DinputButton, "0"),
            setting("dinput_shift_down", "wmmt3.shift_down", Kind::DinputButton, "8"),
            setting("dinput_shift_up", "wmmt3.shift_up", Kind::DinputButton, "9"),
            setting("dinput_gear1", "page_input.button_gear_1", Kind::DinputButton, "0"),
            setting("dinput_gear2", "page_input.button_gear_2", Kind::DinputButton, "0"),
            setting("dinput_gear3", "page_input.button_gear_3", Kind::DinputButton, "0"),
            setting("dinput_gear4", "page_input.button_gear_4", Kind::DinputButton, "0"),
            setting("dinput_gear5", "page_input.button_gear_5", Kind::DinputButton, "0"),
            setting("dinput_gear6", "page_input.button_gear_6", Kind::DinputButton, "0"),
            // KittenTools' dinput_lever_up .. _right are not offered either: a wheel's shifter
            // is the H-pattern above or the two shift buttons, and its POV hat moves the lever.
            setting("dinput_coin", "wmmt3.coin", Kind::DinputButton, "0"),
            setting("dinput_card", "wmmt3.card", Kind::DinputButton, "0"),
            setting("dinput_test", "wmmt3.test", Kind::DinputButton, "0"),
            setting("dinput_service", "wmmt3.service", Kind::DinputButton, "0"),
        ],
    },
    Section {
        title: "page_input.force_feedback",
        input_mode: Some(DINPUT_MODE),
        switch: Some(&FFB_ENABLE),
        settings: &[
            setting("ffb_constant_force", "page_input.constant_force", Kind::Number { min: 0, max: 100 }, "100"),
            setting("ffb_damper", "page_input.damper", Kind::Number { min: 0, max: 100 }, "100"),
            setting("ffb_base_damper", "page_input.base_damper", Kind::Number { min: 0, max: 100 }, "20"),
            setting("ffb_dead_band", "wmmt3.ffb_dead_band", Kind::Number { min: 0, max: 1000 }, "20"),
            setting("ffb_invert", "wmmt3.ffb_invert", Kind::Toggle, "false"),
        ],
    },
    Section {
        title: "wmmt3.section_keyboard",
        input_mode: None,
        switch: None,
        settings: &[
            setting("intrude_key", "wmmt3.intrude", Kind::Key, "0x74"),
            setting("test_key", "wmmt3.test", Kind::Key, "0x70"),
            setting("service_key", "wmmt3.service", Kind::Key, "0x71"),
            setting("coin_key", "wmmt3.coin", Kind::Key, "0x72"),
            setting("card_key", "wmmt3.card", Kind::Key, "0x73"),
        ],
    },
];

/// The settings on the Graphics tab of a WMMT3 game's settings page.
pub const GRAPHICS_SECTIONS: &[Section] = &[Section {
    title: "wmmt3.section_screen",
    input_mode: None,
    switch: None,
    settings: &[
        setting("fullscreen", "wmmt3.fullscreen", Kind::Toggle, "false"),
        setting("borderless", "page_graphics.borderless", Kind::Toggle, "false")
            .when(Condition::Off("fullscreen")),
        setting("cabinet_resolution", "wmmt3.cabinet_resolution", Kind::Toggle, "true"),
        setting("window_width", "wmmt3.window_width", Kind::Number { min: 320, max: 7680 }, "1280")
            .when(Condition::Off("cabinet_resolution")),
        setting("window_height", "wmmt3.window_height", Kind::Number { min: 240, max: 4320 }, "960")
            .when(Condition::Off("cabinet_resolution")),
        setting("stretch", "wmmt3.stretch", Kind::Toggle, "false"),
        setting("render_scale", "wmmt3.render_scale", Kind::Choice(SCALES), "1"),
        setting("msaa_samples", "wmmt3.msaa_samples", Kind::Choice(SAMPLES), "0"),
        setting("anisotropic", "wmmt3.anisotropic", Kind::Choice(SAMPLES), "0"),
    ],
}];

/// The settings on the System tab of a WMMT3 game's settings page (with the log level of
/// `[sysconf]`, which is not a `[wmmt3]` key).
pub const SYSTEM_SECTIONS: &[Section] = &[
    Section {
        title: "wmmt3.section_card",
        input_mode: None,
        switch: None,
        settings: &[setting("card_auto_insert", "page_system.auto_card_insert", Kind::Toggle, "false")],
    },
    Section {
        title: "wmmt3.section_card_print",
        input_mode: None,
        switch: None,
        settings: &[
            // A folder of blank card images: each new card gets one at random and keeps it.
            // KittenTools also takes a single image here, which can be typed in.
            setting("print_template", "wmmt3.print_template", Kind::Folder, ""),
            setting("print_font_file", "wmmt3.print_font_file", Kind::FontFile { name: "print_font_name" }, ""),
            setting("print_font_name", "wmmt3.print_font_name", Kind::Text, "MS Gothic"),
        ],
    },
    Section {
        title: "wmmt3.section_coins",
        input_mode: None,
        switch: None,
        settings: &[setting("free_play", "wmmt3.free_play", Kind::Toggle, "false")],
    },
    Section {
        title: "wmmt3.section_linked_play",
        input_mode: None,
        switch: None,
        settings: &[
            setting("linked_play", "wmmt3.linked_play", Kind::Toggle, "false"),
            setting("link_subnet", "wmmt3.link_subnet", Kind::Text, "192.168.1.0/24")
                .when(Condition::On("linked_play")),
            setting("link_ip", "wmmt3.link_ip", Kind::Text, "").when(Condition::On("linked_play")),
        ],
    },
    Section {
        title: "wmmt3.section_sound",
        input_mode: None,
        switch: None,
        settings: &[setting("openal_dll", "wmmt3.openal_dll", Kind::Text, "OpenAL32.dll")],
    },
    Section {
        title: "wmmt3.section_load_times",
        input_mode: None,
        switch: None,
        settings: &[
            setting("disk_speed_emulation", "wmmt3.disk_speed_emulation", Kind::Toggle, "true"),
            setting("disk_mbps", "wmmt3.disk_mbps", Kind::Number { min: 1, max: 1000 }, "30")
                .when(Condition::On("disk_speed_emulation")),
            setting("disk_seek_ms", "wmmt3.disk_seek_ms", Kind::Number { min: 0, max: 1000 }, "13")
                .when(Condition::On("disk_speed_emulation")),
            setting("cabinet_ram_mb", "wmmt3.cabinet_ram_mb", Kind::Number { min: 0, max: 4096 }, "1024")
                .when(Condition::On("disk_speed_emulation")),
            setting("cabinet_used_mb", "wmmt3.cabinet_used_mb", Kind::Number { min: 0, max: 4096 }, "450")
                .when(Condition::On("disk_speed_emulation")),
        ],
    },
    Section {
        title: "wmmt3.section_dongle",
        input_mode: None,
        switch: None,
        settings: &[
            setting("hasp_serial", "wmmt3.hasp_serial", Kind::Text, "000000-000001"),
            setting("hasp2_serial", "wmmt3.hasp2_serial", Kind::Text, "000000-000002").when(Condition::Game(DX_PLUS)),
        ],
    },
];

/// Every setting the launcher offers, with those set on another setting's row.
pub fn all() -> impl Iterator<Item = &'static Setting> {
    INPUT_SECTIONS
        .iter()
        .chain(GRAPHICS_SECTIONS)
        .chain(SYSTEM_SECTIONS)
        .flat_map(|section| section.switch.into_iter().chain(section.settings.iter()))
        .flat_map(|setting| {
            let on_its_row = match &setting.kind {
                Kind::Axis { reverse } => Some(*reverse),
                _ => None,
            };
            std::iter::once(setting).chain(on_its_row)
        })
}

/// The setting with the key `key`.
pub fn find(key: &str) -> Option<&'static Setting> {
    all().find(|s| s.key == key)
}

/// The values set in the launcher, by key. A setting without one has KittenTools' default.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Wmmt3Settings {
    values: BTreeMap<&'static str, String>,
}

impl Wmmt3Settings {
    /// Reads the values from a game's settings file.
    pub fn load(config: &Path) -> Self {
        let section = ini::SectionReader::load(config, SECTION);
        let values = all()
            .filter_map(|s| section.get(s.key).map(|v| (s.key, v)))
            .collect();
        Wmmt3Settings { values }
    }

    /// The value of a setting: the one set in the launcher, or KittenTools' default.
    pub fn value(&self, setting: &Setting) -> &str {
        self.values.get(setting.key).map(String::as_str).unwrap_or(setting.default)
    }

    /// The input mode in use (`XINPUT_MODE` or `DINPUT_MODE`).
    pub fn input_mode(&self) -> &str {
        find("input_mode").map(|s| self.value(s)).unwrap_or(XINPUT_MODE)
    }

    /// Whether `setting` has an effect, with these values, for the game with the database
    /// game id `game_id`.
    pub fn applies(&self, setting: &Setting, game_id: Option<&str>) -> bool {
        let switch_on = |key: &str| find(key).is_some_and(|switch| self.is_on(switch));
        match &setting.shown_if {
            None => true,
            Some(Condition::On(key)) => switch_on(key),
            Some(Condition::Off(key)) => !switch_on(key),
            Some(Condition::Game(id)) => game_id == Some(*id),
        }
    }

    /// Whether a switch (a `Kind::Toggle` setting) is on.
    pub fn is_on(&self, switch: &Setting) -> bool {
        self.value(switch) == "true"
    }

    /// Sets a value and saves it into a game's settings file.
    pub fn set(&mut self, config: &Path, setting: &'static Setting, value: String) -> io::Result<()> {
        ini::set_value(config, SECTION, setting.key, &value)?;
        self.values.insert(setting.key, value);
        Ok(())
    }

    /// The values set in the launcher, as `(key, value)`.
    pub fn set_values(&self) -> impl Iterator<Item = (&'static str, &str)> {
        self.values.iter().map(|(k, v)| (*k, v.as_str()))
    }

    /// Settings with the given values set, for tests.
    #[cfg(test)]
    pub fn from_pairs(pairs: &[(&str, &str)]) -> Self {
        let values = pairs
            .iter()
            .map(|(k, v)| (find(k).expect("a known setting").key, v.to_string()))
            .collect();
        Wmmt3Settings { values }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_util::test_dir::{fresh, remove};
    use std::collections::HashSet;

    #[test]
    fn keys_are_unique() {
        let mut seen = HashSet::new();
        for s in all() {
            assert!(seen.insert(s.key), "{} is listed twice", s.key);
        }
    }

    #[test]
    fn defaults_are_valid_values() {
        for s in all() {
            let ok = match &s.kind {
                Kind::Toggle => s.default == "true" || s.default == "false",
                Kind::Number { min, max } => s.default.parse::<i64>().is_ok_and(|n| (*min..=*max).contains(&n)),
                Kind::Choice(items) => items.iter().any(|(v, _)| *v == s.default),
                Kind::XinputButton => XINPUT_BUTTONS.contains(&s.default) || s.default == "none",
                Kind::DinputButton => s.default.parse::<u32>().is_ok(),
                Kind::Axis { .. } => crate::rawinput::DINPUT_AXES.contains(&s.default),
                Kind::Key => KEYS.iter().any(|(v, _)| *v == s.default),
                Kind::Text | Kind::Folder | Kind::FontFile { .. } | Kind::Device { .. } => true,
            };
            assert!(ok, "{}: default {:?} does not fit its kind", s.key, s.default);
        }
    }

    #[test]
    fn conditions_name_a_switch() {
        for s in all() {
            if let Some(Condition::On(key) | Condition::Off(key)) = &s.shown_if {
                let switch = find(key).unwrap_or_else(|| panic!("{}: no setting {key}", s.key));
                assert!(matches!(switch.kind, Kind::Toggle), "{}: {key} is not a switch", s.key);
            }
        }
    }

    #[test]
    fn font_files_name_a_text_setting() {
        for s in all() {
            if let Kind::FontFile { name } = &s.kind {
                let target = find(name).unwrap_or_else(|| panic!("{}: no setting {name}", s.key));
                assert!(matches!(target.kind, Kind::Text), "{}: {name} is not a text setting", s.key);
            }
        }
    }

    #[test]
    fn a_setting_applies_only_while_its_condition_holds() {
        let width = find("window_width").unwrap();
        let second_dongle = find("hasp2_serial").unwrap();
        assert!(!Wmmt3Settings::default().applies(width, None), "the cabinet's resolution is on by default");
        assert!(Wmmt3Settings::from_pairs(&[("cabinet_resolution", "false")]).applies(width, None));
        assert!(Wmmt3Settings::default().applies(second_dongle, Some("W3P")));
        assert!(!Wmmt3Settings::default().applies(second_dongle, Some("W3X")));
    }

    #[test]
    fn values_default_until_set_and_survive_a_reload() {
        let dir = fresh("wmmt3_settings");
        let config = dir.join("config.ini");
        let mut settings = Wmmt3Settings::load(&config);
        let range = find("steering_range").unwrap();
        assert_eq!(settings.value(range), "128");
        assert_eq!(settings.set_values().count(), 0);

        settings.set(&config, range, "100".into()).unwrap();
        let reloaded = Wmmt3Settings::load(&config);
        assert_eq!(reloaded.value(range), "100");
        assert_eq!(reloaded.set_values().collect::<Vec<_>>(), vec![("steering_range", "100")]);
        remove(&dir);
    }
}
