//! Initial D The Arcade (ALLS), run by segatools with `idachook.dll`.
//!
//! Launching one: write `segatools.ini` next to amdaemon.exe (with the hook's
//! `config_hook.json` and the season's spike file), then start amdaemon and
//! `GameProject-Win64-Shipping.exe`. It has no ServerBox.

use std::process::Child;
use crate::storage::GameRecord;

use rust_i18n::t;

use super::{check_initial_d_settings, CardKind, Series, SettingsKind, UiFeatures, GameFamily, LaunchInput, LaunchProblem, Prepared, Started, alls};

pub struct Idac;

impl GameFamily for Idac {
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
            Some("SDGT") => CardKind::Aime,
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
        alls::start(input, prepared, None)
    }

    fn launch_serverbox(&self, _input: &LaunchInput) -> Result<Child, String> {
        Err(t!("common.segatools_has_no_serverbox").to_string())
    }
}
