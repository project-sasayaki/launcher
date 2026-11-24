//! Initial D Arcade Stage 6 / 7 / 8 (RingEdge), run by mice.
//!
//! Launching one:
//! 1. put picodaemon into the game folder and write the reader's settings;
//! 2. copy mice's files into the game folder and write `mice\config.ini` and `mice\idas.ini`;
//! 3. rewrite `initialD.ini` for the server (D6 / D7; D8 has none);
//! 4. put the graphics patch into the game and ServerBox folders (or take it out);
//! 5. start the reader, then `mice.exe -b <game exe>`.

use std::path::{Path, PathBuf};
use std::process::Child;

use rust_i18n::t;

use super::{check_initial_d_settings, CardKind, Series, SettingsKind, UiFeatures, GameFamily, LaunchInput, LaunchProblem, Prepared, Started, game_dir_of, gfx_patch, idas, initiald, serverbox};
use crate::loaders::{self, mice};
use crate::storage::{GameRecord, GraphicsConfig};

pub struct IdasRingEdge;

impl GameFamily for IdasRingEdge {
    fn ui_features(&self) -> UiFeatures {
        UiFeatures {
            test_mode: true,
            graphics_patch: true,
            region: true,
            cards: true,
            settings: SettingsKind::InitialD,
            series: Series::InitialD,
            companion_executables: false,
            card_extension: "png",
        }
    }

    fn card_kind(&self, record: &GameRecord) -> CardKind {
        match record.game_id.as_deref() {
            Some("SBUU" | "SBVW" | "SBWV") => CardKind::Id6,
            Some("SBZZ" | "SDAL") => CardKind::Id8,
            _ => CardKind::Id5,
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
        let card_reader = idas::prepare_card_reader(input, false)?;
        let loader = prepare_game(input)
            .map_err(|e| LaunchProblem::failed(t!("page_launch.game_launch_failed", error = e)))?;
        Ok(Prepared { loader, card_reader: Some(card_reader) })
    }

    fn start(&self, input: &LaunchInput, prepared: Prepared) -> Result<Started, LaunchProblem> {
        idas::start(input, prepared)
    }

    fn launch_serverbox(&self, input: &LaunchInput) -> Result<Child, String> {
        let (sb_rec, sb_dir) = serverbox::prepare(&input.record)?;
        deploy(&sb_rec, &sb_dir, input, &serverbox_snapshot(input), mice::MiceTarget::ServerBox)?;
        let mice_exe = loaders::resolve_loader(&sb_rec, &input.settings, &input.loaders)?;
        loaders::start_game(&sb_rec, &mice_exe).map_err(|e| format!("ServerBox {e}"))
    }
}

/// Writes mice's configs into the game folder, rewrites `initialD.ini` and puts the
/// graphics patch in place; returns the mice.exe to start the game with.
fn prepare_game(input: &LaunchInput) -> Result<PathBuf, String> {
    let game_dir = game_dir_of(&input.record);
    let mice_exe = deploy(&input.record, &game_dir, input, &snapshot(input), mice::MiceTarget::Game)?;
    let _ = initiald::patch_for(&input.record, input.net.server_host());

    let gfx_enabled = input.record.gfx_patch.unwrap_or(true);
    gfx_patch::deploy(&game_dir, gfx_enabled)?;
    if let Some(sb_dir) = serverbox::dir_of(&input.record) {
        let _ = gfx_patch::deploy(&sb_dir, gfx_enabled);
    }
    Ok(mice_exe)
}

/// mice's settings for the game.
fn snapshot(input: &LaunchInput) -> mice::DeploySnapshot {
    mice::DeploySnapshot {
        net: input.net.clone(),
        system: input.system.clone(),
        coin: input.coin.clone(),
        graphics: input.graphics.clone(),
        input: input.input.clone(),
        card: input.card.clone(),
        room: input.room.clone(),
    }
}

/// mice's settings for the ServerBox: no graphics settings (they are for the game only)
/// and no card.
fn serverbox_snapshot(input: &LaunchInput) -> mice::DeploySnapshot {
    mice::DeploySnapshot { graphics: GraphicsConfig::default(), card: None, ..snapshot(input) }
}

/// Copies mice into `dir` and writes its configs for `target`; returns the mice.exe that
/// runs `record`.
fn deploy(
    record: &GameRecord,
    dir: &Path,
    input: &LaunchInput,
    snapshot: &mice::DeploySnapshot,
    target: mice::MiceTarget,
) -> Result<PathBuf, String> {
    let mice_exe = loaders::resolve_loader(record, &input.settings, &input.loaders)?;
    let tool_dir = mice_exe.parent().ok_or_else(|| {
        t!("launch.cannot_determine_mice_tool_directory", loader = mice_exe.display()).to_string()
    })?;
    mice::deploy(dir, tool_dir, snapshot, target)?;
    Ok(mice_exe)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::test_input;
    use crate::storage::Loader;

    #[test]
    fn game_gets_the_settings_and_card() {
        let input = test_input::input(Loader::Mice, Some("SBZZ"));
        let snap = snapshot(&input);
        assert_eq!(snap.net, input.net);
        assert_eq!(snap.card, input.card);
        assert!(mice::render_config(&snap, mice::MiceTarget::Game).contains("[window]"));
        assert!(mice::render_idas(&snap).contains("render_scale"));
    }

    #[test]
    fn serverbox_gets_no_graphics_and_no_card() {
        let input = test_input::input(Loader::Mice, Some("SBZZ"));
        let snap = serverbox_snapshot(&input);
        assert_eq!(snap.card, None);
        let cfg = mice::render_config(&snap, mice::MiceTarget::ServerBox);
        assert!(!cfg.contains("[window]"), "the ServerBox config.ini should have no [window] section:\n{cfg}");
        assert!(!cfg.contains("windowed"), "the windowed key should not be written:\n{cfg}");
        let idas = mice::render_idas(&snap);
        assert!(!idas.contains("render_scale"), "the ServerBox idas.ini should not write render_scale:\n{idas}");
        assert!(!idas.contains("[Graphics]"), "the ServerBox idas.ini should have no [Graphics] section:\n{idas}");
        assert!(cfg.contains("[network]"), "the network section of config.ini should stay:\n{cfg}");
        assert!(idas.contains("simulateNetwork"), "the simulate network key of idas.ini should stay:\n{idas}");
    }
}
