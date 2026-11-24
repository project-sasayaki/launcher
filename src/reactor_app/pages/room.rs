use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

use windows_reactor::*;

use crate::awl::{AwlManager, RoomStatus};
use crate::roomdir::RoomDirClient;
use rust_i18n::t;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::widgets::{record_matches_product, selectable_text, separator, settings_row};

const HOST_DOWN_TIMEOUT: Duration = Duration::from_secs(10);

const MEMBER_KICK_AFTER: Duration = Duration::from_secs(10);

const MEMBER_JOIN_GRACE: Duration = Duration::from_secs(90);

pub(in crate::reactor_app) const ROOM_DEFAULT_NICKNAME: &str = "player1234";

const TICK_INTERVAL_IN_ROOM: Duration = Duration::from_secs(1);

const TICK_INTERVAL_OFF_PAGE: Duration = Duration::from_secs(15);

impl Launcher {
    pub(in crate::reactor_app) fn dir_client(&self) -> RoomDirClient {
        RoomDirClient::new(
            self.room
                .directory
                .clone()
                .unwrap_or_else(|| crate::roomdir::DEFAULT_DIRECTORY.to_string()),
        )
    }

    pub(in crate::reactor_app) fn room_net_override(
        &self,
    ) -> Option<crate::loaders::mice::RoomNetOverride> {
        match &self.room_status {
            RoomStatus::Hosting { .. } => Some(crate::loaders::mice::RoomNetOverride {
                cidr: crate::awl::ROOM_CIDR.to_string(),
                my_game_ip: None,
                server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
            }),
            RoomStatus::Joined { .. } => Some(crate::loaders::mice::RoomNetOverride {
                cidr: crate::awl::ROOM_CIDR.to_string(),
                my_game_ip: self.room_my_ip.clone(),
                server_ip: crate::awl::HOST_SERVERBOX_IP.to_string(),
            }),
            _ => None,
        }
    }

    pub(in crate::reactor_app) fn effective_game_ip(&self) -> Option<String> {
        match self.room_net_override() {
            Some(override_) => Some(
                override_
                    .my_game_ip
                    .unwrap_or_else(|| crate::awl::HOST_GAME_IP.to_string()),
            ),
            None => self
                .net
                .host_ip
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .or_else(|| {
                    self.selected_adapter()
                        .and_then(|a| a.ips.first())
                        .map(|ip| ip.ip.clone())
                }),
        }
    }

    pub(in crate::reactor_app) fn effective_serverbox_ip(&self) -> Option<String> {
        if self.room_net_override().is_some() {
            return Some(crate::awl::HOST_SERVERBOX_IP.to_string());
        }
        let saved = self
            .net
            .serverbox_ip
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        saved.or_else(|| {
            let a = self.selected_adapter()?;
            a.ips
                .get(1)
                .map(|ip| ip.ip.clone())
                .or_else(|| a.ips.first().and_then(|ip| crate::netinfo::next_ip(&ip.ip)))
        })
    }

    pub(in crate::reactor_app) fn effective_subnet(&self) -> Option<String> {
        match self.room_net_override() {
            Some(override_) => Some(override_.cidr),
            None => self
                .selected_adapter()
                .and_then(|a| a.ips.first())
                .map(crate::netinfo::network_cidr)
                .or_else(|| {
                    self.net
                        .ip
                        .as_deref()
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                }),
        }
    }

    pub(in crate::reactor_app) fn room_game_index(&self) -> Option<usize> {
        let game = self.room_game.trim();
        if game.is_empty() {
            return None;
        }
        self.library
            .games
            .iter()
            .position(|r| record_matches_product(r, game))
    }

    pub(in crate::reactor_app) fn room_has_serverbox(&self) -> bool {
        let Some(index) = self.room_game_index() else {
            return false;
        };
        self.library
            .games
            .get(index)
            .and_then(|r| r.serverbox.as_deref())
            .map(str::trim)
            .is_some_and(|s| !s.is_empty())
    }

    pub(in crate::reactor_app) fn room_uses_serverbox(&self) -> bool {
        let Some(index) = self.room_game_index() else {
            return false;
        };
        matches!(
            self.library.games.get(index).map(|r| &r.loader),
            Some(crate::storage::Loader::Kitten | crate::storage::Loader::Mice)
        )
    }

    pub(in crate::reactor_app) fn room_launch(
        &mut self,
        context: &windows_reactor::ComponentContext<Self>,
        serverbox: bool,
    ) {
        let Some(index) = self.room_game_index() else {
            let game = self.room_game.clone();
            self.dialog = Dialog::Error {
                message: t!("page_room.game_is_not_in_library", game = game).to_string(),
            };
            self.nav_tag = "games".to_string();
            return;
        };
        if serverbox {
            if self
                .library
                .games
                .get(index)
                .and_then(|r| r.serverbox.clone())
                .is_none()
            {
                self.dialog = Dialog::Error {
                    message: t!("page_room.serverbox_is_not_configured_for")
                    .to_string(),
                };
                self.nav_tag = "games".to_string();
                return;
            }
            self.do_launch_serverbox(context, index);
        } else {
            self.do_launch(context, index);
        }
    }

    pub(in crate::reactor_app) fn room_refresh_list(
        &mut self,
        context: &windows_reactor::ComponentContext<Self>,
        with_overlay: bool,
    ) {
        let overlay = with_overlay;
        if overlay {
            self.room_overlay = Some(t!("page_room.refreshing_room_list").to_string());
        }
        let dir = self.dir_client();
        let filter = self.room_filter_game_id.clone();
        context.spawn_background(move |_| Message::RoomListRefreshed(dir.list_rooms(filter.as_deref()), overlay));
    }

    pub(in crate::reactor_app) fn room_nickname(&self) -> String {
        if let Some(n) = self.room.nickname.as_deref() {
            let n = n.trim();
            if !n.is_empty() {
                return n.to_string();
            }
        }
        self.room_nick_override.clone().unwrap_or_default()
    }

    pub(in crate::reactor_app) fn room_start_host(
        &mut self,
        name: String,
        game_idx: Option<usize>,
        password: String,
        context: &windows_reactor::ComponentContext<Self>,
    ) {
        if self.room_status != RoomStatus::Idle {
            return;
        }

        if self.room_nickname().is_empty() {
            self.room_nick_pending =
                Some(crate::reactor_app::message::PendingRoomAction::Create {
                    name,
                    game_idx,
                    password,
                });
            context.spawn_background(|_| {
                std::thread::sleep(std::time::Duration::from_millis(150));
                Message::RoomNickPromptReady
            });
            return;
        }
        let name = name.trim().to_string();
        let Some(game_idx) = game_idx else {
            self.dialog = Dialog::Error {
                message: t!("page_room.select_game").to_string(),
            };
            return;
        };
        let games = crate::db::room_games();
        let Some(entry) = games.get(game_idx) else {
            self.dialog = Dialog::Error {
                message: t!("page_room.selected_game_does_not_exist").to_string(),
            };
            return;
        };

        let game_id = entry.game_id.clone().unwrap_or_else(|| entry.name.clone());
        let game = entry.name.clone();
        let keychip = self.keychip.clone().unwrap_or_default();
        if keychip.trim().is_empty() {
            self.dialog = Dialog::Error {
                message: t!("page_room.keychip_serial_is_not_configured").to_string(),
            };
            return;
        }
        self.room_keychip = keychip.clone();
        self.room.last_room_id = None;
        self.room.last_room_password = None;
        let _ = self.store.save_room(&self.room);
        let overlay_text = if name.is_empty() {
            t!("page_room.creating_room").to_string()
        } else {
            t!("page_room.creating_named_room", name = name).to_string()
        };
        self.room_overlay = Some(overlay_text);
        self.room_status = RoomStatus::Creating { name: name.clone() };
        self.room_game = game.clone();
        self.room_password = password.clone();
        self.room_members_invited.clear();
        let cfg = self.room.clone();
        let dir = self.dir_client();
        let handle = self.room_handle.clone();
        context.spawn_background(move |_| {
            match AwlManager::host_flow(&cfg, &name, &game, &game_id, &password, &keychip, &dir) {
                Ok((h, room_id)) => {
                    *handle.lock().unwrap() = Some(h);
                    Message::RoomCreateResult(Ok(room_id))
                }
                Err(e) => Message::RoomCreateResult(Err(e)),
            }
        });
    }

    pub(in crate::reactor_app) fn room_on_created(
        &mut self,
        res: Result<String, String>,
        context: &windows_reactor::ComponentContext<Self>,
    ) {
        match res {
            Ok(room_id) => {
                let mut name = match &self.room_status {
                    RoomStatus::Creating { name } => name.clone(),
                    _ => String::new(),
                };
                if name.is_empty() {
                    name = room_id.clone();
                }
                self.room_name = name.clone();
                self.room_status = RoomStatus::Hosting { room_id, name };
                self.room_my_ip = Some(crate::awl::HOST_GAME_IP.to_string());

                self.room.last_room_password = Some(self.room_password.clone());
                let _ = self.store.save_room(&self.room);
                        self.room_members_invited.clear();

                self.room_dir_members = None;
                self.room_last_reconcile = None;
                self.room_last_playing_report = Some(self.game_running);
                let host_sync = crate::room_sync::HostSync::start();
                let host_peer = AwlManager::my_peer_id().unwrap_or_default();
                let nick = self.room_nickname();
                host_sync.publish(&host_peer, &[], self.game_running, Some(&nick));
                self.room_host_sync = Some(host_sync);
                self.room_schedule_tick(self.room_tick_interval(), context);
                self.spawn_room_watchdog();
            }
            Err(e) => {
                self.room_status = RoomStatus::Idle;
                self.dialog = Dialog::Error { message: e };
            }
        }
    }

    pub(in crate::reactor_app) fn spawn_room_watchdog(&mut self) {
        if !matches!(self.room_status, RoomStatus::Hosting { .. }) {
            return;
        }
        let room_id = match &self.room_status {
            RoomStatus::Hosting { room_id, .. } => room_id.clone(),
            _ => return,
        };
        let keychip = self.room_keychip.clone();

        let Ok(peer_id) = AwlManager::my_peer_id() else {
            return;
        };
        let directory = self
            .room
            .directory
            .clone()
            .unwrap_or_else(|| crate::roomdir::DEFAULT_DIRECTORY.to_string());

        if let Some(mut w) = self.room_watchdog.take() {
            let _ = w.kill();
        }
        let Ok(exe) = std::env::current_exe() else {
            return;
        };
        let pid = std::process::id();
        let mut cmd = std::process::Command::new(exe);
        cmd.arg("--room-watchdog")
            .arg(pid.to_string())
            .arg(&room_id)
            .arg(&keychip)
            .arg(&peer_id)
            .arg(&directory);

        #[cfg(windows)]

        cmd.creation_flags(crate::win_util::CREATE_NO_WINDOW | 0x0100_0000);
        if let Ok(child) = cmd.spawn() {
            self.room_watchdog = Some(child);
        }
    }

    pub(in crate::reactor_app) fn room_start_join(
        &mut self,
        room_id: String,
        password: String,
        context: &windows_reactor::ComponentContext<Self>,
    ) {
        if self.room_status != RoomStatus::Idle {
            return;
        }

        if self.room_nickname().is_empty() {
            self.room_nick_pending =
                Some(crate::reactor_app::message::PendingRoomAction::Join {
                    room_id,
                    password,
                });
            context.spawn_background(|_| {
                std::thread::sleep(std::time::Duration::from_millis(150));
                Message::RoomNickPromptReady
            });
            return;
        }
        let room_id = room_id.trim().to_uppercase();
        if room_id.is_empty() {
            self.dialog = Dialog::Error {
                message: t!("page_room.enter_room_id").to_string(),
            };
            return;
        }
        self.room.last_room_password = Some(password.clone());

        self.room_join_password = password.clone();
        self.room_overlay = Some(t!("page_room.joining_room").to_string());
        self.room_status = RoomStatus::Joining { room_id: room_id.clone() };
        let cfg = self.room.clone();
        let dir = self.dir_client();
        let handle = self.room_handle.clone();
        context.spawn_background(move |_| match AwlManager::join_flow(&cfg, &room_id, &password, &dir) {
            Ok((h, join)) => {
                *handle.lock().unwrap() = Some(h);
                Message::RoomJoinResult(Ok(join))
            }
            Err(e) => Message::RoomJoinResult(Err(e)),
        });
    }

    pub(in crate::reactor_app) fn room_on_joined(
        &mut self,
        res: Result<crate::roomdir::JoinInfo, String>,
        context: &windows_reactor::ComponentContext<Self>,
    ) {
        match res {
            Ok(join) => {
                let room_id = match &self.room_status {
                    RoomStatus::Joining { room_id } => room_id.clone(),
                    _ => String::new(),
                };

                self.room.last_room_id = Some(room_id.clone());
                self.room.last_room_password = Some(self.room_join_password.clone());
                let _ = self.store.save_room(&self.room);
                self.room_status = RoomStatus::Joined { room_id: room_id.clone(), is_host: false };
                self.room_my_ip = Some(join.your_ip);
                self.room_game = join.game;

                self.room_name = if join.name.is_empty() { room_id } else { join.name };
                self.room_host_down = None;

                self.room_sync_version = 0;
                self.room_sync_table = None;
                self.room_last_playing_report = None;
                self.room_last_status_sent = None;
                self.room_member_sync = Some(crate::room_sync::MemberSync::start(
                    AwlManager::my_peer_id().unwrap_or_default(),
                    self.room_nickname(),
                ));
                self.room_schedule_tick(self.room_tick_interval(), context);
            }
            Err(e) => {
                self.room_status = RoomStatus::Idle;

                self.room.last_room_id = None;
                self.room.last_room_password = None;
                let _ = self.store.save_room(&self.room);
                self.dialog = Dialog::Error { message: e };
            }
        }
    }

    pub(in crate::reactor_app) fn room_stop(
        &mut self,
        context: &windows_reactor::ComponentContext<Self>,
    ) {

        if let Some(mut w) = self.room_watchdog.take() {
            let _ = w.kill();
        }
        let (is_host, room_id) = match &self.room_status {
            RoomStatus::Hosting { room_id, .. } => (true, room_id.clone()),
            RoomStatus::Joined { room_id, is_host } => (*is_host, room_id.clone()),
            _ => return,
        };
        self.room_status = RoomStatus::Idle;
        self.room_members_invited.clear();
        self.room_member_down.clear();
        self.room_member_ever_connected.clear();
        self.room_seat_ips.clear();
        self.room_last_heartbeat = None;
        self.room_host_down = None;
        self.room_my_ip = None;
        self.room_saw_self_in_table = false;
        self.room_nick_pending = None;
        self.room_game.clear();
        self.room_name.clear();

        if is_host {
            if let Some(sync) = self.room_host_sync.as_ref() {
                sync.notify_close();

                std::thread::sleep(std::time::Duration::from_millis(600));
            }
        }

        self.room_host_sync = None;
        self.room_member_sync = None;
        self.room_sync_version = 0;
        self.room_sync_table = None;
        self.room_dir_members = None;
        self.room_last_reconcile = None;
        self.room_last_playing_report = None;
        self.room_last_status_sent = None;
        self.room.last_room_id = None;
        self.room.last_room_password = None;
        let _ = self.store.save_room(&self.room);

        self.room_overlay = Some(if is_host {
            t!("page_room.closing_room").to_string()
        } else {
            t!("page_room.leaving_room").to_string()
        });
        let handle = self.room_handle.lock().unwrap().take();
        let dir = self.dir_client();

        let keychip = self.room_keychip.clone();
        context.spawn_background(move |_| {

            let my = AwlManager::my_peer_id().ok();

            let friendly = |e: String| {
                if e == "HTTP 404" {
                    t!("page_room.room_has_been_closed").to_string()
                } else {
                    e
                }
            };
            let res = if is_host {
                dir.close(&room_id, my.as_deref(), &keychip).map_err(|e| {
                    friendly(t!("page_room.failed_to_close_room", error = e).to_string())
                })
            } else {
                match my {
                    Some(peer) => dir.leave(&room_id, &peer).map_err(|e| {
                        friendly(t!("page_room.failed_to_leave_room", error = e).to_string())
                    }),
                    None => Err(t!("page_room.local_awl_identity_not_found").to_string()),
                }
            };
            AwlManager::remove_all_peers();
            drop(handle);
            Message::RoomFinished(res)
        });
    }

    pub(in crate::reactor_app) fn room_try_resume(
        &mut self,
        context: &windows_reactor::ComponentContext<Self>,
    ) {
        if self.room_status != RoomStatus::Idle {
            return;
        }
        let keychip = self.keychip.clone().unwrap_or_default();
        let has_keychip = !keychip.trim().is_empty();
        let last_room_id = self.room.last_room_id.clone().unwrap_or_default();
        let last_password = self.room.last_room_password.clone().unwrap_or_default();
        let has_last = !last_room_id.trim().is_empty();
        if !has_keychip && !has_last {
            return;
        }
        if has_keychip {
            self.room_keychip = keychip.clone();
        }

        if has_last {
            self.room_join_password = last_password.clone();
            self.room_status = RoomStatus::Joining { room_id: last_room_id.clone() };
        }
        self.room_overlay = Some(t!("page_room.returning_to_room").to_string());
        let cfg = self.room.clone();
        let dir = self.dir_client();
        let handle = self.room_handle.clone();
        context.spawn_background(move |_| {

            if has_keychip {
                match dir.my_room(&keychip) {
                    Ok(Some(room)) => {

                        let game_id = crate::db::room_games()
                            .into_iter()
                            .find(|g| g.name == room.game)
                            .and_then(|g| g.game_id);
                        match AwlManager::host_resume_flow(&cfg, game_id.as_deref()) {
                            Ok(h) => {
                                *handle.lock().unwrap() = Some(h);
                                return Message::RoomResumeResult(Ok(Some(room)));
                            }
                            Err(e) => return Message::RoomResumeResult(Err(e)),
                        }
                    }
                    Ok(None) => {}
                    Err(e) => return Message::RoomResumeResult(Err(e)),
                }
            }

            if has_last {
                match AwlManager::join_flow(&cfg, &last_room_id, &last_password, &dir) {
                    Ok((h, join)) => {
                        *handle.lock().unwrap() = Some(h);
                        return Message::RoomJoinResult(Ok(join));
                    }
                    Err(e) => return Message::RoomJoinResult(Err(e)),
                }
            }
            Message::RoomResumeResult(Ok(None))
        });
    }

    pub(in crate::reactor_app) fn room_on_tick(
        &mut self,
        context: &windows_reactor::ComponentContext<Self>,
    ) {

        self.room_peers = AwlManager::known_peers();
        match self.room_status.clone() {
            RoomStatus::Hosting { room_id, .. } => {
                let dir = self.dir_client();

                let unknown_connected = match &self.room_dir_members {
                    Some(cache) => {
                        let known: HashSet<&str> =
                            cache.members.iter().map(|m| m.peer_id.as_str()).collect();
                        self.room_peers.iter().any(|p| {
                            p.connected && p.alias != "host" && !known.contains(p.peer_id.as_str())
                        })
                    }
                    None => true,
                };
                let reconcile_due = self
                    .room_last_reconcile
                    .map_or(true, |t| t.elapsed() >= Duration::from_secs(60));
                if unknown_connected || reconcile_due {
                    self.room_last_reconcile = Some(Instant::now());
                    match dir.members(&room_id) {
                        Ok(members) => self.room_dir_members = Some(members),
                        Err(_) => {}
                    }
                }
                if let Some(cache) = self.room_dir_members.clone() {
                    let mut ids: HashSet<String> =
                        cache.members.iter().map(|m| m.peer_id.clone()).collect();
                    ids.insert(cache.host_peer_id.clone());

                    let mut down_now: HashMap<String, Instant> = HashMap::new();
                    let mut kicked = false;
                    for m in &cache.members {
                        let connected = self
                            .room_peers
                            .iter()
                            .find(|p| p.peer_id == m.peer_id)
                            .map(|p| p.connected)
                            .unwrap_or(false);
                        if connected {
                            self.room_member_ever_connected.insert(m.peer_id.clone());
                            continue;
                        }
                        let grace = if self.room_member_ever_connected.contains(&m.peer_id) {
                            MEMBER_KICK_AFTER
                        } else {
                            MEMBER_JOIN_GRACE
                        };
                        let since = match self.room_member_down.get(&m.peer_id) {
                            Some(t) => *t,
                            None => {
                                down_now.insert(m.peer_id.clone(), Instant::now());
                                continue;
                            }
                        };
                        if since.elapsed() >= grace {
                            let _ = dir.leave(&room_id, &m.peer_id);
                            ids.remove(&m.peer_id);
                            kicked = true;
                        } else {
                            down_now.insert(m.peer_id.clone(), since);
                        }
                    }
                    self.room_member_down = down_now;
                    let mut cache = cache;
                    if kicked {
                        cache.members.retain(|m| ids.contains(&m.peer_id));
                        self.room_dir_members = Some(cache.clone());
                    }

                    self.room_member_ever_connected.retain(|p| ids.contains(p));

                    for m in &cache.members {
                        if !ids.contains(&m.peer_id) {
                            continue;
                        }
                        if !self.room_members_invited.iter().any(|p| p == &m.peer_id) {
                            if AwlManager::invite_member(&m.peer_id, &m.ip).is_ok() {
                                self.room_members_invited.push(m.peer_id.clone());
                            }
                        }
                    }
                    self.room_peers.retain(|p| ids.contains(&p.peer_id));
                    self.room_members_invited.retain(|p| ids.contains(p));

                    self.room_seat_ips = cache
                        .members
                        .iter()
                        .map(|m| (m.peer_id.clone(), m.ip.clone()))
                        .chain(std::iter::once((
                            cache.host_peer_id.clone(),
                            crate::awl::HOST_GAME_IP.to_string(),
                        )))
                        .collect();

                    if let Some(sync) = self.room_host_sync.as_ref() {
                        let pairs: Vec<(String, String)> = cache
                            .members
                            .iter()
                            .map(|m| (m.peer_id.clone(), m.ip.clone()))
                            .collect();
                        let nick = self.room_nickname();
                        sync.publish(&cache.host_peer_id, &pairs, self.game_running, Some(&nick));
                    }

                    if self
                        .room_last_heartbeat
                        .map_or(true, |t| t.elapsed() >= Duration::from_secs(300))
                    {
                        if dir.heartbeat(&room_id, &self.room_keychip).is_ok() {
                            self.room_last_heartbeat = Some(Instant::now());
                        }
                    }
                }
                self.room_schedule_tick(self.room_tick_interval(), context);
            }
            RoomStatus::Joined { room_id: _, is_host: false } => {

                if let Some(sync) = self.room_member_sync.as_ref() {
                    if sync.is_closed() {
                        self.room_stop(context);
                        self.dialog = Dialog::Error {
                            message: t!("page_room.host_has_closed_room")
                            .to_string(),
                        };
                        return;
                    }
                }

                if let Some(sync) = self.room_member_sync.as_ref() {
                    if let Some(table) = sync.take_snapshot() {
                        if table.version != self.room_sync_version {
                            self.room_sync_version = table.version;
                            self.room_sync_table = Some(table.clone());
                            let my = AwlManager::my_peer_id().ok();
                            let self_in_table = my
                                .as_deref()
                                .map(|my| {
                                    my == table.host_peer_id
                                        || table.members.iter().any(|m| m.peer_id == my)
                                })
                                .unwrap_or(false);
                            if self_in_table {
                                self.room_saw_self_in_table = true;
                            }

                            if !self_in_table
                                && self.room_saw_self_in_table
                                && self.nav_tag == "room"
                            {

                                self.room_stop(context);
                                self.dialog = Dialog::Error {
                                    message: t!("page_room.have_been_removed_from_room")
                                    .to_string(),
                                };
                                return;
                            }

                            let still_member = my.is_none() || self_in_table;
                            if still_member {

                                if let Some(my) = &my {
                                    for m in &table.members {
                                        if m.peer_id == *my
                                            || self
                                                .room_members_invited
                                                .iter()
                                                .any(|p| p == &m.peer_id)
                                        {
                                            continue;
                                        }
                                        if AwlManager::invite_member(&m.peer_id, &m.ip).is_ok() {
                                            self.room_members_invited.push(m.peer_id.clone());
                                        }
                                    }
                                }

                                let mut ids: HashSet<String> =
                                    table.members.iter().map(|m| m.peer_id.clone()).collect();
                                ids.insert(table.host_peer_id.clone());
                                self.room_peers.retain(|p| ids.contains(&p.peer_id));
                                self.room_members_invited.retain(|p| ids.contains(p));

                                self.room_seat_ips = table
                                    .members
                                    .iter()
                                    .map(|m| (m.peer_id.clone(), m.ip.clone()))
                                    .chain(std::iter::once((
                                        table.host_peer_id.clone(),
                                        crate::awl::HOST_GAME_IP.to_string(),
                                    )))
                                    .collect();
                            }
                        }
                    }
                }

                if self.nav_tag != "room" {
                    self.room_schedule_tick(self.room_tick_interval(), context);
                    return;
                }
                let host_connected = self
                    .room_peers
                    .iter()
                    .find(|p| p.alias == "host")
                    .map(|p| p.connected)
                    .unwrap_or(false);
                if host_connected {
                    self.room_host_down = None;
                } else {
                    let since = *self.room_host_down.get_or_insert(Instant::now());
                    if since.elapsed() >= HOST_DOWN_TIMEOUT {
                        self.room_stop(context);
                        self.dialog = Dialog::Error {
                            message: t!("page_room.room_disconnected").to_string(),
                        };
                        return;
                    }
                }

                let playing = self.game_running;
                let keepalive_due = self
                    .room_last_status_sent
                    .map_or(true, |t| t.elapsed() >= Duration::from_secs(30));
                if self.room_last_playing_report != Some(playing) || keepalive_due {
                    if let Some(sync) = self.room_member_sync.as_ref() {
                        sync.send_status(playing);
                    }
                    self.room_last_playing_report = Some(playing);
                    self.room_last_status_sent = Some(Instant::now());
                }
                self.room_schedule_tick(self.room_tick_interval(), context);
            }
            _ => {

            }
        }
    }

    pub(in crate::reactor_app) fn room_tick_interval(&self) -> Duration {
        if self.nav_tag == "room" {
            TICK_INTERVAL_IN_ROOM
        } else {
            TICK_INTERVAL_OFF_PAGE
        }
    }

    pub(in crate::reactor_app) fn room_schedule_tick(&self, interval: Duration, context: &windows_reactor::ComponentContext<Self>) {
        context.spawn_background(move |_| {
            std::thread::sleep(interval);
            Message::RoomTick
        });
    }
}

fn seat_label(seat_ip: Option<&str>, alias: &str) -> String {
    let octet = seat_ip
        .and_then(|ip| ip.rsplit('.').next().and_then(|s| s.parse::<u8>().ok()))

        .or_else(|| alias.strip_prefix("join-").and_then(|s| s.parse::<u8>().ok()));
    match octet.and_then(seat_name_of_octet) {
        Some(name) => name.to_string(),
        None => "?".to_string(),
    }
}

pub(in crate::reactor_app) fn seat_name_of_octet(octet: u8) -> Option<&'static str> {
    match octet {
        101 => Some("A1"),
        103 => Some("A2"),
        104 => Some("B1"),
        105 => Some("B2"),
        _ => None,
    }
}

fn member_cells(
    row: i32,
    seat: &str,
    alias: &str,
    ip: &str,
    connected: bool,
    playing_text: String,
    ping_text: String,
    link_text: String,
) -> Vec<KeyedView> {
    let dot_color = if connected {
        Color::argb(255, 0x6C, 0xCB, 0x5F)
    } else {
        Color::argb(255, 0xFF, 0x99, 0xA4)
    };

    let cell = |col: i32, v: View| -> KeyedView {
        KeyedView::new(
            format!("m{row}c{col}"),
            Border::new()
                .margin(Thickness::new(0.0, 8.0, 0.0, 8.0))
                .grid_row(row)
                .grid_column(col)
                .content(v),
        )
    };
    vec![
        cell(0, View::from(
            TextBlock::new()
                .text(seat.to_string())
                .font_weight(FontWeight::SEMI_BOLD)
                .vertical_alignment(VerticalAlignment::Center)
                .text_trimming(TextTrimming::CharacterEllipsis),
        )),
        cell(1, View::from(
            TextBlock::new()
                .text(alias.to_string())
                .vertical_alignment(VerticalAlignment::Center)
                .text_trimming(TextTrimming::CharacterEllipsis),
        )),
        cell(2, View::from(
            TextBlock::new()
                .text(ip.to_string())
                .font_size(12.0)
                .opacity(0.7)
                .vertical_alignment(VerticalAlignment::Center),
        )),
        cell(3, View::from(
            Border::new()
                .width(11.0)
                .height(11.0)
                .corner_radius(5.5)
                .background(Brush::Solid(dot_color))
                .horizontal_alignment(HorizontalAlignment::Left)
                .vertical_alignment(VerticalAlignment::Center),
        )),
        cell(4, View::from(
            TextBlock::new()
                .text(playing_text)
                .font_size(12.0)
                .vertical_alignment(VerticalAlignment::Center),
        )),
        cell(5, View::from(
            TextBlock::new()
                .text(ping_text)
                .font_size(12.0)
                .opacity(0.8)
                .vertical_alignment(VerticalAlignment::Center),
        )),
        cell(6, View::from(
            TextBlock::new()
                .text(link_text)
                .font_size(12.0)
                .opacity(0.7)
                .vertical_alignment(VerticalAlignment::Center),
        )),
    ]
}

fn playing_label(playing: Option<bool>) -> String {
    match playing {
        Some(true) => t!("page_room.playing").to_string(),
        Some(false) => t!("page_room.idle").to_string(),
        None => "—".to_string(),
    }
}

impl Launcher {

    pub(in crate::reactor_app) fn room_page(&self, context: &mut ViewContext<Self>) -> View {
        match &self.room_status {
            RoomStatus::Hosting { .. } | RoomStatus::Joined { .. } => self.room_active_view(context),
            _ => self.room_lobby_view(context),
        }
    }

    fn room_header(&self, _context: &mut ViewContext<Self>, buttons: View) -> View {
        Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .children((
                StackPanel::new()
                    .grid_column(0)
                    .children([TextBlock::new()
                        .text(t!("nav.in_store_battles"))
                        .font_size(26.0)
                        .font_weight(FontWeight::SEMI_BOLD)]),
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(1)
                    .children((buttons,)),
            ))
            .into()
    }

    fn room_lobby_view(&self, context: &mut ViewContext<Self>) -> View {
        let idle = self.room_status == RoomStatus::Idle;
        let busy = !idle;

        let header_buttons: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .children((
                Button::new()
                    .style(ButtonStyle::Accent)
                    .is_enabled(idle)
                    .on_click(context.message(Message::RoomOpenCreateDialog))
                    .content(t!("page_room.create").to_string()),
                Button::new()
                    .is_enabled(idle)
                    .on_click(context.message(Message::RoomOpenJoinDialog))
                    .content(t!("page_room.join_by_id").to_string()),
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .is_enabled(idle)
                    .on_click(context.message(Message::RoomRefreshList))
                    .content(t!("page_room.refresh").to_string()),
            ))
            .into();

        let busy_hint: View = if busy {
            TextBlock::new()
                .text(match &self.room_status {
                    RoomStatus::Creating { name } => {
                        t!("page_room.creating_named_room", name = name).to_string()
                    }
                    RoomStatus::Joining { room_id } => {
                        t!("page_room.joining_room_by_id", room_id = room_id).to_string()
                    }
                    _ => String::new(),
                })
                .font_size(12.0)
                .opacity(0.7)
                .into()
        } else {
            StackPanel::new().into()
        };

        let n = self.room_list.len();
        let mut row_defs = vec![GridLength::Auto];
        row_defs.push(GridLength::Pixel(1.0));
        for _ in 0..n {
            row_defs.push(GridLength::Auto);
        }

        let mut cells: Vec<KeyedView> = Vec::new();
        let headers = [
            t!("page_room.room").to_string(),
            t!("page_room.game").to_string(),
            t!("page_room.players").to_string(),
        ];
        for (c, txt) in headers.iter().enumerate() {
            cells.push(KeyedView::new(format!("h{c}"), View::from(
                TextBlock::new()
                    .text(txt.clone())
                    .font_size(12.0)
                    .opacity(0.6)
                    .grid_row(0)
                    .grid_column(c as i32)
                    .vertical_alignment(VerticalAlignment::Center)
                    )));
        }

        cells.push(KeyedView::new("sep", View::from(
            Border::new()
                .height(1.0)
                .margin(Thickness::new(0.0, 4.0, 0.0, 4.0))
                .background(ThemeBrush::CardStroke)
                .grid_row(1)
                .grid_column(0)
                .grid_column_span(4)
                )));

        for (i, r) in self.room_list.iter().enumerate() {
            let row = (i + 2) as i32;
            let room_id = r.room_id.clone();
            let (players_color, join_label) = if r.max > r.players {
                (Color::argb(255, 0x6C, 0xCB, 0x5F), t!("common.join").to_string())
            } else {
                (Color::argb(255, 0xFF, 0x99, 0xA4), t!("page_room.full").to_string())
            };
            cells.push(KeyedView::new(format!("r{row}c0"), View::from(
                TextBlock::new()
                    .text(r.name.clone())
                    .grid_row(row)
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    )));
            cells.push(KeyedView::new(format!("r{row}c1"), View::from(
                TextBlock::new()
                    .text(r.game.clone())
                    .font_size(12.0)
                    .opacity(0.8)
                    .grid_row(row)
                    .grid_column(1)
                    .vertical_alignment(VerticalAlignment::Center)
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    )));
            cells.push(KeyedView::new(format!("r{row}c2"), View::from(
                TextBlock::new()
                    .text(format!("{}/{}", r.players, r.max))
                    .font_size(12.0)
                    .foreground(Brush::Solid(players_color))
                    .grid_row(row)
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center)
                    )));
            cells.push(KeyedView::new(format!("r{row}c3"), View::from(
                Button::new()
                    .is_enabled(idle && r.max > r.players)
                    .on_click(context.message(Message::RoomJoinDirect(room_id)))
                    .grid_row(row)
                    .grid_column(3)
                    .content(join_label)
                    )));
        }

        let list_body: View = if n == 0 {
            StackPanel::new()
                .horizontal_alignment(HorizontalAlignment::Center)
                .spacing(8.0)
                .margin(Thickness::new(0.0, 24.0, 0.0, 24.0))
                .children((
                    FontIcon::new()
                        .glyph("\u{E70A}")
                        .opacity(0.4),
                    TextBlock::new()
                        .text(t!("page_room.no_public_rooms_yet"))
                        .opacity(0.6),
                ))
                .into()
        } else {
            Grid::new()
                .columns([
                    GridLength::Star(1.2),
                    GridLength::Star(1.4),
                    GridLength::Pixel(56.0),
                    GridLength::Pixel(96.0),
                ])
                .rows(row_defs)
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .keyed_children(cells)
                .into()
        };

        let list_card = Border::new()
            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .padding(Thickness::new(16.0, 8.0, 16.0, 12.0))
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .children((
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::List)),
                            &t!("page_room.public_rooms"),
                            View::from(
                                StackPanel::new()
                                    .orientation(Orientation::Horizontal)
                                    .spacing(8.0)
                                    .children((
                                        ComboBox::new()
                                            .width(280.0)
                                            .items_source({
                                                let mut items = vec![t!("page_room.all_games").to_string()];
                                                items.extend(crate::db::room_game_names());
                                                items
                                            })
                                            .selected_index(
                                                self.room_filter_idx.map(|i| i + 1),
                                            )
                                            .placeholder_text(t!("page_room.all_games"))
                                            .on_selection_changed(
                                                context.callback(Message::RoomFilterChanged),
                                            )
                                            .vertical_alignment(VerticalAlignment::Center),
                                        Button::new()
                                            .style(ButtonStyle::Subtle)
                                            .is_enabled(!busy)
                                            .on_click(context.message(Message::RoomRefreshList))
                                            .content(t!("page_room.refresh").to_string()),
                                    )),
                            ),
                        ),
                        list_body,
                    )),
            );

        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
                    .spacing(4.0)
                    .children((
                        self.room_header(context, header_buttons),
                        busy_hint,
                        list_card,
                    )),
            )
            .into()
    }

    fn room_members_table(&self, is_host: bool) -> View {

        let mcols = [
            GridLength::Star(1.0),
            GridLength::Star(1.0),
            GridLength::Star(1.0),
            GridLength::Star(1.0),
            GridLength::Star(1.0),
            GridLength::Star(1.0),
            GridLength::Star(1.0),
        ];
        let mut mcells: Vec<KeyedView> = Vec::new();
        let mheaders = [
            t!("page_room.seat").to_string(),
            t!("page_room.member").to_string(),
            t!("page_room.address").to_string(),
            t!("common.status").to_string(),
            t!("page_room.play").to_string(),
            t!("page_room.ping").to_string(),
            t!("page_room.link").to_string(),
        ];
        for (c, txt) in mheaders.iter().enumerate() {
            mcells.push(KeyedView::new(
                format!("mh{c}"),
                View::from(
                    TextBlock::new()
                        .text(txt.clone())
                        .font_size(12.0)
                        .opacity(0.6)
                        .margin(Thickness::new(0.0, 4.0, 0.0, 4.0))
                        .grid_row(0)
                        .grid_column(c as i32)
                        .vertical_alignment(VerticalAlignment::Center),
                ),
            ));
        }
        let self_ip = self.room_my_ip.clone().unwrap_or_default();

        let self_seat = if is_host {
            "A1".to_string()
        } else {
            seat_label(self.room_my_ip.as_deref(), "")
        };

        mcells.push(KeyedView::new(
            "msep",
            View::from(
                Border::new()
                    .height(1.0)
                    .margin(Thickness::new(0.0, 4.0, 0.0, 4.0))
                    .background(ThemeBrush::CardStroke)
                    .grid_row(1)
                    .grid_column(0)
                    .grid_column_span(7),
            ),
        ));

        let playing_map: HashMap<String, bool> = if is_host {
            self.room_host_sync
                .as_ref()
                .map(|s| s.member_playing())
                .unwrap_or_default()
        } else {
            match &self.room_sync_table {
                Some(t) => t
                    .members
                    .iter()
                    .map(|m| (m.peer_id.clone(), m.playing))
                    .chain(std::iter::once((t.host_peer_id.clone(), t.host_playing)))
                    .collect(),
                None => HashMap::new(),
            }
        };

        let self_nick = {
            let n = self.room_nickname();
            if n.is_empty() {
                ROOM_DEFAULT_NICKNAME.to_string()
            } else {
                n
            }
        };
        let self_name = if is_host {
            t!("page_room.self_nick_host_this", self_nick = self_nick).to_string()
        } else {
            t!("page_room.self_nick_this_pc", self_nick = self_nick).to_string()
        };

        struct MemberRow {
            octet: u8,
            seat: String,
            name: String,
            ip: String,
            connected: bool,
            playing: String,
            ping: String,
            link: String,
        }
        let mut rows = vec![MemberRow {
            octet: self_ip
                .rsplit('.')
                .next()
                .and_then(|s| s.parse::<u8>().ok())
                .unwrap_or(u8::MAX),
            seat: self_seat,
            name: self_name,
            ip: self_ip,
            connected: true,
            playing: playing_label(Some(self.game_running)),
            ping: "-".to_string(),
            link: t!("page_room.local").to_string(),
        }];

        let nick_map: HashMap<String, String> = if is_host {
            self.room_host_sync
                .as_ref()
                .map(|s| s.member_nick())
                .unwrap_or_default()
        } else {
            match &self.room_sync_table {
                Some(t) => t
                    .members
                    .iter()
                    .filter_map(|m| m.nick.clone().map(|n| (m.peer_id.clone(), n)))
                    .chain(
                        t.host_nick
                            .clone()
                            .map(|n| (t.host_peer_id.clone(), n)),
                    )
                    .collect(),
                None => HashMap::new(),
            }
        };

        for p in self.room_peers.iter() {
            let alias = if p.alias.is_empty() { "?" } else { p.alias.as_str() };

            let is_host_row = self
                .room_seat_ips
                .get(&p.peer_id)
                .map(|ip| ip.ends_with(".101"))
                .unwrap_or(false)
                || alias == "host";
            let name = match nick_map.get(&p.peer_id) {
                Some(n) if !n.is_empty() => n.clone(),
                _ => {
                    if alias == "host" {
                        ROOM_DEFAULT_NICKNAME.to_string()
                    } else {
                        alias.to_string()
                    }
                }
            };
            let name = if is_host_row {
                t!("page_room.name_host", name = name).to_string()
            } else {
                name
            };
            let link = if p.through_relay {
                t!("page_room.relay").to_string()
            } else {
                t!("page_room.direct").to_string()
            };

            let seat = seat_label(
                self.room_seat_ips.get(&p.peer_id).map(String::as_str),
                &p.alias,
            );
            let octet = self
                .room_seat_ips
                .get(&p.peer_id)
                .map(String::as_str)
                .or(Some(p.ip.as_str()))
                .and_then(|ip| ip.rsplit('.').next().and_then(|s| s.parse::<u8>().ok()));
            rows.push(MemberRow {
                octet: octet.unwrap_or(u8::MAX),
                seat,
                name,
                ip: p.ip.clone(),
                connected: p.connected,
                playing: playing_label(playing_map.get(&p.peer_id).copied()),
                ping: format!("{:.0}ms", p.ping_ms),
                link,
            });
        }

        rows.sort_by_key(|r| r.octet);
        for (i, r) in rows.iter().enumerate() {
            mcells.extend(member_cells(
                (i + 2) as i32,
                &r.seat,
                &r.name,
                &r.ip,
                r.connected,
                r.playing.clone(),
                r.ping.clone(),
                r.link.clone(),
            ));
        }
        let mut mrow_defs = vec![GridLength::Auto];
        mrow_defs.push(GridLength::Pixel(1.0));
        for _ in 0..rows.len() {
            mrow_defs.push(GridLength::Auto);
        }
        let members_table = Grid::new()
            .columns(mcols)
            .rows(mrow_defs)
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .keyed_children(mcells);

        members_table
    }

    fn room_active_view(&self, context: &mut ViewContext<Self>) -> View {
        let (is_host, room_id) = match &self.room_status {
            RoomStatus::Hosting { room_id, .. } => (true, room_id.clone()),
            RoomStatus::Joined { room_id, .. } => (false, room_id.clone()),
            _ => return StackPanel::new().into(),
        };

        let room_pw = if is_host {
            self.room_password.clone()
        } else {
            self.room
                .last_room_password
                .clone()
                .unwrap_or_else(|| self.room_join_password.clone())
        };
        let pw_display: String = if room_pw.trim().is_empty() {
            t!("page_room.none").to_string()
        } else {
            room_pw
        };

        let has_serverbox_addr = self.room_uses_serverbox();
        let has_serverbox = self.room_has_serverbox();

        let (status_text, sr, sg, sb) = match &self.room_status {
            RoomStatus::Hosting { .. } => {
                (t!("page_room.room_open").to_string(), 0x6C, 0xCB, 0x5F)
            }
            RoomStatus::Joined { .. } => {
                (t!("page_room.joined").to_string(), 0x6C, 0xCB, 0x5F)
            }
            _ => (t!("page_room.working").to_string(), 0x9E, 0x9E, 0x9E),
        };

        let members_table = self.room_members_table(is_host);

        let members_card = Border::new()
            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .padding(Thickness::new(16.0, 12.0, 16.0, 12.0))
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .spacing(8.0)
                    .children((
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::Contact)),
                            &t!("page_room.members"),
                            View::from(
                                TextBlock::new()
                                    .text(format!("● {status_text}"))
                                    .foreground(Brush::Solid(Color::argb(255, sr, sg, sb)))
                                    .vertical_alignment(VerticalAlignment::Center),
                            ),
                        ),
                        separator(),
                        members_table,
                    )),
            );

        let info_card = Border::new()
            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .children((
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::World)),
                            &t!("common.room_id"),
                            View::from(
                                selectable_text(room_id.clone())
                                    .font_size(18.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .vertical_alignment(VerticalAlignment::Center),
                            ),
                        ),
                        separator(),
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::Permissions)),
                            &t!("page_room.room_password"),
                            View::from(
                                selectable_text(pw_display)
                                    .font_size(16.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .vertical_alignment(VerticalAlignment::Center),
                            ),
                        ),
                        separator(),
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::List)),
                            &t!("page_room.game"),
                            View::from(
                                selectable_text(self.room_game.clone())
                                    .opacity(0.85)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .text_trimming(TextTrimming::CharacterEllipsis),
                            ),
                        ),
                        separator(),
                        settings_row(
                            View::from(SymbolIcon::new().symbol(Symbol::Play)),
                            &t!("page_room.game_address"),
                            View::from(
                                selectable_text(self.room_my_ip.clone().unwrap_or_default())
                                    .font_size(16.0)
                                    .font_weight(FontWeight::SEMI_BOLD)
                                    .vertical_alignment(VerticalAlignment::Center),
                            ),
                        ),

                        if has_serverbox_addr {
                            separator()
                        } else {
                            StackPanel::new().into()
                        },
                        if has_serverbox_addr {
                            settings_row(
                                View::from(SymbolIcon::new().symbol(Symbol::MapDrive)),
                                &t!("page_room.serverbox_address"),
                                View::from(
                                    selectable_text(crate::awl::HOST_SERVERBOX_IP.to_string())
                                        .font_size(16.0)
                                        .font_weight(FontWeight::SEMI_BOLD)
                                        .vertical_alignment(VerticalAlignment::Center),
                                ),
                            )
                        } else {
                            StackPanel::new().into()
                        },
                    )),
            );

        let room_index = self.room_game_index();

        let game_running_here = self.game_running
            && room_index.is_some()
            && self.game_runtime.as_ref().map(|r| r.index) == room_index;
        let sb_running_here = room_index.is_some()
            && self.serverbox_runtime.as_ref().map(|r| r.index) == room_index;
        let mut actions: Vec<KeyedView> = Vec::new();
        actions.push(KeyedView::new(
            "launch-game",
            if game_running_here {
                View::from(
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .on_click(context.message(Message::LaunchStop(
                            room_index.unwrap_or_default(),
                        )))
                        .content(t!("common.close").to_string()),
                )
            } else {
                View::from(
                    Button::new()
                        .style(ButtonStyle::Accent)
                        .on_click(context.message(Message::RoomLaunchGame))
                        .content(t!("common.launch_game").to_string()),
                )
            },
        ));

        if is_host && has_serverbox {
            actions.push(KeyedView::new(
                "launch-serverbox",
                if sb_running_here {
                    View::from(
                        Button::new()
                            .on_click(context.message(Message::ServerBoxStop(
                                room_index.unwrap_or_default(),
                            )))
                            .content(t!("common.close").to_string()),
                    )
                } else {
                    View::from(
                        Button::new()
                            .on_click(context.message(Message::RoomLaunchServerBox))
                            .content(t!("page_room.launch_serverbox").to_string()),
                    )
                },
            ));
        }
        actions.push(KeyedView::new(
            "room-stop",
            if is_host {
                View::from(
                    Button::new()
                        .on_click(context.message(Message::RoomCloseClicked))
                        .content(t!("page_room.close_room").to_string()),
                )
            } else {
                View::from(
                    Button::new()
                        .on_click(context.message(Message::RoomLeaveClicked))
                        .content(t!("page_room.leave_room").to_string()),
                )
            },
        ));
        let action: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .keyed_children(actions)
            .into();

        let title = self.room_name.clone();

        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
                    .spacing(4.0)
                    .children((
                        Grid::new()
                            .columns([GridLength::Star(1.0), GridLength::Auto])
                            .children((
                                StackPanel::new().spacing(4.0).grid_column(0).children((
                                    TextBlock::new()
                                        .text(title)
                                        .font_size(26.0)
                                        .font_weight(FontWeight::SEMI_BOLD),
                                    TextBlock::new()
                                        .text(if is_host {
                                            t!("page_room.share_room_id")
                                            .to_string()
                                        } else {
                                            t!("page_room.joined_room")
                                                .to_string()
                                        })
                                        .font_size(12.0)
                                        .opacity(0.6),
                                )),
                                StackPanel::new()
                                    .orientation(Orientation::Horizontal)
                                    .spacing(8.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .grid_column(1)
                                    .children((action,)),
                            )),
                        info_card,
                        members_card,
                    )),
            )
            .into()
    }
}

#[cfg(test)]
mod tests {
    use super::seat_name_of_octet;

    #[test]
    fn seat_name_of_octet_mapping() {

        assert_eq!(seat_name_of_octet(101), Some("A1"));
        assert_eq!(seat_name_of_octet(103), Some("A2"));
        assert_eq!(seat_name_of_octet(104), Some("B1"));
        assert_eq!(seat_name_of_octet(105), Some("B2"));
        assert_eq!(seat_name_of_octet(102), None);
        assert_eq!(seat_name_of_octet(0), None);
        assert_eq!(seat_name_of_octet(255), None);
    }
}
