//! The games the launcher runs, grouped into families that are launched the same way.
//!
//! | Family | Games | Module |
//! | --- | --- | --- |
//! | [`IdasLindbergh`] | Initial D Arcade Stage 4 / 5, run by kitten | [`idas_lindbergh`] |
//! | [`IdasRingEdge`] | Initial D Arcade Stage 6 / 7 / 8, run by mice | [`idas_ringedge`] |
//! | [`Idac`] | Initial D The Arcade, run by segatools | [`idac`] |
//! | [`Idz`] | Initial D Arcade Stage Zero, run by segatools | [`idz`] |
//! | [`Wmmt3`] | Wangan Midnight Maximum Tune 3 / 3DX / 3DX+, run by kitten | [`wmmt3`] |
//!
//! Each family implements [`GameFamily`]: what to check before a launch, what to write into
//! the game folder, and what to start. [`launch_game`] runs those steps in order. The other
//! modules hold what several families use: the card reader, the graphics patch, the
//! ServerBox and the `initialD.ini` rewrite.

use std::path::{Path, PathBuf};
use std::process::Child;

use rust_i18n::t;

use crate::loaders::LoaderPaths;
use crate::loaders::mice::RoomNetOverride;
use crate::loaders::segatools::{self, GameKind};
use crate::storage::{
    CoinConfig, GameRecord, GraphicsConfig, InputConfig, Loader, NetworkConfig, Settings, SystemConfig,
};

pub mod card_reader;
pub mod gfx_patch;
pub mod initiald;
pub mod serverbox;

mod alls;
mod idac;
mod idas;
mod idas_lindbergh;
mod idas_ringedge;
mod idz;
pub mod wmmt3;

pub use idac::Idac;
pub use idas_lindbergh::IdasLindbergh;
pub use idas_ringedge::IdasRingEdge;
pub use idz::Idz;
pub use wmmt3::{Wmmt3, Wmmt3Settings};

use card_reader::CardReader;
use serverbox::StaticIp;

/// Everything a launch needs, taken from the launcher's settings at the moment it starts.
#[derive(Clone)]
pub struct LaunchInput {
    pub record: GameRecord,
    pub settings: Settings,
    pub loaders: LoaderPaths,
    /// The network settings with the addresses in effect: those of the room while this PC
    /// is in one, otherwise the Network page's (or the chosen adapter's).
    pub net: NetworkConfig,
    /// The subnet saved on the Network page, as it was typed.
    pub saved_subnet: Option<String>,
    /// The game's own system settings, with the launcher's keychip and the game row's region.
    pub system: SystemConfig,
    /// The game's own coin, graphics and input settings.
    pub coin: CoinConfig,
    pub graphics: GraphicsConfig,
    pub input: InputConfig,
    /// The card chosen for the game (an INID PNG file name, or an Aime access code file).
    pub card: Option<String>,
    /// The room's network while this PC is in a room.
    pub room: Option<RoomNetOverride>,
    /// This PC's place in the room while it is in one.
    pub room_seat: Option<RoomSeat>,
    /// The adapter that gets the ServerBox address when the game needs one.
    pub serverbox_adapter: Option<String>,
    /// The game's own WMMT3 settings, for the WMMT3 games.
    pub wmmt3: Wmmt3Settings,
}

/// This PC's place in a room.
#[derive(Clone, Copy, Debug)]
pub struct RoomSeat {
    pub is_host: bool,
    /// The seat (A1, A2, B1, B2) its address in the room stands for, when it stands for one.
    pub seat: Option<&'static str>,
}

/// What a launch could not do, and where the user can put it right.
#[derive(Debug)]
pub struct LaunchProblem {
    pub message: String,
    pub fix: Option<Fix>,
}

/// The place in the launcher where a launch problem is fixed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fix {
    /// The game's row on the Games page.
    GameRow,
    /// The game's card selection.
    Cards,
    /// The keychip, on the Settings page.
    Keychip,
    Network,
    /// The Input tab of the game's settings page.
    Input,
}

impl LaunchProblem {
    /// A setting is missing or wrong; `fix` is where to correct it.
    pub fn needs(message: impl Into<String>, fix: Fix) -> Self {
        LaunchProblem { message: message.into(), fix: Some(fix) }
    }

    /// Something failed while launching.
    pub fn failed(message: impl Into<String>) -> Self {
        LaunchProblem { message: message.into(), fix: None }
    }
}

/// The game folder written to and started, ready to run.
pub struct Prepared {
    /// The program that starts the game: the loader, or the copy of it in the game folder.
    pub loader: PathBuf,
    /// The card reader to start before the game, for games that have one.
    pub card_reader: Option<CardReader>,
}

/// A game that has started.
pub struct Started {
    pub game: Child,
    /// Programs started with the game (card reader, amdaemon), ended when it exits.
    pub helpers: Vec<Child>,
    /// Programs to end by name when the game exits (amdaemon, the ServerBox), because they
    /// can outlive the processes started here.
    pub kill_on_exit: Vec<String>,
}

/// A running game and what to undo when it exits.
pub struct RunningGame {
    pub started: Started,
    /// The ServerBox address added to the adapter for this session.
    pub static_ip: Option<StaticIp>,
}

/// Controls shown for a game in the library.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UiFeatures {
    pub test_mode: bool,
    pub graphics_patch: bool,
    pub companion_executables: bool,
    /// The region the game runs as is chosen on its row.
    pub region: bool,
    /// The game's card is chosen on the card page.
    pub cards: bool,
    /// The settings the game's settings page offers.
    pub settings: SettingsKind,
    /// The series the Games page lists the game under.
    pub series: Series,
    /// The extension shown for the selected card in the game row.
    pub card_extension: &'static str,
}

/// A series of games, which the Games page lists apart, in this order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Series {
    InitialD,
    WanganMidnight,
}

/// The settings a game's settings page offers (on its Graphics, System and Input tabs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsKind {
    /// Those of the Initial D games, which their loaders share.
    InitialD,
    /// KittenTools' `[wmmt3]` settings ([`wmmt3::settings`]).
    Wmmt3,
}

/// Card format and artwork used by the card page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CardKind {
    Id4,
    #[default]
    Id5,
    Id6,
    Id8,
    Aime,
    Wmmt3,
}

/// What the launcher needs to know about one family of games.
pub trait GameFamily: Sync {
    fn ui_features(&self) -> UiFeatures;

    fn card_kind(&self, record: &GameRecord) -> CardKind;

    /// Whether these games have a ServerBox.
    fn has_serverbox(&self) -> bool;

    /// Checks that the settings these games need are set.
    fn check(&self, input: &LaunchInput) -> Result<(), LaunchProblem>;

    /// Writes the configs and copies the tools into the game folder.
    fn prepare(&self, input: &LaunchInput) -> Result<Prepared, LaunchProblem>;

    /// Starts the helpers and the game. When starting fails, ends what was started.
    fn start(&self, input: &LaunchInput, prepared: Prepared) -> Result<Started, LaunchProblem>;

    /// Starts the game's ServerBox.
    fn launch_serverbox(&self, input: &LaunchInput) -> Result<Child, String>;
}

/// The family a library record belongs to.
pub fn family_of(record: &GameRecord) -> &'static dyn GameFamily {
    match record.loader {
        Loader::Kitten if wmmt3::is_wmmt3_game(record) => &Wmmt3,
        Loader::Kitten => &IdasLindbergh,
        Loader::Mice => &IdasRingEdge,
        Loader::Segatools => match segatools::game_kind(record.game_id.as_deref()) {
            GameKind::Idz => &Idz,
            GameKind::Idac => &Idac,
        },
    }
}

/// Checks the settings, prepares the game folder and starts the game.
pub fn launch_game(input: &LaunchInput) -> Result<RunningGame, LaunchProblem> {
    let family = family_of(&input.record);
    family.check(input)?;

    let prepared = family.prepare(input)?;

    let static_ip = if family.has_serverbox() {
        serverbox::add_static_ip_if_needed(input).map_err(|e| {
            LaunchProblem::failed(t!("page_launch.failed_to_add_serverbox_static", error = e))
        })?
    } else {
        None
    };

    match family.start(input, prepared) {
        Ok(started) => Ok(RunningGame { started, static_ip }),
        Err(problem) => {
            if let Some(ip) = &static_ip {
                let _ = ip.remove();
            }
            Err(problem)
        }
    }
}

/// Checks the settings and starts the game's ServerBox.
pub fn launch_serverbox(input: &LaunchInput) -> Result<Child, LaunchProblem> {
    if input.net.ip.is_none() {
        return Err(LaunchProblem::needs(t!("page_launch.network_subnet_is_not_configured"), Fix::Network));
    }
    if is_blank(&input.system.keychip) {
        return Err(LaunchProblem::needs(t!("page_launch.keychip_serial_is_not_set"), Fix::Keychip));
    }
    family_of(&input.record)
        .launch_serverbox(input)
        .map_err(|e| LaunchProblem::failed(t!("page_launch.serverbox_launch_failed", error = e)))
}

/// The settings every Initial D game needs: the region chosen on its row, the keychip, and
/// a network (`network_configured`, which the families judge in their own way).
fn check_initial_d_settings(input: &LaunchInput, network_configured: bool) -> Result<(), LaunchProblem> {
    if is_blank(&input.record.region) {
        return Err(LaunchProblem::needs(t!("page_launch.region_is_not_set"), Fix::GameRow));
    }
    if is_blank(&input.system.keychip) {
        return Err(LaunchProblem::needs(t!("page_launch.keychip_serial_is_not_set"), Fix::Keychip));
    }
    if !network_configured {
        return Err(LaunchProblem::needs(t!("page_launch.network_subnet_is_not_configured"), Fix::Network));
    }
    Ok(())
}

/// The folder of the game's file (the ELF or exe the loader runs).
pub fn game_dir_of(record: &GameRecord) -> PathBuf {
    Path::new(&record.path).parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."))
}

/// Whether an optional setting is unset or blank.
fn is_blank(value: &Option<String>) -> bool {
    value.as_deref().map(str::trim).is_none_or(str::is_empty)
}

/// Launch inputs for the tests of the families.
#[cfg(test)]
pub(crate) mod test_input {
    use super::*;

    pub fn record(loader: Loader, game_id: Option<&str>) -> GameRecord {
        GameRecord {
            id: "test".into(),
            path: String::new(),
            name: "Test game".into(),
            crc: None,
            version: Some("3.21.00".into()),
            arch: None,
            loader,
            platform: None,
            image: None,
            cover: None,
            serverbox: None,
            reader: None,
            amdaemon: None,
            game_exe: None,
            game_id: game_id.map(str::to_string),
            gfx_patch: None,
            test_mode: None,
            card: None,
            region: Some("JPN".into()),
            play_count: 0,
            last_played: None,
            added_at: 0,
        }
    }

    /// Every graphics option set, so a test can see whether it reaches a config.
    pub fn graphics() -> GraphicsConfig {
        GraphicsConfig {
            default: Some(false),
            borderless: Some(false),
            resolution: Some("1920x1080".into()),
            render_scale: Some(true),
            render_scale_level: Some("2x".into()),
            msaa: Some(true),
            msaa_level: Some("4x".into()),
            af: Some(true),
            af_level: Some("16x".into()),
        }
    }

    pub fn input(loader: Loader, game_id: Option<&str>) -> LaunchInput {
        LaunchInput {
            record: record(loader, game_id),
            settings: Settings::default(),
            loaders: LoaderPaths::default(),
            net: NetworkConfig { ip: Some("192.168.100.0/24".into()), ..Default::default() },
            saved_subnet: Some("192.168.100.0/24".into()),
            system: SystemConfig { keychip: Some("A72E-01A12345678".into()), ..Default::default() },
            coin: CoinConfig::default(),
            graphics: graphics(),
            input: InputConfig::default(),
            card: Some("card.png".into()),
            room: None,
            room_seat: None,
            serverbox_adapter: None,
            wmmt3: Wmmt3Settings::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_input::record;
    use super::*;

    #[test]
    fn ui_features_match_existing_game_rows() {
        let cases = [
            (Loader::Kitten, Some("SBQZ"), UiFeatures { test_mode: true, graphics_patch: false, region: true, cards: true, settings: SettingsKind::InitialD, series: Series::InitialD, companion_executables: false, card_extension: "png" }),
            (Loader::Mice, Some("SBZZ"), UiFeatures { test_mode: true, graphics_patch: true, region: true, cards: true, settings: SettingsKind::InitialD, series: Series::InitialD, companion_executables: false, card_extension: "png" }),
            (Loader::Segatools, Some("SDGT"), UiFeatures { test_mode: false, graphics_patch: false, region: true, cards: true, settings: SettingsKind::InitialD, series: Series::InitialD, companion_executables: true, card_extension: "txt" }),
            (Loader::Segatools, Some("SDDF"), UiFeatures { test_mode: false, graphics_patch: false, region: true, cards: true, settings: SettingsKind::InitialD, series: Series::InitialD, companion_executables: true, card_extension: "txt" }),
            (Loader::Segatools, None, UiFeatures { test_mode: false, graphics_patch: false, region: true, cards: true, settings: SettingsKind::InitialD, series: Series::InitialD, companion_executables: true, card_extension: "txt" }),
            (Loader::Kitten, Some("W3P"), UiFeatures { test_mode: false, graphics_patch: false, region: false, cards: true, settings: SettingsKind::Wmmt3, series: Series::WanganMidnight, companion_executables: false, card_extension: "bin" }),
        ];
        for (loader, id, expected) in cases {
            let rec = record(loader, id);
            assert_eq!(family_of(&rec).ui_features(), expected, "{id:?}");
        }
    }

    #[test]
    fn card_kind_preserves_known_and_unknown_game_ids() {
        let cases = [
            (Loader::Kitten, Some("SBML"), CardKind::Id4),
            (Loader::Kitten, Some("SBQZ"), CardKind::Id5),
            (Loader::Kitten, None, CardKind::Id5),
            (Loader::Mice, Some("SBUU"), CardKind::Id6),
            (Loader::Mice, Some("SBVW"), CardKind::Id6),
            (Loader::Mice, Some("SBWV"), CardKind::Id6),
            (Loader::Mice, Some("SBZZ"), CardKind::Id8),
            (Loader::Mice, Some("SDAL"), CardKind::Id8),
            (Loader::Mice, None, CardKind::Id5),
            (Loader::Segatools, Some("SDDF"), CardKind::Aime),
            (Loader::Segatools, Some("SDER"), CardKind::Aime),
            (Loader::Segatools, Some("SDGT"), CardKind::Aime),
            (Loader::Segatools, None, CardKind::Id5),
            (Loader::Segatools, Some("UNKNOWN"), CardKind::Id5),
            (Loader::Kitten, Some("WM3"), CardKind::Wmmt3),
            (Loader::Kitten, Some("W3X"), CardKind::Wmmt3),
            (Loader::Kitten, Some("W3P"), CardKind::Wmmt3),
        ];
        for (loader, id, expected) in cases {
            let rec = record(loader, id);
            assert_eq!(family_of(&rec).card_kind(&rec), expected, "{id:?}");
        }
    }

    #[test]
    fn families_by_loader_and_game() {
        assert!(family_of(&record(Loader::Kitten, Some("SBQZ"))).has_serverbox());
        assert!(family_of(&record(Loader::Mice, Some("SBZZ"))).has_serverbox());
        assert!(!family_of(&record(Loader::Segatools, Some("SDGT"))).has_serverbox());
        assert!(!family_of(&record(Loader::Segatools, Some("SDDF"))).has_serverbox());
        assert!(!family_of(&record(Loader::Kitten, Some("W3P"))).has_serverbox());
    }

    #[test]
    fn blank_settings() {
        assert!(is_blank(&None));
        assert!(is_blank(&Some("  ".into())));
        assert!(!is_blank(&Some("A72E-01A12345678".into())));
    }
}
