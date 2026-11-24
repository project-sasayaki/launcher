//! What the segatools games (Initial D The Arcade on ALLS, Initial D Arcade Stage Zero on
//! Nu) have in common: segatools writes the configs next to amdaemon.exe, and the launch
//! starts amdaemon and the game, each through `inject.exe` with the game's hook.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};

use rust_i18n::t;

use super::{Fix, LaunchInput, LaunchProblem, Prepared, Started, is_blank, serverbox};
use crate::loaders::{self, segatools};
use crate::storage::GameRecord;
use crate::win_util::CREATE_NO_WINDOW;

/// The network is ready when this PC is in a room, or a subnet is saved on the Network page.
pub fn network_configured(input: &LaunchInput) -> bool {
    input.room.is_some() || !is_blank(&input.saved_subnet)
}

/// The checks of these games: a card, amdaemon.exe and the game executable must be set.
pub fn check(input: &LaunchInput) -> Result<(), LaunchProblem> {
    if is_blank(&input.card) {
        return Err(LaunchProblem::needs(t!("page_launch.no_card_selected"), Fix::Cards));
    }
    if is_blank(&input.record.amdaemon) {
        return Err(LaunchProblem::needs(t!("page_launch.amdaemon_exe_is_not_configured"), Fix::GameRow));
    }
    if is_blank(&input.record.game_exe) {
        return Err(LaunchProblem::needs(t!("page_launch.game_executable"), Fix::GameRow));
    }
    Ok(())
}

/// Copies segatools next to amdaemon.exe and writes `segatools.ini`; the loader to start
/// with is the copy of `inject.exe` there.
pub fn prepare(input: &LaunchInput) -> Result<Prepared, LaunchProblem> {
    let deploy = || -> Result<PathBuf, String> {
        let inject = loaders::resolve_loader(&input.record, &input.settings, &input.loaders)?;
        let (_, dir) = amdaemon_of(&input.record)?;
        segatools::deploy(&dir, inject.parent().unwrap_or(Path::new(".")), &snapshot(input))?;

        let local_inject = dir.join("inject.exe");
        if !local_inject.is_file() {
            return Err(t!("launch.inject_exe_not_found_in", local_inject = local_inject.display()).to_string());
        }
        Ok(local_inject)
    };
    let loader = deploy().map_err(|e| LaunchProblem::failed(t!("page_launch.game_launch_failed", error = e)))?;
    Ok(Prepared { loader, card_reader: None })
}

/// Starts amdaemon, then the game. `serverbox_image` is the ServerBox a game of this
/// kind starts by itself, to end along with it.
pub fn start(input: &LaunchInput, prepared: Prepared, serverbox_image: Option<&str>) -> Result<Started, LaunchProblem> {
    let snapshot = snapshot(input);
    let kind = segatools::game_kind(snapshot.game_id.as_deref());

    let mut amdaemon = spawn_amdaemon(&input.record, &prepared.loader, kind, &snapshot)
        .map_err(|e| LaunchProblem::failed(t!("page_launch.failed_to_start_amdaemon", error = e)))?;
    let game = match spawn_game(&input.record, &prepared.loader, kind, &snapshot) {
        Ok(game) => game,
        Err(e) => {
            let _ = amdaemon.kill();
            return Err(LaunchProblem::failed(t!("page_launch.game_launch_failed", error = e)));
        }
    };

    let mut kill_on_exit = vec!["amdaemon.exe".to_string()];
    kill_on_exit.extend(serverbox::image_name(&input.record));
    if let Some(image) = serverbox_image {
        if !kill_on_exit.iter().skip(1).any(|n| n.eq_ignore_ascii_case(image)) {
            kill_on_exit.push(image.to_string());
        }
    }
    Ok(Started { game, helpers: vec![amdaemon], kill_on_exit })
}

/// segatools' settings for the game. In a room, the room decides whether this PC is the
/// host and which seat it has.
fn snapshot(input: &LaunchInput) -> segatools::DeploySnapshot {
    let mut system = input.system.clone();
    if let (Some(_), Some(seat)) = (&input.room, input.room_seat) {
        system.master = Some(seat.is_host);
        if let Some(name) = seat.seat {
            system.seat = Some(name.to_string());
        }
    }
    segatools::DeploySnapshot {
        net: input.net.clone(),
        system,
        coin: input.coin.clone(),
        input: input.input.clone(),
        graphics: input.graphics.clone(),
        version: input.record.version.clone(),
        game_id: input.record.game_id.clone(),
        aime_card: input.card.clone(),
        room: input.room.clone(),
    }
}

/// amdaemon.exe and its folder, where segatools and the game run.
fn amdaemon_of(record: &GameRecord) -> Result<(PathBuf, PathBuf), String> {
    let amdaemon = record
        .amdaemon
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| t!("launch.this_game_has_no_amdaemon").to_string())?;
    if !amdaemon.is_file() {
        return Err(t!("launch.amdaemon_exe_does_not_exist", amdaemon = amdaemon.display()).to_string());
    }
    let dir = amdaemon.parent().map(PathBuf::from).ok_or_else(|| {
        t!("launch.cannot_determine_amdaemon_directory", amdaemon = amdaemon.display()).to_string()
    })?;
    Ok((amdaemon, dir))
}

/// The game's main executable.
fn game_exe_of(record: &GameRecord) -> Result<PathBuf, String> {
    let exe = record
        .game_exe
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| t!("launch.this_game_has_no_game").to_string())?;
    if !exe.is_file() {
        return Err(t!("launch.game_executable_does_not_exist", exe = exe.display()).to_string());
    }
    Ok(exe)
}

/// `inject.exe -d -k <hook> amdaemon.exe -c <configs>` in amdaemon's folder.
fn spawn_amdaemon(
    record: &GameRecord,
    inject: &Path,
    kind: segatools::GameKind,
    snapshot: &segatools::DeploySnapshot,
) -> Result<Child, String> {
    let dir = Path::new(record.amdaemon.as_deref().unwrap_or_default())
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_default();
    Command::new(inject)
        .current_dir(&dir)
        .args(segatools::amdaemon_args(kind, snapshot.game_id.as_deref()))
        .spawn()
        .map_err(|e| t!("launch.failed_to_start_amdaemon", inject = inject.display(), error = e).to_string())
}

/// `inject.exe -d -k <hook> <game exe> …` in amdaemon's folder, without a console window.
fn spawn_game(
    record: &GameRecord,
    inject: &Path,
    kind: segatools::GameKind,
    snapshot: &segatools::DeploySnapshot,
) -> Result<Child, String> {
    let (_, dir) = amdaemon_of(record)?;
    let game_exe = game_exe_of(record)?;
    let culture = segatools::culture_of(snapshot.system.region.as_deref());
    let user_dir = segatools::find_userdata_dir(&dir);
    Command::new(inject)
        .current_dir(&dir)
        .args(segatools::game_args(kind, &game_exe, culture, &user_dir))
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|e| t!("launch.failed_to_start_game", game_exe = game_exe.display(), error = e).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::games::{RoomSeat, test_input};
    use crate::loaders::mice::RoomNetOverride;
    use crate::storage::Loader;

    fn in_room(seat: RoomSeat) -> LaunchInput {
        let mut input = test_input::input(Loader::Segatools, Some("SDGT"));
        input.room = Some(RoomNetOverride {
            cidr: crate::awl::ROOM_CIDR.to_string(),
            my_game_ip: None,
            server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
        });
        input.room_seat = Some(seat);
        input
    }

    #[test]
    fn outside_a_room_the_system_page_decides_the_seat() {
        let mut input = test_input::input(Loader::Segatools, Some("SDGT"));
        input.system.master = Some(false);
        input.system.seat = Some("B2".into());
        let snap = snapshot(&input);
        assert_eq!(snap.system.master, Some(false));
        assert_eq!(snap.system.seat.as_deref(), Some("B2"));
        assert_eq!(snap.aime_card, input.card);
        assert_eq!(snap.net, input.net);
    }

    #[test]
    fn in_a_room_the_room_decides_the_seat() {
        let host = snapshot(&in_room(RoomSeat { is_host: true, seat: Some("A1") }));
        assert_eq!(host.system.master, Some(true));
        assert_eq!(host.system.seat.as_deref(), Some("A1"));

        let mut member_input = in_room(RoomSeat { is_host: false, seat: None });
        member_input.system.seat = Some("SINGLE".into());
        let member = snapshot(&member_input);
        assert_eq!(member.system.master, Some(false));
        assert_eq!(member.system.seat.as_deref(), Some("SINGLE"), "without a seat address the saved seat stays");
    }

    #[test]
    fn network_is_ready_in_a_room_or_with_a_saved_subnet() {
        let mut input = test_input::input(Loader::Segatools, Some("SDGT"));
        assert!(network_configured(&input));
        input.saved_subnet = Some("  ".into());
        assert!(!network_configured(&input));
        assert!(network_configured(&in_room(RoomSeat { is_host: true, seat: Some("A1") })));
    }
}
