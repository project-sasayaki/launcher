//! What Initial D Arcade Stage 4–8 have in common, whichever loader runs them: the
//! picodaemon card reader and a ServerBox.

use std::thread;
use std::time::Duration;

use rust_i18n::t;

use super::card_reader::{self, CardReader, ReaderSettings};
use super::{Fix, LaunchInput, LaunchProblem, Prepared, Started, game_dir_of, is_blank, serverbox};

/// How long the card reader gets to start before the game does.
const READER_BOOT_WAIT: Duration = Duration::from_secs(1);

/// The checks of these games: an H-pattern shifter needs XInput, and a card and a
/// ServerBox must be set.
pub fn check(input: &LaunchInput) -> Result<(), LaunchProblem> {
    let dinput = input.input.kind.as_deref() == Some("dinput");
    if dinput && input.input.shifter_type.as_deref() == Some("h") {
        return Err(LaunchProblem::needs(t!("page_launch.this_game_does_not_support"), Fix::Input));
    }
    if is_blank(&input.card) {
        return Err(LaunchProblem::needs(t!("page_launch.no_card_selected"), Fix::Cards));
    }
    if is_blank(&input.record.serverbox) {
        return Err(LaunchProblem::needs(t!("page_launch.serverbox_is_not_configured"), Fix::GameRow));
    }
    Ok(())
}

/// Puts picodaemon into the game folder and writes its settings. `lindbergh` adds what
/// only the kitten games need: the Cg runtime, and the input backend and card button the
/// reader reads for them.
pub fn prepare_card_reader(input: &LaunchInput, lindbergh: bool) -> Result<CardReader, LaunchProblem> {
    let game_dir = game_dir_of(&input.record);
    let picodaemon = card_reader::ensure_picodaemon(&game_dir)
        .map_err(|e| LaunchProblem::failed(t!("page_launch.card_reader_prep_failed", error = e)))?;
    if lindbergh {
        card_reader::ensure_cg_libs(&game_dir)
            .map_err(|e| LaunchProblem::failed(t!("page_launch.graphics_lib_prep_failed", error = e)))?;
    }

    let io3_mode = input.input.kind.as_deref().map(|v| v.trim().to_ascii_lowercase());
    let io3_mode = match io3_mode.as_deref() {
        Some("xinput") => Some("xinput"),
        Some("dinput") => Some("dinput"),
        _ => None,
    };
    let reader = ReaderSettings {
        card: input.card.as_deref(),
        game_ip: input.net.host_ip.as_deref(),
        auto_insert: input.system.auto_insert_card == Some(true),
        io3_mode: if lindbergh { io3_mode } else { None },
        card_button: if lindbergh { input.input.xi_card.as_deref() } else { None },
    };
    card_reader::deploy(&picodaemon, &input.settings, &input.loaders, &reader)
        .map_err(|e| LaunchProblem::failed(t!("page_launch.card_reader_deploy_failed", error = e)))
}

/// Starts the card reader, gives it a moment, then starts the game.
pub fn start(input: &LaunchInput, prepared: Prepared) -> Result<Started, LaunchProblem> {
    let mut helpers = Vec::new();
    if let Some(reader) = &prepared.card_reader {
        let child = reader
            .start()
            .map_err(|e| LaunchProblem::failed(t!("page_launch.failed_to_start_card_reader", error = e)))?;
        helpers.push(child);
        thread::sleep(READER_BOOT_WAIT);
    }

    match crate::loaders::start_game(&input.record, &prepared.loader) {
        Ok(game) => Ok(Started {
            game,
            helpers,
            kill_on_exit: serverbox::image_name(&input.record).into_iter().collect(),
        }),
        Err(e) => {
            for mut helper in helpers {
                let _ = helper.kill();
            }
            Err(LaunchProblem::failed(t!("page_launch.game_launch_failed", error = e)))
        }
    }
}

