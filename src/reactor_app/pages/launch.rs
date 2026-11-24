use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use windows_reactor::*;

use rust_i18n::t;

use crate::games::{self, Fix, LaunchInput, LaunchProblem, RoomSeat, Wmmt3Settings};
use crate::loaders;
use crate::storage::GameRecord;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::state::{GameRuntime, ServerBoxRuntime};
use crate::reactor_app::widgets::now_secs;

pub(in crate::reactor_app) const GAME_POLL_INTERVAL: Duration = Duration::from_secs(1);

impl Launcher {
    /// Everything a launch of `rec` needs, from the settings in effect now (the room's
    /// network while this PC is in a room).
    fn launch_input(&mut self, rec: &GameRecord, card: Option<String>) -> LaunchInput {
        // A change still waiting to be saved is part of the game's settings.
        self.save_pending_now();
        self.detected = loaders::detect_loaders();
        let room = self.room_net_override();

        let mut net = self.net.clone();
        net.host_ip = self.effective_game_ip();
        net.serverbox_ip = self.effective_serverbox_ip();
        net.ip = self.effective_subnet();
        if room.is_some() {
            net.simulate = Some(false);
        }

        let saved = self.store.load_game_settings(rec);
        let mut system = saved.system;
        system.keychip = self.keychip.clone();
        system.region = rec.region.clone().or_else(|| self.store.load_system().region);

        LaunchInput {
            record: rec.clone(),
            settings: self.settings.clone(),
            loaders: self.detected.clone(),
            net,
            saved_subnet: self.net.ip.clone(),
            system,
            coin: saved.coin,
            graphics: saved.graphics,
            input: saved.input,
            card,
            room_seat: room.as_ref().map(|_| self.room_seat()),
            room,
            serverbox_adapter: self.selected_adapter().map(|a| a.friendly.clone()),
            wmmt3: Wmmt3Settings::load(&self.store.game_settings_path(rec)),
        }
    }

    /// Whether this PC hosts the room it is in, and the seat its room address stands for.
    fn room_seat(&self) -> RoomSeat {
        let is_host = matches!(self.room_status, crate::awl::RoomStatus::Hosting { .. });
        let seat_ip = if is_host { Some(crate::awl::HOST_GAME_IP.to_string()) } else { self.room_my_ip.clone() };
        let seat = seat_ip
            .as_deref()
            .and_then(|ip| ip.rsplit('.').next())
            .and_then(|s| s.parse::<u8>().ok())
            .and_then(super::room::seat_name_of_octet);
        RoomSeat { is_host, seat }
    }

    /// Where the launch-failed dialog sends the user to fix `problem`.
    fn goto_for(problem: &LaunchProblem, index: usize) -> Option<(String, Option<usize>)> {
        let (page, game) = match problem.fix? {
            Fix::GameRow => ("games", Some(index)),
            Fix::Cards => ("cards", Some(index)),
            Fix::Keychip => ("settings", None),
            Fix::Network => ("network", None),
            // The Input tab of the game's settings page (see `apply_dialog`).
            Fix::Input => ("game_settings", Some(index)),
        };
        Some((page.to_string(), game))
    }

    pub(in crate::reactor_app) fn do_launch(&mut self, context: &ComponentContext<Self>, index: usize) {
        let Some(rec) = self.library.games.get(index).cloned() else {
            return;
        };
        self.launch_overlay = Some(index);

        let card = self.selected_card(&rec);
        let input = self.launch_input(&rec, card);

        context.spawn_background(move |_| match games::launch_game(&input) {
            Ok(running) => Message::GameLaunched {
                runtime: GameRuntime {
                    index,
                    child: Arc::new(Mutex::new(running.started.game)),
                    companions: Arc::new(Mutex::new(running.started.helpers)),
                    kill_on_exit: running.started.kill_on_exit,
                    static_ip: running.static_ip,
                },
            },
            Err(problem) => Message::LaunchDone {
                index,
                goto: Self::goto_for(&problem, index),
                result: Err(problem.message),
            },
        });
    }

    pub(in crate::reactor_app) fn spawn_game_poll(context: &ComponentContext<Self>) {
        context.spawn_background(move |_| {
            thread::sleep(GAME_POLL_INTERVAL);
            Message::GamePollTick
        });
    }

    pub(in crate::reactor_app) fn do_launch_serverbox(&mut self, context: &ComponentContext<Self>, index: usize) {
        let Some(rec) = self.library.games.get(index).cloned() else {
            return;
        };
        if rec.serverbox.is_none() {
            return;
        }
        if !games::family_of(&rec).has_serverbox() {
            self.dialog = Dialog::Error {
                message: t!("common.segatools_has_no_serverbox").to_string(),
            };
            return;
        }

        let input = self.launch_input(&rec, None);
        context.spawn_background(move |_| match games::launch_serverbox(&input) {
            Ok(child) => Message::ServerBoxLaunched {
                result: Ok(()),
                runtime: Some(ServerBoxRuntime { index, child: Arc::new(Mutex::new(child)) }),
                goto: None,
            },
            Err(problem) => Message::ServerBoxLaunched {
                goto: Self::goto_for(&problem, index),
                result: Err(problem.message),
                runtime: None,
            },
        });
    }

    pub(in crate::reactor_app) fn handle_launch_done(
        &mut self,
        index: usize,
        result: Result<(), String>,
        goto: Option<(String, Option<usize>)>,
    ) {
        self.game_running = false;

        if self.launch_overlay == Some(index) {
            self.launch_overlay = None;
        }
        match result {
            Ok(()) => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.play_count += 1;
                    g.last_played = Some(now_secs());
                }
                self.store.save_library(&self.library);
            }
            Err(e) => {

                self.dialog = Dialog::LaunchFailed { message: e, goto };
            }
        }
    }

    pub(in crate::reactor_app) fn poll_game_exit(&mut self, context: &ComponentContext<Self>) {
        let rt = match self.game_runtime.take() {
            Some(r) => r,
            None => return,
        };
        let exited = match rt.child.lock() {
            Ok(mut c) => c.try_wait().map(|s| s.is_some()).unwrap_or(true),
            Err(_) => true,
        };
        if exited {

            if let Ok(mut v) = rt.companions.lock() {
                for mut ch in v.drain(..) {
                    let _ = ch.kill();
                }
            }

            rt.taskkill_images();
            self.game_running = false;

            self.launch_overlay = None;
            if let Some(g) = self.library.games.get_mut(rt.index) {
                g.play_count += 1;
                g.last_played = Some(now_secs());
            }
            self.store.save_library(&self.library);

            if let Some(ip) = rt.static_ip.clone() {
                context.spawn_background(move |_| Message::StaticIpRemoved(ip.remove()));
            }
        } else {

            self.game_runtime = Some(rt);
            Self::spawn_game_poll(context);
        }
    }

}
