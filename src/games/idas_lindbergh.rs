//! Initial D Arcade Stage 4 / 5 (Lindbergh), run by kitten.
//!
//! Launching one:
//! 1. put picodaemon and the Cg runtime into the game folder and write the reader's settings;
//! 2. copy kitten's files into the game folder and write `kitten\config.ini` (and
//!    `mice\config.ini`, which the reader reads);
//! 3. rewrite `initialD.ini` for the server;
//! 4. start the reader, then `kitten.exe -b <game file>`.

use std::path::PathBuf;
use std::process::Child;

use rust_i18n::t;

use super::{check_initial_d_settings, CardKind, Series, SettingsKind, UiFeatures, GameFamily, LaunchInput, LaunchProblem, Prepared, Started, idas, initiald, serverbox};
use crate::db;
use crate::loaders::{self, kitten};
use crate::storage::{GameRecord, GraphicsConfig};

pub struct IdasLindbergh;

impl GameFamily for IdasLindbergh {
    fn ui_features(&self) -> UiFeatures {
        UiFeatures {
            test_mode: true,
            graphics_patch: false,
            region: true,
            cards: true,
            settings: SettingsKind::InitialD,
            series: Series::InitialD,
            companion_executables: false,
            card_extension: "png",
        }
    }

    fn card_kind(&self, record: &GameRecord) -> CardKind {
        if record.game_id.as_deref() == Some("SBML") {
            CardKind::Id4
        } else {
            CardKind::Id5
        }
    }

    fn has_serverbox(&self) -> bool {
        true
    }

    fn check(&self, input: &LaunchInput) -> Result<(), LaunchProblem> {
        check_initial_d_settings(input, input.net.ip.is_some())?;
        idas::check(input)
    }

    fn prepare(&self, input: &LaunchInput) -> Result<Prepared, LaunchProblem> {
        let card_reader = idas::prepare_card_reader(input, true)?;
        let loader = deploy(&input.record, input, snapshot(input))
            .map_err(|e| LaunchProblem::failed(t!("page_launch.game_launch_failed", error = e)))?;
        let _ = initiald::patch_for(&input.record, input.net.server_host());
        Ok(Prepared { loader, card_reader: Some(card_reader) })
    }

    fn start(&self, input: &LaunchInput, prepared: Prepared) -> Result<Started, LaunchProblem> {
        idas::start(input, prepared)
    }

    fn launch_serverbox(&self, input: &LaunchInput) -> Result<Child, String> {
        let (sb_rec, _) = serverbox::prepare(&input.record)?;
        let loader = deploy(&sb_rec, input, serverbox_snapshot(input))?;
        loaders::start_game(&sb_rec, &loader).map_err(|e| format!("ServerBox {e}"))
    }
}

/// kitten's settings for the game.
fn snapshot(input: &LaunchInput) -> kitten::DeploySnapshot {
    kitten::DeploySnapshot {
        net: input.net.clone(),
        system: input.system.clone(),
        coin: input.coin.clone(),
        graphics: input.graphics.clone(),
        input: input.input.clone(),
        game_id: db::lookup_record(&input.record).and_then(|m| m.game_id.clone()),
    }
}

/// kitten's settings for the ServerBox: the graphics settings are for the game only.
fn serverbox_snapshot(input: &LaunchInput) -> kitten::DeploySnapshot {
    kitten::DeploySnapshot { graphics: GraphicsConfig::default(), ..snapshot(input) }
}

/// Copies kitten into the folder of `record`'s file and writes its configs; returns the
/// kitten.exe to start the file with.
fn deploy(record: &GameRecord, input: &LaunchInput, snapshot: kitten::DeploySnapshot) -> Result<PathBuf, String> {
    let kitten_exe = loaders::resolve_loader(record, &input.settings, &input.loaders)?;
    let tool_dir = kitten_exe.parent().ok_or_else(|| {
        t!("launch.cannot_determine_kitten_tool_directory", loader = kitten_exe.display()).to_string()
    })?;
    kitten::deploy(record, tool_dir, &snapshot)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::test_input;
    use crate::storage::Loader;

    #[test]
    fn game_gets_the_settings_in_effect() {
        let input = test_input::input(Loader::Kitten, Some("SBQZ"));
        let snap = snapshot(&input);
        assert_eq!(snap.net, input.net);
        assert_eq!(snap.system, input.system);
        assert_eq!(snap.graphics, input.graphics);
        assert!(kitten::render(&snap).contains("[display]"));
    }

    #[test]
    fn serverbox_gets_no_graphics() {
        let input = test_input::input(Loader::Kitten, Some("SBQZ"));
        let text = kitten::render(&serverbox_snapshot(&input));
        assert!(!text.contains("[display]"), "the ServerBox side should have no [display] section:\n{text}");
        assert!(!text.contains("width ="), "the resolution should not be written:\n{text}");
        assert!(!text.contains("anisotropic"), "anisotropic filtering should not be written:\n{text}");
        assert!(text.contains("[network]"), "the network section should stay:\n{text}");
    }
}
