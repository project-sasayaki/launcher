mod message;
mod pages;
mod state;
mod theme;
mod widgets;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::path::PathBuf;
use std::process::Child;
use std::sync::{Arc, Mutex, atomic::{AtomicBool, AtomicUsize, Ordering}};
use std::time::{Duration, Instant};

use windows_reactor::*;

use crate::cards::{self, CardScan};
use crate::awl::{AwlHandle, PeerView, RoomStatus};
use crate::filedialog;
use crate::i18n::{self, Lang};
use crate::loaders;
use crate::netinfo::{self, AdapterInfo};
use crate::rawinput;
use crate::roomdir::RoomSummary;
use crate::server::{self, ServerState};
use crate::storage::{self, Library, Settings, VpnConfig};
use rust_i18n::t;
use crate::update::{HttpAbort, UpdatePhase, UpdateState, UpdateStatus, UpdateTarget};
use crate::vpn::{VpnHandle, VpnStatus};

use self::message::{Dialog, Message};
use self::widgets::spawn_anim_tick;
use self::pages::graphics::{GFX_AF_LEVELS, GFX_MSAA_LEVELS, GFX_RENDER_SCALE_LEVELS, GFX_RESOLUTIONS};
use self::pages::input::{FFB_MAX, INPUT_KINDS, INPUT_RATIO_DEFAULT, INPUT_RATIO_MAX, SHIFTER_TYPES};
use self::pages::network::{VPN_AUTO_RETRIES, VPN_STABLE_RESET};
use self::pages::system::{SYS_LOGLEVELS, SYS_SEATS};
use self::state::{CardBaseCache, CardImage, GameRuntime, PendingAdd, ServerBoxRuntime};
use self::theme::system_uses_light_theme;

const APP_ICON: &[u8] = include_bytes!("assets/launcher.ico");

pub struct Launcher {
    store: storage::Store,
    library: Library,
    settings: Settings,
    detected: loaders::LoaderPaths,
    server_state: ServerState,
    nav_tag: String,
    pane_open: bool,
    dialog: Dialog,
    pending: Option<PendingAdd>,

    adapters: Vec<AdapterInfo>,

    adapters_scanned: bool,

    net: storage::NetworkConfig,

    /// The keychip serial, the launcher's own: it also names this PC in the rooms.
    keychip: Option<String>,

    lang: Lang,

    /// The record id of the game whose settings page is open (or was last opened).
    settings_game: Option<String>,

    /// The tab shown on the game's settings page.
    settings_tab: pages::game_settings::SettingsTab,

    // The settings of `settings_game`, loaded from its own file when its settings page
    // opens and saved there on every change.
    system: storage::SystemConfig,

    coin: storage::CoinConfig,

    graphics: storage::GraphicsConfig,

    input: storage::InputConfig,

    wmmt3: crate::games::Wmmt3Settings,

    /// The DirectInput devices offered on a WMMT3 game's Input tab.
    wmmt3_devices: Vec<rawinput::GameDevice>,

    vpn: VpnConfig,

    vpn_status: VpnStatus,

    vpn_handle: Arc<Mutex<Option<VpnHandle>>>,

    vpn_auto_retries: u32,

    vpn_connected_at: Option<Instant>,

    vpn_watch_gen: Arc<AtomicUsize>,

    room: storage::RoomConfig,

    room_status: RoomStatus,

    room_handle: Arc<Mutex<Option<AwlHandle>>>,

    room_watchdog: Option<Child>,

    room_password: String,

    room_members_invited: Vec<String>,

    room_host_down: Option<Instant>,

    room_list: Vec<RoomSummary>,

    room_peers: Vec<PeerView>,

    room_seat_ips: HashMap<String, String>,

    room_my_ip: Option<String>,

    room_saw_self_in_table: bool,

    room_nick_pending: Option<crate::reactor_app::message::PendingRoomAction>,

    room_game: String,

    room_name: String,

    room_keychip: String,

    room_member_down: HashMap<String, Instant>,

    room_member_ever_connected: HashSet<String>,

    room_last_heartbeat: Option<Instant>,

    room_host_sync: Option<crate::room_sync::HostSync>,

    room_member_sync: Option<crate::room_sync::MemberSync>,

    room_sync_version: u64,

    room_sync_table: Option<crate::room_sync::SyncTable>,

    room_nick_override: Option<String>,

    room_dir_members: Option<crate::roomdir::MembersInfo>,

    room_last_reconcile: Option<Instant>,

    room_last_playing_report: Option<bool>,

    room_last_status_sent: Option<Instant>,

    room_overlay: Option<String>,

    room_filter_idx: Option<usize>,

    room_filter_game_id: Option<String>,

    room_join_password: String,

    input_devices: Vec<rawinput::GameDevice>,

    input_devices_scanned: bool,

    input_capture_gen: u32,

    input_capture_start: Option<Instant>,

    input_capture_remain: f64,

    coin_expanded: bool,

    coin_anim: f64,

    coin_anim_gen: u32,

    save_debounce_gen: u32,

    pending_saves: u32,

    ffb_expanded: bool,

    ffb_anim: f64,

    ffb_anim_gen: u32,

    drop_index: Option<usize>,

    drop_open: bool,

    drop_anim: f64,

    drop_anim_gen: u32,

    light_theme: bool,

    cards_game: Option<usize>,

    card_kind: crate::games::CardKind,

    cards_loading: bool,

    cards_warm: usize,
    cards: CardScan,

    win_size: WindowSize,

    card_slide: f64,

    card_enter_seq: u32,

    dpi_scale: f64,

    dpi_image: ElementRef<Image>,

    dpi_placeholder: Arc<Vec<u8>>,

    card_image_cache: RefCell<HashMap<PathBuf, Result<CardImage, Arc<str>>>>,

    base_cache: RefCell<Option<CardBaseCache>>,

    update_shared: Arc<Mutex<UpdateStatus>>,

    poll_running: bool,

    update_cancel: HashMap<UpdateTarget, Arc<AtomicBool>>,

    update_abort: HashMap<UpdateTarget, Arc<HttpAbort>>,

    game_running: bool,

    game_runtime: Option<GameRuntime>,

    serverbox_runtime: Option<ServerBoxRuntime>,

    launch_overlay: Option<usize>,

    update_prompt_key: Option<String>,

    window_icon: Option<&'static str>,

    hovered: Option<usize>,
}

impl Component for Launcher {
    type Message = Message;
    type Input = ();

    fn create(_input: &(), context: &ComponentContext<Self>) -> Self {
        let store = storage::Store::new();
        let library = store.load_library();
        let settings = store.load_settings();
        let net = store.load_network();
        let keychip = store.load_system().keychip;
        // Settings config.ini kept for every game move into each game's own file.
        let moved = store.move_shared_settings(&library.games);
        let vpn = store.load_vpn();
        let room = store.load_room();
        let lang = store.load_lang();
        i18n::set_lang(lang);
        let detected = loaders::detect_loaders();

        let server_host0 = net.server_host().to_string();
        context.spawn_background(move |_| {
            std::thread::sleep(Duration::from_secs(2));
            Message::ServerPolled(server::probe_now(&server_host0))
        });

        context.spawn_background(|_| Message::AdaptersScanned(Box::new(netinfo::list_adapters())));

        let mut launcher = Launcher {
            store,
            library,
            settings,
            detected,
            server_state: ServerState::Unknown,
            nav_tag: "games".to_string(),
            pane_open: true,
            dialog: Dialog::None,
            pending: None,
            adapters: Vec::new(),
            adapters_scanned: false,
            net,
            keychip,
            lang,
            settings_game: None,
            settings_tab: pages::game_settings::SettingsTab::default(),
            system: storage::SystemConfig::default(),
            coin: storage::CoinConfig::default(),
            graphics: storage::GraphicsConfig::default(),
            input: storage::InputConfig::default(),
            wmmt3: crate::games::Wmmt3Settings::default(),
            wmmt3_devices: Vec::new(),
            vpn,
            vpn_status: VpnStatus::NotConfigured,
            vpn_handle: Arc::new(Mutex::new(None)),
            vpn_auto_retries: 0,
            vpn_connected_at: None,
            vpn_watch_gen: Arc::new(AtomicUsize::new(0)),
            room,
            room_status: RoomStatus::Idle,
            room_handle: Arc::new(Mutex::new(None)),
            room_watchdog: None,
            room_password: String::new(),
            room_members_invited: Vec::new(),
            room_host_down: None,
            room_list: Vec::new(),
            room_peers: Vec::new(),
            room_seat_ips: HashMap::new(),
            room_my_ip: None,
            room_saw_self_in_table: false,
            room_nick_pending: None,
            room_game: String::new(),
            room_name: String::new(),
            room_keychip: String::new(),
            room_member_down: HashMap::new(),
            room_member_ever_connected: HashSet::new(),
            room_last_heartbeat: None,
            room_host_sync: None,
            room_member_sync: None,
            room_sync_version: 0,
            room_sync_table: None,
            room_nick_override: None,
            room_dir_members: None,
            room_last_reconcile: None,
            room_last_playing_report: None,
            room_last_status_sent: None,
            room_overlay: None,
            room_filter_idx: None,
            room_filter_game_id: None,
            room_join_password: String::new(),
            input_devices: Vec::new(),
            input_devices_scanned: false,
            input_capture_gen: 0,
            input_capture_start: None,
            input_capture_remain: 0.0,
            coin_expanded: false,
            coin_anim: 0.0,
            coin_anim_gen: 0,
            save_debounce_gen: 0,
            pending_saves: 0,
            ffb_expanded: true,
            ffb_anim: 1.0,
            ffb_anim_gen: 0,
            drop_index: None,
            drop_open: false,
            drop_anim: 0.0,
            drop_anim_gen: 0,
            hovered: None,
            light_theme: system_uses_light_theme(),
            cards_game: None,
            card_kind: crate::games::CardKind::default(),
            cards_loading: false,
            cards_warm: 0,
            cards: CardScan::default(),
            card_slide: 0.0,
            card_enter_seq: 0,
            win_size: WindowSize::default(),
            dpi_scale: 1.0,
            dpi_image: ElementRef::new(),
            dpi_placeholder: {
                let mut buf = Cursor::new(Vec::new());
                image::RgbaImage::from_pixel(1, 1, image::Rgba([0, 0, 0, 0]))
                    .write_to(&mut buf, image::ImageFormat::Png)
                    .expect("encode 1x1 placeholder png");
                Arc::new(buf.into_inner())
            },
            card_image_cache: RefCell::new(HashMap::new()),
            base_cache: RefCell::new(None),
            update_shared: Arc::new(Mutex::new(UpdateStatus::default())),
            poll_running: false,
            update_cancel: HashMap::new(),
            update_abort: HashMap::new(),
            game_running: false,
            game_runtime: None,
            serverbox_runtime: None,
            launch_overlay: None,
            update_prompt_key: None,
            window_icon: Self::prepare_window_icon(),
        };

        launcher.report_moved_settings(moved);

        launcher.run_check(true, context);
        context.spawn_background(move |_| {
            std::thread::sleep(Duration::from_secs(3600));
            Message::AutoCheckTick
        });

        if launcher.vpn.config_path.is_some() {
            launcher.start_vpn_connect(context);
        }
        launcher
    }

    fn update(&mut self, message: Message, context: &ComponentContext<Self>) {

        let Some(message) = self.handle_ffb(message, context) else { return };
        let Some(message) = self.handle_network(message, context) else { return };
        let Some(message) = self.handle_system(message, context) else { return };
        let Some(message) = self.handle_graphics(message, context) else { return };
        let Some(message) = self.handle_input(message, context) else { return };
        let Some(message) = self.handle_wmmt3(message, context) else { return };
        let Some(message) = self.handle_game_settings(message, context) else { return };
        let Some(message) = self.handle_vpn(message, context) else { return };
        let Some(message) = self.handle_room(message, context) else { return };
        match message {

            Message::NavChanged(Some(tag)) => {
                self.nav_tag = tag;

                if self.nav_tag == "room" {
                    let keychip_missing = self
                        .keychip
                        .as_deref()
                        .map(|v| v.trim().is_empty())
                        .unwrap_or(true);
                    let has_rejoin_memory = self
                        .room
                        .last_room_id
                        .as_deref()
                        .map(|v| !v.trim().is_empty())
                        .unwrap_or(false);
                    if keychip_missing && !has_rejoin_memory && matches!(self.room_status, RoomStatus::Idle) {
                        self.dialog = Dialog::LaunchFailed {
                            message: t!("app.keychip_serial_is_not_configured").to_string(),
                            goto: None,
                        };
                        self.nav_tag = "settings".to_string();
                    } else {
                        self.room_refresh_list(context, false);
                        self.room_try_resume(context);
                    }
                }

                if self.nav_tag == "network" {
                    self.adapters_scanned = false;
                    context.spawn_background(|_| {
                        Message::AdaptersScanned(Box::new(netinfo::list_adapters()))
                    });
                }
            }
            Message::NavChanged(None) => {}
            Message::PaneOpenChanged(open) => self.pane_open = open,
            Message::AddClicked => {
                context.spawn_background(|_| {
                    let picked = filedialog::pick_open_file(
                        &[
                            (&t!("app.all_supported"), "*.exe;*.elf;ICF1;main"),
                            (&t!("app.executable"), "*.exe;*.elf;main"),
                            (&t!("app.icf1_file"), "ICF1"),
                            (&t!("common.all_files"), "*.*"),
                        ],
                        &t!("app.select_game_file"),
                    );
                    Message::FilePicked(picked)
                });
            }
            Message::FilePicked(None) => {}
            Message::FilePicked(Some(path)) => self.handle_file_picked(path, context),
            Message::Identified(boxed) => self.handle_identified(*boxed),
            Message::IcfIdentified(boxed) => self.handle_icf_identified(*boxed),
            Message::ServerPolled(state) => {
                self.server_state = state;

                let host = self.net.server_host().to_string();
                context.spawn_background(move |_| {
                    std::thread::sleep(server::PROBE_INTERVAL);
                    Message::ServerPolled(server::probe_now(&host))
                });
            }
            Message::GameRowDropToggled(index) => self.toggle_row_drop(index, context),
            Message::GameRowDropTick(epoch, step) => self.tick_row_drop(epoch, step, context),
            Message::GameRowHovered(index) => self.hovered = Some(index),
            Message::GameRowUnhovered(index) => {
                if self.hovered == Some(index) {
                    self.hovered = None;
                }
            }

            Message::CardsScanned(seq, scan) if self.is_current_card_page(seq) => {
                self.apply_cards_scanned(*scan, context);
            }
            Message::CardClicked(index) if self.nav_tag == "cards" => {
                if let Some(card) = self.cards.cards.get(index) {
                    self.select_card(card.name.clone());
                }
            }
            Message::CardsLoadTick(seq) if self.is_current_card_page(seq) => {
                self.warm_card_cache(context);
            }
            Message::AimeCardCreate
                if self.nav_tag == "cards" && self.card_kind == crate::games::CardKind::Aime =>
            {
                let Some(dir) = self.aime_device_dir() else {
                    self.dialog = Dialog::Error {
                        message: t!("app.amdaemon_exe_is_not_configured").to_string(),
                    };
                    return;
                };
                let seq = self.card_enter_seq;
                context.spawn_background(move |_| {
                    let result = cards::create_aime_card_in(&dir)
                        .map(|_| ())
                        .map_err(|error| error.to_string());
                    Message::AimeCardCreated(seq, result)
                });
            }
            Message::AimeCardCreated(seq, result) if self.is_current_card_page(seq) => match result {
                Ok(()) => self.reload_aime_cards(),
                Err(error) => {
                    self.dialog = Dialog::Error {
                        message: t!("app.failed_to_create_card", error = error).to_string(),
                    };
                }
            },
            Message::BlankCardSelect if self.nav_tag == "cards" => {
                self.select_blank_card();
            }
            Message::BackToGames => {
                self.nav_tag = "games".to_string();
                self.cards_game = None;
                self.card_kind = crate::games::CardKind::default();
                self.cards_loading = false;
                self.cards_warm = 0;
            }
            Message::CardSlideTick(t) => {
                self.card_slide = t;

                if t < 1.0 {
                    let next = (t + 0.016 / 0.30).min(1.0);
                    spawn_anim_tick(context, move || Message::CardSlideTick(next));
                }
            }
            Message::WindowResized(size) => {
                self.win_size = size;
            }
            Message::RowTestModeToggled(index, on) => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.test_mode = Some(on);
                    self.store.save_library(&self.library);
                }
            }
            Message::GfxPatchToggled(index, on) => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.gfx_patch = Some(on);
                    self.store.save_library(&self.library);
                }
            }
            Message::InputReverseToggled(slot, on) => {
                match slot {
                    0 => self.input.reverse_accel_axis = Some(on),
                    1 => self.input.reverse_brake_axis = Some(on),
                    _ => {}
                }
                self.save_input();
            }

            Message::CheckUpdatesNow => self.run_check(true, context),
            Message::AutoCheckTick => {

                let _ = context.spawn_background(move |_| {
                    std::thread::sleep(Duration::from_secs(3600));
                    Message::AutoCheckTick
                });
                if self.settings.auto_check_updates {
                    self.run_check(false, context);
                }
            }
            Message::UpdatesChecked(res) => self.apply_updates_checked(res),
            Message::UpdateLoader(target) => self.start_loader_update(target, context),
            Message::CancelUpdate(target) => self.cancel_loader_update(target),
            Message::UpdateAllClicked => self.update_all_loaders(context),
            Message::UpdateFinished(target, result) => self.finish_update(target, result),
            Message::UpdatePoll => self.poll_update_progress(context),
            Message::AutoUpdateToggled(v) => {
                self.settings.auto_check_updates = v;
                self.store.save_settings(&self.settings);
            }

            _ => {}
        }
    }

    fn view(&self, _input: &(), context: &mut ViewContext<Self>) -> View {
        context.window_title(t!("app.window_title"));

        let dpi_image = self.dpi_image.clone();
        let sender = context.sender();
        context.use_effect("dpi-scale-observer", (), move || {
            let dpi_image = dpi_image.clone();
            let sender = sender.clone();
            let observation = dpi_image.observe_rasterization_scale(move |scale| {
                let _ = sender.send(Message::DpiScaleChanged(scale));
            });
            Some(Box::new(move || drop(observation)))
        });

        let mut visuals = WindowVisuals::new()
            .backdrop(WindowBackdrop::Mica)
            .theme(WindowTheme::System)
            .client_size(1180.0, 720.0);
        if let Some(icon) = self.window_icon {
            visuals = visuals.icon(icon);
        }
        context.window_visuals(visuals);

        context.on_window_size(context.callback(Message::WindowResized));

        let tag = self.nav_tag.clone();
        let nav_item = |t: &'static str, label: &str, icon: Symbol| {
            let selected = tag == t;
            KeyedView::new(
                t,
                NavigationViewItem::new()
                    .tag(t)
                    .is_selected(selected)
                    .slots([
                        SlotView::new(NavigationViewItemSlot::Content, label.to_string()),
                        SlotView::new(NavigationViewItemSlot::Icon, SymbolIcon::new().symbol(icon)),
                    ]),
            )
        };
        let menu_items = vec![
            nav_item("games", &t!("nav.games"), Symbol::Play),
            nav_item("room", &t!("nav.in_store_battles"), Symbol::Shop),
            nav_item("network", &t!("nav.network"), Symbol::Globe),
            nav_item("vpn", &t!("nav.vpn"), Symbol::Permissions),
            nav_item("settings", &t!("nav.settings"), Symbol::Repair),
        ];

        let content: View = match self.nav_tag.as_str() {
            "cards" => self.cards_page(context),
            "game_settings" => self.game_settings_page(context),
            "network" => self.network_page(context),
            "room" => self.room_page(context),
            "vpn" => self.vpn_page(context),
            "settings" => self.settings_page(context),
            _ => self.games_page(context),
        };

        let pane_open = self.pane_open;
        let navigation = NavigationView::new()
            .pane_display_mode(NavigationViewPaneDisplayMode::Left)
            .open_pane_length(170.0)
            .is_pane_open(pane_open)
            .on_is_pane_open_changed(context.callback(Message::PaneOpenChanged))
            .is_pane_toggle_button_visible(true)
            .is_back_button_visible(NavigationViewBackButtonVisible::Collapsed)
            .is_settings_visible(false)
            .always_show_header(false)
            .pane_title(t!("common.app_name"))
            .on_selected_tag_changed(
                context.callback(|opt: Option<String>| Message::NavChanged(opt)),
            )
            .grid_row(1)
            .slots([
                SlotView::collection(NavigationViewSlot::MenuItems, menu_items),
                SlotView::new(
                    NavigationViewSlot::Content,

                    Border::new()
                        .border_brush(ThemeBrush::CardStroke)
                        .border_thickness(Thickness::new(0.0, 0.0, 1.0, 0.0))
                        .content(content),
                ),
            ]);

        let dialog_view = self.dialog_view(context);

        Grid::new()
            .rows([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .children((

                self.top_bar(context),

                navigation,

                self.status_bar(),

                dialog_view,

                Image::new()
                    .source_data(EncodedImage::new((*self.dpi_placeholder).clone()))
                    .element_ref(&self.dpi_image)
                    .width(1.0)
                    .height(1.0)
                    .opacity(0.0)
                    .grid_row(0),

                self.launch_overlay_view(context),

                self.room_overlay_view(),
            ))
    }
}

impl Launcher {

    fn handle_ffb(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::FfbExpanderToggled => self.toggle_ffb_expander(context),
            Message::FfbAnimTick(epoch, step) => self.tick_ffb_anim(epoch, step, context),
            Message::FfbEnableToggled(on) => {
                self.input.ffb_enable = Some(on);
                self.save_input();
            }
            Message::FfbParamChanged(idx, v) => {
                let value = v.round().clamp(0.0, FFB_MAX) as u32;
                match idx {
                    0 => self.input.ffb_constant_force = Some(value),
                    1 => self.input.ffb_damper = Some(value),
                    2 => self.input.ffb_rumble = Some(value),
                    3 => self.input.ffb_rumble_duration = Some(value),
                    4 => self.input.ffb_base_damper = Some(value),
                    5 => self.input.ffb_dead_band = Some(value),
                    _ => {}
                }
                self.queue_save(Self::SAVE_INPUT, context);
            }
            Message::RowMenu { index, action } => self.handle_row_menu(index, action, context),
            Message::CompanionPicked {
                kind,
                index,
                path: Some(path),
            } => self.handle_companion_picked(kind, index, path, context),
            Message::CompanionPicked { path: None, .. } => {}
            Message::CompanionIdentified {
                kind,
                index,
                path,
                result,
            } => self.handle_companion_identified(kind, index, path, result),
            Message::NamingTextChanged(name) => {
                if let Dialog::Naming { name: slot } = &mut self.dialog {
                    *slot = name;
                }
            }
            Message::GameRemoveClicked(index) => {
                if let Some(g) = self.library.games.get(index) {
                    self.dialog = Dialog::ConfirmRemove {
                        index,
                        name: g.name.clone(),
                    };
                }
            }
            Message::DialogClosed(ContentDialogResult::Primary) => self.apply_dialog(context),
            Message::DialogClosed(_) => {

                if matches!(self.dialog, Dialog::InputCapture { .. }) {
                    self.end_capture();
                }

                if let Dialog::RoomNickPrompt { pending } =
                    std::mem::replace(&mut self.dialog, Dialog::None)
                {
                    self.room_nick_override =
                        Some(crate::reactor_app::pages::room::ROOM_DEFAULT_NICKNAME.to_string());
                    match pending {
                        crate::reactor_app::message::PendingRoomAction::Create {
                            name,
                            game_idx,
                            password,
                        } => self.room_start_host(name, game_idx, password, context),
                        crate::reactor_app::message::PendingRoomAction::Join {
                            room_id,
                            password,
                        } => self.room_start_join(room_id, password, context),
                    }
                } else {
                    self.dialog = Dialog::None;
                }
                self.pending = None;
            }
            Message::DpiScaleChanged(s) => self.dpi_scale = s,
            Message::CardComposeFailed(summary) => {
                self.dialog = Dialog::Error { message: summary };
            }
            other => return Some(other),
        };
        None
    }

    fn handle_network(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::RefreshAdapters => {
                self.adapters_scanned = false;
                context.spawn_background(|_| {
                    Message::AdaptersScanned(Box::new(netinfo::list_adapters()))
                });
            }
            Message::AdaptersScanned(list) => {
                self.adapters = *list;
                self.adapters_scanned = true;
            }

            Message::AdapterSelected(Some(index)) => {
                if let Some(a) = self.adapters.get(index) {

                    self.net.adapter = Some(a.guid.clone());
                    self.net.ip = netinfo::subnet_labels(&a.ips).first().cloned();

                    if a.friendly == crate::awl::AWL_IFACE_NAME {
                        self.net.host_ip = self
                            .room_my_ip
                            .clone()
                            .or_else(|| a.ips.first().map(|ip| ip.ip.clone()));
                        self.net.serverbox_ip = Some(crate::awl::HOST_SERVERBOX_IP.to_string());
                    } else {

                        self.net.host_ip = a.ips.first().map(|ip| ip.ip.clone());
                        self.net.serverbox_ip = match a.ips.get(1) {
                            Some(second) => Some(second.ip.clone()),
                            None => a.ips.first().and_then(|ip| netinfo::next_ip(&ip.ip)),
                        };
                    }
                    self.save_net();
                }
            }
            Message::AdapterSelected(None) => {}
            Message::IpSelected(Some(index)) => {
                if let Some(a) = self.selected_adapter() {
                    let labels = netinfo::subnet_labels(&a.ips);
                    if let Some(cidr) = labels.get(index) {
                        self.net.ip = Some(cidr.clone());
                        self.save_net();
                    }
                }
            }
            Message::IpSelected(None) => {}
            Message::NetworkSimulateToggled(on) => {
                self.net.simulate = Some(on);
                self.save_net();
            }
            Message::ServerHostChanged(text) => {
                let t = text.trim().to_string();
                self.net.server = if t.is_empty() { None } else { Some(t) };

                self.queue_save(Self::SAVE_NET, context);

            }
            Message::RoomNickChanged(text) => {

                let t: String = text.trim().chars().take(24).collect();
                self.room.nickname = if t.is_empty() { None } else { Some(t) };
                let _ = self.store.save_room(&self.room);
            }
            Message::ServerboxIpChanged(text) => {
                let t = text.trim().to_string();

                if self.net.simulate == Some(false) && !self.awl_adapter_selected() {
                    self.net.serverbox_ip = if t.is_empty() { None } else { Some(t) };
                    self.queue_save(Self::SAVE_NET, context);
                }
            }
            Message::HostIpChanged(text) => {

                if !self.awl_adapter_selected() {
                    let t = text.trim().to_string();
                    self.net.host_ip = if t.is_empty() { None } else { Some(t) };
                    self.queue_save(Self::SAVE_NET, context);
                }
            }
            Message::StaticIpRemoved(result) => {

                if let Err(e) = result {
                    self.dialog = Dialog::Error {
                        message: t!("app.failed_to_remove_static_ip", error = e).to_string(),
                    };
                }
            }
            other => return Some(other),
        };
        None
    }

    fn handle_system(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::LogLevelSelected(Some(index)) => {
                self.system.loglevel = SYS_LOGLEVELS
                    .get(index)
                    .and_then(|(v, _)| v.map(String::from));
                self.save_system();
            }
            Message::LogLevelSelected(None) => {}

            Message::LangSelected(Some(index)) => {
                let Some(&new_lang) = Lang::ALL.get(index) else { return None };
                if new_lang != self.lang {
                    self.lang = new_lang;
                    i18n::set_lang(new_lang);
                    if let Err(e) = self.store.save_lang(new_lang) {
                        self.dialog = Dialog::Error {
                            message: format!("{}: {e}", t!("app.failed_to_save_language")),
                        };
                    }
                }
            }
            Message::LangSelected(None) => {}
            Message::GameRegionSelected(index, sel) => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.region = sel.and_then(|i| pages::games::GAME_REGIONS.get(i))
                        .map(|v| v.to_string());
                    self.store.save_library(&self.library);
                }
            }
            Message::KeychipChanged(text) => {
                let t = text.trim().to_string();
                self.keychip = (!t.is_empty()).then_some(t);
                self.queue_save(Self::SAVE_KEYCHIP, context);
            }
            Message::SaveDebounceTick(token) => self.flush_pending_saves(token),
            Message::SeatSelected(Some(index)) => {
                if let Some((v, _)) = SYS_SEATS.get(index) {
                    self.system.seat = Some(v.to_string());

                    if *v == "SINGLE" {
                        self.system.master = Some(true);
                    }
                    self.save_system();
                }
            }
            Message::SeatSelected(None) => {}
            Message::SeatMasterToggled(on) => {
                self.system.master = Some(on);
                self.save_system();
            }
            Message::SkipReaderInitToggled(on) => {
                self.system.skip_reader_init = Some(on);
                self.save_system();
            }
            Message::AutoInsertCardToggled(on) => {
                self.system.auto_insert_card = Some(on);
                self.save_system();
            }

            Message::CoinExpanderToggled => self.toggle_coin_expander(context),
            Message::CoinAnimTick(epoch, step) => self.tick_coin_anim(epoch, step, context),
            Message::FreeplayToggled(on) => {
                self.coin.freeplay = Some(on);
                self.save_coin();
            }
            Message::CoinValueChanged(field, value) => {

                let v = value.map(|f| f as u32);
                match field {
                    0 => self.coin.start_coin = v,
                    1 => self.coin.continue_coin = v,
                    2 => self.coin.card_coin = v,
                    _ => self.coin.car_coin = v,
                }
                self.save_coin();
            }
            other => return Some(other),
        };
        None
    }

    fn handle_graphics(&mut self, message: Message, _context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::GraphicsDefaultToggled(on) => {
                self.graphics.default = Some(on);
                self.save_graphics();
            }
            Message::GraphicsBorderlessToggled(on) => {
                self.graphics.borderless = Some(on);
                self.save_graphics();
            }
            Message::GraphicsResolutionSelected(Some(index)) => {
                self.graphics.resolution = GFX_RESOLUTIONS.get(index).map(|v| v.to_string());
                self.save_graphics();
            }
            Message::GraphicsResolutionSelected(None) => {}
            Message::GraphicsRenderScaleToggled(on) => {
                self.graphics.render_scale = Some(on);
                self.save_graphics();
            }
            Message::GraphicsRenderScaleLevelSelected(Some(index)) => {
                self.graphics.render_scale_level =
                    GFX_RENDER_SCALE_LEVELS.get(index).map(|v| v.to_string());
                self.save_graphics();
            }
            Message::GraphicsRenderScaleLevelSelected(None) => {}
            Message::GraphicsMsaaToggled(on) => {
                self.graphics.msaa = Some(on);
                self.save_graphics();
            }
            Message::GraphicsMsaaLevelSelected(Some(index)) => {
                self.graphics.msaa_level = GFX_MSAA_LEVELS.get(index).map(|v| v.to_string());
                self.save_graphics();
            }
            Message::GraphicsMsaaLevelSelected(None) => {}
            Message::GraphicsAfToggled(on) => {
                self.graphics.af = Some(on);
                self.save_graphics();
            }
            Message::GraphicsAfLevelSelected(Some(index)) => {
                self.graphics.af_level = GFX_AF_LEVELS.get(index).map(|v| v.to_string());
                self.save_graphics();
            }
            Message::GraphicsAfLevelSelected(None) => {}
            other => return Some(other),
        };
        None
    }

    fn handle_input(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::InputKindSelected(Some(index)) => {
                self.input.kind = INPUT_KINDS.get(index).map(|v| v.to_string());

                self.save_input();

                self.refresh_input_devices(context);
            }
            Message::InputKindSelected(None) => {}
            Message::InputRefreshDevices => self.refresh_input_devices(context),
            Message::InputRatioChanged(v) => {
                let value = v.round().clamp(0.0, INPUT_RATIO_MAX) as u32;
                self.input.ratio = Some(value);
                self.queue_save(Self::SAVE_INPUT, context);
            }
            Message::InputShifterTypeSelected(Some(index)) => {
                self.input.shifter_type = SHIFTER_TYPES.get(index).map(|v| v.to_string());
                self.save_input();
            }
            Message::InputShifterTypeSelected(None) => {}
            Message::InputDualStickToggled(b) => {
                self.input.dual_stick = Some(b);
                self.save_input();
            }
            Message::InputLinearSteeringToggled(b) => {
                self.input.linear_steering = Some(b);
                self.save_input();
            }
            Message::InputLeftDeadzoneChanged(v) => {
                let value = v.round().clamp(0.0, 100.0) as u32;
                self.input.left_deadzone = Some(value);
                self.queue_save(Self::SAVE_INPUT, context);
            }
            Message::InputRightDeadzoneChanged(v) => {
                let value = v.round().clamp(0.0, 100.0) as u32;
                self.input.right_deadzone = Some(value);
                self.queue_save(Self::SAVE_INPUT, context);
            }
            Message::InputDevicesScanned(list) => {
                self.input_devices = *list;
                self.input_devices_scanned = true;
            }
            Message::InputDeviceSelected(slot, Some(index)) => self.apply_device_selected(slot, index),
            Message::InputDeviceSelected(_, None) => {}
            Message::InputDeviceDiagnosticsOpen => {
                self.dialog = Dialog::DeviceDiagnostics {
                    text: rawinput::device_diagnostics_text(),
                };
            }
            Message::InputRatioReset => {
                self.input.ratio = Some(INPUT_RATIO_DEFAULT);
                self.save_input();
            }
            Message::InputCaptureStart(slot) => self.start_capture(slot, context),
            Message::InputCaptureTick(epoch) => self.tick_capture(epoch, context),
            Message::InputCaptured(epoch, slot, button) => self.apply_captured_button(epoch, slot, button),
            Message::InputCapturedAxis(epoch, slot, axis_name) => self.apply_captured_axis(epoch, slot, axis_name),
            Message::InputCapturedXi(epoch, slot, name) => self.apply_captured_xi_button(epoch, slot, name),
            Message::InputCaptureTimeout(epoch) => {
                if epoch != self.input_capture_gen {
                    return None;
                }
                self.capture_timed_out();
            }
            other => return Some(other),
        };
        None
    }

    fn handle_vpn(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::VpnImportClicked => {
                context.spawn_background(|_| {
                    let picked = filedialog::pick_open_file(
                        &[(&t!("app.openvpn_config"), "*.ovpn"), (&t!("common.all_files"), "*.*")],
                        &t!("app.select_vpn_config_file"),
                    );
                    Message::VpnConfigPicked(picked)
                });
            }
            Message::VpnConfigPicked(Some(path)) => {

                self.vpn_watch_gen.fetch_add(1, Ordering::Relaxed);
                self.vpn_auto_retries = 0;
                self.vpn_connected_at = None;
                if let Some(h) = self.vpn_handle.lock().unwrap().take() {
                    drop(h);
                }

                self.vpn.config_path = Some(path.display().to_string());
                if let Err(e) = self.store.save_vpn(&self.vpn) {
                    self.dialog = Dialog::Error {
                        message: t!("common.failed_to_save_config_ini", error = e).to_string(),
                    };
                }
                self.vpn_status = VpnStatus::Disconnected;
            }
            Message::VpnConfigPicked(None) => {}
            Message::VpnPassphraseChanged(text) => {
                let t = text;
                self.vpn.passphrase = if t.is_empty() {
                    None
                } else {
                    Some(t)
                };
                self.queue_save(Self::SAVE_VPN, context);
            }
            Message::VpnConnectClicked => self.start_vpn_connect(context),
            Message::VpnDisconnectClicked => {

                self.vpn_watch_gen.fetch_add(1, Ordering::Relaxed);
                self.vpn_auto_retries = 0;
                self.vpn_connected_at = None;
                if let Some(h) = self.vpn_handle.lock().unwrap().take() {
                    drop(h);
                }
                self.vpn_status = VpnStatus::Disconnected;
            }
            Message::VpnConnectResult(None) => {

                self.vpn_connected_at = Some(Instant::now());
                self.vpn_status = VpnStatus::Connected;

                self.spawn_vpn_watch(context);
            }
            Message::VpnConnectResult(Some(err)) => {
                if self.vpn_auto_retries > 0 {

                    if self.vpn_auto_retries < VPN_AUTO_RETRIES {
                        self.dialog = Dialog::Error {
                            message: t!("app.auto_reconnect_failed", attempt = self.vpn_auto_retries, max_attempts = VPN_AUTO_RETRIES, error = err).to_string(),
                        };
                        self.spawn_vpn_connect_task(context);
                    } else {
                        self.vpn_auto_retries = 0;
                        self.vpn_status = VpnStatus::Failed;
                        self.dialog = Dialog::Error {
                            message: t!("app.vpn_auto_reconnect_failed_after", max_attempts = VPN_AUTO_RETRIES, error = err).to_string(),
                        };
                    }
                } else {
                    self.vpn_status = VpnStatus::Failed;
                    self.dialog = Dialog::Error { message: err };
                }
            }
            Message::VpnDied(None) => {}
            Message::VpnDied(Some(reason)) => {

                if self.vpn_handle.lock().unwrap().take().is_none() {
                    return None;
                }

                if self.vpn_connected_at.is_some_and(|t| t.elapsed() >= VPN_STABLE_RESET) {
                    self.vpn_auto_retries = 0;
                }
                if self.vpn_auto_retries < VPN_AUTO_RETRIES {
                    self.vpn_auto_retries += 1;
                    self.vpn_status = VpnStatus::Connecting;
                    self.dialog = Dialog::Error {
                        message: t!("app.vpn_reconnecting", reason = reason, attempt = self.vpn_auto_retries, max_attempts = VPN_AUTO_RETRIES).to_string(),
                    };
                    self.spawn_vpn_connect_task(context);
                } else {
                    self.vpn_auto_retries = 0;
                    self.vpn_status = VpnStatus::Failed;
                    self.dialog = Dialog::Error {
                        message: t!("app.vpn_reconnect_failed", reason = reason).to_string(),
                    };
                }
            }
            other => return Some(other),
        };
        None
    }

    fn handle_room(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {

            Message::RoomFilterChanged(sel) => {

                let games_idx = match sel {
                    None | Some(0) => None,
                    Some(i) => Some(i - 1),
                };
                self.room_filter_idx = games_idx;
                self.room_filter_game_id = games_idx.and_then(|i| {
                    crate::db::room_games().get(i).map(|g| {
                        g.game_id.clone().unwrap_or_else(|| g.name.clone())
                    })
                });

                self.room_refresh_list(context, false);
            }
            Message::RoomCreateGameChanged(idx) => {
                if let Dialog::RoomCreate { game, .. } = &mut self.dialog {
                    *game = idx;
                }
            }
            Message::RoomCreateResult(res) => {
                self.room_overlay = None;
                self.room_on_created(res, context);
            }
            Message::RoomJoinResult(res) => {
                self.room_overlay = None;
                self.room_on_joined(res, context);
            }
            Message::RoomNickPromptReady => {

                if let Some(pending) = self.room_nick_pending.take() {
                    self.dialog = Dialog::RoomNickPrompt { pending };
                }
            }
            Message::RoomLeaveClicked | Message::RoomCloseClicked => self.room_stop(context),
            Message::RoomLaunchGame => self.room_launch(context, false),
            Message::RoomLaunchServerBox => self.room_launch(context, true),
            Message::RoomFinished(res) => {

                self.room_overlay = None;
                if let Err(e) = res {
                    self.dialog = Dialog::Error { message: e };
                }

                self.room_refresh_list(context, false);
            }
            Message::RoomTick => self.room_on_tick(context),
            Message::RoomRefreshList => self.room_refresh_list(context, true),
            Message::RoomResumeResult(res) => {
                self.room_overlay = None;
                match res {
                Ok(None) => {

                }
                Ok(Some(room)) => {
                    self.room_status = RoomStatus::Hosting { room_id: room.room_id.clone(), name: room.name.clone() };
                    self.room_name = room.name;
                    self.room_game = room.game;
                    self.room_my_ip = Some(crate::awl::HOST_GAME_IP.to_string());

                    self.room_password = self.room.last_room_password.clone().unwrap_or_default();
                    self.room_members_invited.clear();

                    self.room_dir_members = None;
                    self.room_last_reconcile = None;
                    self.room_last_playing_report = Some(self.game_running);
                    let host_sync = crate::room_sync::HostSync::start();
                    let host_peer = crate::awl::AwlManager::my_peer_id().unwrap_or_default();
                    let nick = self.room_nickname();
                    host_sync.publish(&host_peer, &[], self.game_running, Some(&nick));
                    self.room_host_sync = Some(host_sync);
                    self.room_schedule_tick(self.room_tick_interval(), context);
                    self.spawn_room_watchdog();
                }
                    Err(e) => self.dialog = Dialog::Error { message: e },
                }
            },
            Message::RoomOpenCreateDialog => {
                if self.room_status == RoomStatus::Idle {
                    self.dialog = Dialog::RoomCreate { name: String::new(), game: None, password: String::new() };
                }
            }
            Message::RoomCreateNameChanged(s) => {
                if let Dialog::RoomCreate { name, .. } = &mut self.dialog {
                    *name = s;
                }
            }
            Message::RoomCreatePasswordChanged(s) => {
                if let Dialog::RoomCreate { password, .. } = &mut self.dialog {
                    *password = s;
                }
            }
            Message::RoomOpenJoinDialog => {
                if self.room_status == RoomStatus::Idle {
                    self.dialog = Dialog::RoomJoin { room_id: String::new(), password: String::new() };
                }
            }
            Message::RoomJoinDirect(room_id) => {
                if self.room_status == RoomStatus::Idle {

                    self.room_start_join(room_id, String::new(), context)
                }
            }
            Message::RoomJoinIdChanged(s) => {
                if let Dialog::RoomJoin { room_id, .. } = &mut self.dialog {
                    *room_id = s;
                }
            }
            Message::RoomJoinPasswordChanged(s) => {
                if let Dialog::RoomJoin { password, .. } = &mut self.dialog {
                    *password = s;
                }
            }
            Message::RoomListRefreshed(res, from_overlay) => {

                if from_overlay {
                    self.room_overlay = None;
                }
                match res {
                    Ok(list) => self.room_list = list,
                    Err(e) => self.dialog = Dialog::Error { message: e },
                }
            }
            Message::LaunchDone { index, result, goto } => {
                self.handle_launch_done(index, result, goto)
            }
            Message::GameLaunched { runtime, .. } => {

                self.game_running = true;
                self.game_runtime = Some(runtime);
                Self::spawn_game_poll(context);

                self.run_check(true, context);
            }
            Message::GamePollTick => self.poll_game_exit(context),
            Message::ServerBoxLaunched {
                result,
                runtime,
                goto,
            } => {
                self.serverbox_runtime = runtime;
                match result {
                    Ok(()) => {}
                    Err(e) => {
                        self.dialog = Dialog::LaunchFailed {
                            message: e,
                            goto,
                        };
                    }
                }
            }
            Message::LaunchStop(index) => {

                let game_running_here =
                    self.game_running && self.game_runtime.as_ref().map(|r| r.index) == Some(index);
                let sb = self.serverbox_runtime.take();
                let sb_running_here = sb.as_ref().map(|r| r.index) == Some(index);
                if !game_running_here && !sb_running_here {
                    return None;
                }
                let rt = self.game_runtime.take();
                self.game_running = false;

                if game_running_here && self.launch_overlay == Some(index) {
                    self.launch_overlay = None;
                }

                if let Some(ip) = rt.as_ref().and_then(|r| r.static_ip.clone()) {
                    context.spawn_background(move |_| Message::StaticIpRemoved(ip.remove()));
                }
                context.spawn_background(move |_| {
                    if let Some(r) = &rt {

                        r.taskkill_images();
                        if let Ok(mut c) = r.child.lock() {
                            let _ = c.kill();
                        }
                        if let Ok(mut v) = r.companions.lock() {
                            for mut ch in v.drain(..) {
                                let _ = ch.kill();
                            }
                        }
                    }
                    if sb_running_here {
                        if let Some(s) = &sb {
                            if let Ok(mut c) = s.child.lock() {
                                let _ = c.kill();
                            }
                        }
                    }
                    Message::LaunchDone { index, result: Ok(()), goto: None }
                });
            }
            Message::ServerBoxStop(index) => {
                if self.serverbox_runtime.as_ref().map(|r| r.index) != Some(index) {
                    return None;
                }
                let sb = self.serverbox_runtime.take();
                if let Some(s) = &sb {
                    if let Ok(mut c) = s.child.lock() {
                        let _ = c.kill();
                    }
                }
            }
            other => return Some(other),
        };
        None
    }

    const SAVE_KEYCHIP: u32 = 1;
    const SAVE_NET: u32 = 1 << 1;
    const SAVE_INPUT: u32 = 1 << 2;
    const SAVE_VPN: u32 = 1 << 3;

    pub(in crate::reactor_app) fn queue_save(&mut self, kind: u32, context: &ComponentContext<Self>) {
        self.pending_saves |= kind;
        self.save_debounce_gen += 1;
        let token = self.save_debounce_gen;
        context.spawn_background(move |_| {
            std::thread::sleep(std::time::Duration::from_millis(500));
            Message::SaveDebounceTick(token)
        });
    }

    /// Saves at once the changes still waiting for their debounce, for a step that needs
    /// them on disk now (switching to another game's settings, launching a game).
    pub(in crate::reactor_app) fn save_pending_now(&mut self) {
        self.flush_pending_saves(self.save_debounce_gen);
    }

    pub(in crate::reactor_app) fn flush_pending_saves(&mut self, token: u32) {
        if token != self.save_debounce_gen {
            return;
        }
        let pending = std::mem::replace(&mut self.pending_saves, 0);
        if pending & Self::SAVE_KEYCHIP != 0 {
            self.save_keychip();
        }
        if pending & Self::SAVE_NET != 0 {
            self.save_net();
        }
        if pending & Self::SAVE_INPUT != 0 {
            self.save_input();
        }
        if pending & Self::SAVE_VPN != 0 {
            if let Err(e) = self.store.save_vpn(&self.vpn) {
                self.dialog = Dialog::Error {
                    message: t!("common.failed_to_save_config_ini", error = e).to_string(),
                };
            }
        }
    }

    fn prepare_window_icon() -> Option<&'static str> {
        let path = std::env::temp_dir().join("sasayaki_launcher.ico");
        let stale = match std::fs::read(&path) {
            Ok(bytes) => bytes != APP_ICON,
            Err(_) => true,
        };
        if stale && std::fs::write(&path, APP_ICON).is_err() {
            return None;
        }
        let path_str = path.to_str()?;
        Some(Box::leak(path_str.to_string().into_boxed_str()))
    }

    fn top_bar(&self, context: &mut ViewContext<Self>) -> View {
        Border::new()
            .grid_row(0)
            .margin(Thickness::new(16.0, 12.0, 16.0, 8.0))
            .content(
                Grid::new()
                    .columns([GridLength::Star(1.0), GridLength::Auto])
                    .children((
                        TextBlock::new()
                            .text(t!("common.app_name"))
                            .font_size(16.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .vertical_alignment(VerticalAlignment::Center)
                            .grid_column(0),
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .grid_column(1)
                            .children((

                                StackPanel::new()
                                    .orientation(Orientation::Horizontal)
                                    .spacing(6.0)
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .children((
                                        TextBlock::new()
                                            .text(t!("app.visit_dnet_website"))
                                            .font_size(14.0)
                                            .vertical_alignment(VerticalAlignment::Center)
                                            .opacity(0.8),
                                        HyperlinkButton::new()
                                            .navigate_uri("https://dnet.arcadehub.xyz")
                                            .expect("the homepage URI is valid")
                                            .content(
                                                TextBlock::new()
                                                    .text("https://dnet.arcadehub.xyz")
                                                    .font_size(14.0)
                                                    .foreground(ThemeBrush::Accent),
                                            ),
                                    )),
                                Button::new()
                                    .style(ButtonStyle::Accent)
                                    .on_click(context.message(Message::AddClicked))
                                    .content(t!("app.add_game").to_string()),
                            )),
                    )),
            )
    }

    fn status_bar(&self) -> View {
        let srv_color = match self.server_state {
            ServerState::Online => Color::argb(255, 0x6C, 0xCB, 0x5F),
            ServerState::Offline => Color::argb(255, 0xFF, 0x99, 0xA4),
            ServerState::Unknown => Color::argb(255, 0x9E, 0x9E, 0x9E),
        };
        let vpn_color = match self.vpn_status {
            VpnStatus::NotConfigured => Color::argb(255, 0x9E, 0x9E, 0x9E),
            VpnStatus::Disconnected => Color::argb(255, 0xFF, 0x99, 0xA4),
            VpnStatus::Connecting => Color::argb(255, 0x9E, 0x9E, 0x9E),
            VpnStatus::Connected => Color::argb(255, 0x6C, 0xCB, 0x5F),
            VpnStatus::Failed => Color::argb(255, 0xFF, 0x99, 0xA4),
        };

        let (upd_color, upd_label) = {
            let s = self.update_shared.lock().unwrap();
            let mut checking = false;
            let mut outdated = false;
            let mut error = false;
            let mut up_to_date = false;
            for ts in s.per_target.values() {
                match ts.state {
                    UpdateState::Checking => checking = true,
                    UpdateState::Outdated => outdated = true,
                    UpdateState::Error => error = true,
                    UpdateState::UpToDate => up_to_date = true,
                    UpdateState::NotConfigured => {}
                }
                if matches!(
                    ts.phase,
                    UpdatePhase::Checking
                        | UpdatePhase::Downloading
                        | UpdatePhase::Extracting
                        | UpdatePhase::Installing
                ) {
                    checking = true;
                }
            }
            if error {
                (
                    Color::argb(255, 0xFF, 0x99, 0xA4),
                    t!("app.update_error"),
                )
            } else if checking {
                (
                    Color::argb(255, 0xFF, 0xB0, 0x4D),
                    t!("app.checking"),
                )
            } else if outdated {
                (
                    Color::argb(255, 0xFF, 0xB0, 0x4D),
                    t!("common.update_available"),
                )
            } else if up_to_date {

                (
                    Color::argb(255, 0x6C, 0xCB, 0x5F),
                    t!("app.up_to_date"),
                )
            } else {

                (
                    Color::argb(255, 0x9E, 0x9E, 0x9E),
                    t!("app.not_checked"),
                )
            }
        };

        let light = |color: Color| -> View {
            Border::new()
                .width(11.0)
                .height(11.0)
                .corner_radius(5.5)
                .background(Brush::Solid(color))
                .into()
        };

        let item = |label: &str, color: Color| -> View {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(6.0)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(label.to_string())
                        .font_size(12.0)
                        .opacity(0.7),
                    light(color),
                ))
                .into()
        };

        Border::new()
            .grid_row(2)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::new(0.0, 1.0, 0.0, 0.0))
            .margin(Thickness::new(16.0, 0.0, 16.0, 4.0))
            .padding(Thickness::new(0.0, 6.0, 0.0, 6.0))
            .content(
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(20.0)
                    .children((
                        item(&t!("app.server"), srv_color),
                        item("VPN", vpn_color),
                        {
                            let (room_color, room_label) = match &self.room_status {
                                RoomStatus::Idle => (Color::argb(255, 0x9E, 0x9E, 0x9E), t!("app.room")),
                                RoomStatus::Creating { .. } | RoomStatus::Joining { .. } => {
                                    (Color::argb(255, 0x9E, 0x9E, 0x9E), t!("app.room_connecting"))
                                }
                                RoomStatus::Hosting { .. } | RoomStatus::Joined { .. } => {
                                    (Color::argb(255, 0x6C, 0xCB, 0x5F), t!("app.room"))
                                }
                            };
                            item(&room_label, room_color)
                        },
                        item(&upd_label, upd_color),
                    )),
            )
    }
}
