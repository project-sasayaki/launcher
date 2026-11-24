//! Initial D Arcade Stage Zero (Nu), run by segatools with `idzhook.dll`.
//!
//! Launching one: write `segatools.ini` next to amdaemon.exe, then start amdaemon and the
//! game. The launcher starts no ServerBox for it, but when the game exits it also ends
//! `ServerBoxD8_Nu_x64.exe` (or the `_CHN` one), which can be left running.

use std::process::Child;
use crate::storage::GameRecord;

use rust_i18n::t;

use super::{check_initial_d_settings, CardKind, Series, SettingsKind, UiFeatures, GameFamily, LaunchInput, LaunchProblem, Prepared, Started, alls};
use crate::loaders::segatools;

pub struct Idz;

impl GameFamily for Idz {
    fn ui_features(&self) -> UiFeatures {
        UiFeatures {
            test_mode: false,
            graphics_patch: false,
            region: true,
            cards: true,
            settings: SettingsKind::InitialD,
            series: Series::InitialD,
            companion_executables: true,
            card_extension: "txt",
        }
    }

    fn card_kind(&self, record: &GameRecord) -> CardKind {
        match record.game_id.as_deref() {
            Some("SDDF" | "SDER") => CardKind::Aime,
            _ => CardKind::Id5,
        }
    }

    fn has_serverbox(&self) -> bool {
        false
    }

    fn check(&self, input: &LaunchInput) -> Result<(), LaunchProblem> {
        check_initial_d_settings(input, alls::network_configured(input))?;
        alls::check(input)
    }

    fn prepare(&self, input: &LaunchInput) -> Result<Prepared, LaunchProblem> {
        alls::prepare(input)
    }

    fn start(&self, input: &LaunchInput, prepared: Prepared) -> Result<Started, LaunchProblem> {
        let own_serverbox = segatools::idz_serverbox_image(input.record.game_id.as_deref());
        alls::start(input, prepared, Some(own_serverbox))
    }

    fn launch_serverbox(&self, _input: &LaunchInput) -> Result<Child, String> {
        Err(t!("common.segatools_has_no_serverbox").to_string())
    }
}
