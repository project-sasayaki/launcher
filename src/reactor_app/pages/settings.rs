use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use windows_reactor::*;

use crate::i18n::Lang;
use rust_i18n::t;
use crate::update::{self, TargetStatus, UpdateIndex, UpdatePhase, UpdateState, UpdateTarget};

use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::widgets::{separator, settings_card, settings_row, settings_toggle};

const COLOR_GREY: Color = Color::argb(255, 0x9E, 0x9E, 0x9E);
const COLOR_OK: Color = Color::argb(255, 0x6C, 0xCB, 0x5F);
const COLOR_WARN: Color = Color::argb(255, 0xFF, 0xB0, 0x4D);
const COLOR_ERR: Color = Color::argb(255, 0xFF, 0x99, 0xA4);
const COLOR_ACCENT: Color = Color::argb(255, 0x4C, 0x8B, 0xF5);

impl Launcher {

    pub(in crate::reactor_app) fn run_check(&mut self, force: bool, context: &ComponentContext<Self>) {

        let queryable: Vec<UpdateTarget> = update::ALL_TARGETS
            .iter()
            .copied()
            .filter(|t| t.has_update_source())
            .collect();
        let to_query: Vec<UpdateTarget> = if force {
            queryable.clone()
        } else if let Some(idx) = update::load_cache() {
            let results = update::check_updates(&idx, &self.settings);
            update::apply_check(&self.update_shared, results);
            let missing: Vec<UpdateTarget> = queryable
                .iter()
                .copied()
                .filter(|t| !idx.loaders.iter().any(|l| l.name == t.dir_name()))
                .collect();
            if missing.is_empty() {
                return;
            }

            {
                let mut s = self.update_shared.lock().unwrap();
                for t in &missing {
                    if let Some(ts) = s.per_target.get_mut(t) {
                        ts.state = UpdateState::Checking;
                        ts.phase = UpdatePhase::Checking;
                    }
                }
            }
            missing
        } else {
            queryable
        };

        {
            let mut s = self.update_shared.lock().unwrap();
            for t in &to_query {
                if let Some(ts) = s.per_target.get_mut(t) {
                    ts.state = UpdateState::Checking;
                    ts.phase = UpdatePhase::Checking;
                }
            }
        }
        let settings = self.settings.clone();
        let shared = self.update_shared.clone();
        self.ensure_poll(context);
        context.spawn_background(move |_| {

            let mut index = UpdateIndex::default();
            update::fetch_updates_for(&to_query, &mut |target, rel| {
                let ts = update::check_one(target, rel.as_ref(), &settings);
                update::apply_check(&shared, HashMap::from([(target, ts)]));
                if let Some(rel) = rel {
                    index.loaders.push(rel);
                }
            });
            update::save_cache(&index);

            let results = {
                let s = shared.lock().unwrap();
                s.per_target.clone()
            };
            Message::UpdatesChecked(Ok(results))
        });
    }

    fn ensure_poll(&mut self, context: &ComponentContext<Self>) {
        if self.poll_running {
            return;
        }
        self.poll_running = true;
        context.spawn_background(move |_| {
            std::thread::sleep(Duration::from_millis(200));
            Message::UpdatePoll
        });
    }

    fn launcher_update_pending(&self) -> bool {
        self.update_shared
            .lock()
            .map(|s| {
                s.per_target
                    .get(&UpdateTarget::Launcher)
                    .map(|ts| ts.state == UpdateState::Outdated)
                    .unwrap_or(false)
            })
            .unwrap_or(false)
    }

    pub(in crate::reactor_app) fn update_all_loaders(&mut self, context: &ComponentContext<Self>) {
        if self.game_running {
            self.dialog = Dialog::Error {
                message: t!("page_settings.cannot_update_while_game_is")
                .to_string(),
            };
            return;
        }
        if self.launcher_update_pending() {
            self.dialog = Dialog::Error {
                message: t!("page_settings.launcher_update_available")
                .to_string(),
            };
            return;
        }
        let targets: Vec<UpdateTarget> = {
            let s = self.update_shared.lock().unwrap();
            update::ALL_TARGETS
                .iter()
                .filter(|t| {
                    s.per_target
                        .get(t)
                        .map(|ts| ts.state == UpdateState::Outdated)
                        .unwrap_or(false)
                })
                .copied()
                .collect()
        };
        if targets.is_empty() {
            return;
        }
        for target in targets {
            self.start_loader_update(target, context);
        }
    }

    fn loaders_card(&self, context: &mut ViewContext<Self>) -> View {
        let n = update::ALL_TARGETS.len();

        if n == 0 {
            return Border::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .corner_radius(4.0)
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(1.0)
                .padding(Thickness::new(20.0, 14.0, 20.0, 18.0))
                .content(
                    TextBlock::new()
                        .text(t!("page_settings.none"))
                        .font_size(13.0)
                        .opacity(0.5)
                        .horizontal_alignment(HorizontalAlignment::Center),
                )
                .into();
        }

        let mut row_defs = vec![GridLength::Auto];
        row_defs.push(GridLength::Auto);
        for k in 0..n {
            row_defs.push(GridLength::Auto);
            if k < n - 1 {
                row_defs.push(GridLength::Auto);
            }
        }

        let mut cells: Vec<KeyedView> = Vec::new();
        let headers = [
            t!("page_settings.component").to_string(),
            t!("page_settings.version").to_string(),
            t!("common.status").to_string(),
            String::new(),
        ];
        for (c, txt) in headers.iter().enumerate() {
            cells.push(KeyedView::new(format!("h{c}"), View::from(
                TextBlock::new()
                    .text(txt.clone())
                    .font_size(13.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .opacity(0.5)
                    .grid_row(0)
                    .grid_column(c as i32)
                    .vertical_alignment(VerticalAlignment::Center),
            )));
        }

        cells.push(KeyedView::new("sep-h", View::from(
            Border::new()
                .height(1.5)
                .margin(Thickness::new(0.0, 10.0, 0.0, 10.0))
                .background(ThemeBrush::CardStroke)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_row(1)
                .grid_column(0)
                .grid_column_span(4),
        )));

        let launcher_pending = {
            let s = self.update_shared.lock().unwrap();
            s.per_target
                .get(&UpdateTarget::Launcher)
                .map(|ts| ts.state == UpdateState::Outdated)
                .unwrap_or(false)
        };

        for (i, target) in update::ALL_TARGETS.iter().enumerate() {
            let row = (2 + 2 * i) as i32;
            let ts = {
                let s = self.update_shared.lock().unwrap();
                s.per_target.get(target).cloned().unwrap_or_default()
            };
            let active = matches!(
                ts.phase,
                UpdatePhase::Downloading | UpdatePhase::Extracting | UpdatePhase::Installing
            );
            let pct = if ts.download_total > 0 {
                ts.download_done as f64 / ts.download_total as f64
            } else {
                0.0
            };
            let (label, color) = match ts.state {
                UpdateState::NotConfigured => (t!("page_settings.update_source_not_configured"), COLOR_GREY),
                UpdateState::Checking => (t!("page_settings.checking"), COLOR_WARN),
                UpdateState::UpToDate => (t!("page_settings.up_to_date"), COLOR_OK),
                UpdateState::Outdated => (t!("common.update_available"), COLOR_WARN),
                UpdateState::Error => (t!("common.error"), COLOR_ERR),
            };
            let phase_text = match ts.phase {
                UpdatePhase::Downloading => t!("page_settings.downloading"),
                UpdatePhase::Extracting => t!("page_settings.extracting"),
                UpdatePhase::Installing => t!("page_settings.installing"),
                _ => "".into(),
            };

            let version_cell: View = if active {
                View::from(
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .grid_row(row)
                        .grid_column(1)
                        .children((
                            View::from(
                                Grid::new()
                                    .width(200.0)
                                    .height(6.0)
                                    .children((
                                        View::from(
                                            Border::new()
                                                .corner_radius(3.0)
                                                .background(Brush::Solid(COLOR_GREY)),
                                        ),
                                        View::from(
                                            Border::new()
                                                .corner_radius(3.0)
                                                .background(Brush::Solid(COLOR_ACCENT))
                                                .width(pct * 200.0)
                                                .horizontal_alignment(HorizontalAlignment::Left),
                                        ),
                                    )),
                            ),
                            View::from(
                                TextBlock::new()
                                    .text(format!("{} {:.0}%", phase_text, pct * 100.0))
                                    .font_size(12.0),
                            ),
                        )),
                )
            } else {
                let local = update::last_version_of(&self.settings, *target);
                let remote = ts.remote_version.clone();

                let ver = if ts.state == UpdateState::Outdated {
                    match (&local, &remote) {
                        (Some(l), Some(r)) => format!("{} → {}", l, r),
                        (None, Some(r)) => format!("— → {}", r),
                        (Some(l), None) => l.clone(),
                        _ => "—".to_string(),
                    }
                } else {
                    match (&local, &remote) {
                        (Some(l), Some(r)) if l != r => format!("{} → {}", l, r),
                        (_, Some(r)) => r.clone(),
                        (Some(l), _) => l.clone(),
                        _ => "—".to_string(),
                    }
                };
                View::from(
                    TextBlock::new()
                        .text(ver)
                        .font_size(12.0)
                        .opacity(0.85)
                        .text_trimming(TextTrimming::CharacterEllipsis)
                        .grid_row(row)
                        .grid_column(1)
                        .vertical_alignment(VerticalAlignment::Center),
                )
            };

            let status_text = if ts.state == UpdateState::Error {
                ts.detail.clone().unwrap_or_else(|| label.to_string())
            } else {
                label.to_string()
            };
            let status_cell: View = View::from(
                TextBlock::new()
                    .text(status_text)
                    .font_size(12.0)
                    .foreground(Brush::Solid(color))
                    .text_trimming(TextTrimming::CharacterEllipsis)
                    .grid_row(row)
                    .grid_column(2)
                    .vertical_alignment(VerticalAlignment::Center),
            );

            let action_cell: View = if active {
                View::from(
                    Button::new()
                        .on_click(context.callback(move |_| Message::CancelUpdate(*target)))
                        .grid_row(row)
                        .grid_column(3)
                        .vertical_alignment(VerticalAlignment::Center)
                        .content(t!("common.cancel").to_string()),
                )
            } else if ts.state == UpdateState::Outdated
                && (*target == UpdateTarget::Launcher || !launcher_pending)
            {
                View::from(
                    Button::new()
                        .on_click(context.callback(move |_| Message::UpdateLoader(*target)))
                        .grid_row(row)
                        .grid_column(3)
                        .vertical_alignment(VerticalAlignment::Center)
                        .content(t!("page_settings.update").to_string()),
                )
            } else {
                View::from(
                    StackPanel::new()
                        .grid_row(row)
                        .grid_column(3)
                        .vertical_alignment(VerticalAlignment::Center),
                )
            };

            cells.push(KeyedView::new(format!("r{row}c0"), View::from(
                TextBlock::new()
                    .text(target.display_name())
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .grid_row(row)
                    .grid_column(0)
                    .vertical_alignment(VerticalAlignment::Center),
            )));
            cells.push(KeyedView::new(format!("r{row}c1"), version_cell));
            cells.push(KeyedView::new(format!("r{row}c2"), status_cell));
            cells.push(KeyedView::new(format!("r{row}c3"), action_cell));
            if i + 1 < n {
                cells.push(KeyedView::new(format!("sep{i}"), View::from(
                    Border::new()
                        .height(1.0)
                        .margin(Thickness::new(0.0, 7.0, 0.0, 7.0))
                        .background(ThemeBrush::CardStroke)
                        .vertical_alignment(VerticalAlignment::Center)
                        .grid_row(row + 1)
                        .grid_column(0)
                        .grid_column_span(4),
                )));
            }
        }

        Border::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .padding(Thickness::new(20.0, 14.0, 20.0, 18.0))
            .content(
                Grid::new()
                    .columns([
                        GridLength::Star(1.6),
                        GridLength::Star(1.2),
                        GridLength::Star(1.0),
                        GridLength::Pixel(96.0),
                    ])
                    .column_spacing(28.0)
                    .rows(row_defs)
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .keyed_children(cells),
            )
            .into()
    }

    /// Saves the keychip, which is the launcher's, into `config.ini`.
    pub(in crate::reactor_app) fn save_keychip(&mut self) {
        if let Err(e) = self.store.save_keychip(self.keychip.as_deref()) {
            self.dialog = Dialog::Error {
                message: t!("common.failed_to_save_config_ini", error = e).to_string(),
            };
        }
    }

    pub(in crate::reactor_app) fn settings_page(&self, context: &mut ViewContext<Self>) -> View {
        // Each language is listed by its own name, so it can be found whatever is shown now.
        let lang_items: Vec<String> = Lang::ALL.iter().map(|l| l.native_name().to_string()).collect();
        let lang_idx = Lang::ALL.iter().position(|l| *l == self.lang).unwrap_or(0);

        let title: View = StackPanel::new()
            .children([View::from(
                TextBlock::new()
                    .text(t!("nav.settings"))
                    .font_size(26.0)
                    .font_weight(FontWeight::SEMI_BOLD),
            )])
            .into();

        let version_card = Border::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(settings_row(
                SymbolIcon::new().symbol(Symbol::Setting).into(),
                &t!("page_settings.launcher_version"),
                View::from(
                    TextBlock::new()
                        .text(format!("v{}", crate::APP_VERSION))
                        .font_size(13.0)
                        .opacity(0.8)
                        .vertical_alignment(VerticalAlignment::Center),
                ),
            ));

        let lang_card = Border::new()
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(settings_row(
                SymbolIcon::new().symbol(Symbol::Globe).into(),
                &t!("page_settings.language"),
                ComboBox::new()
                    .width(180.0)
                    .items_source(lang_items)
                    .selected_index(Some(lang_idx))
                    .on_selection_changed(context.callback(Message::LangSelected))
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
            ));

        let server_card = Border::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(settings_row(
                SymbolIcon::new().symbol(Symbol::Globe).into(),
                &t!("page_settings.server_address"),
                View::from(
                    TextBox::new()
                        .width(360.0)
                        .placeholder_text(t!("page_settings.default_sdgt_arcadehub_xyz"))
                        .text(self.net.server.clone().unwrap_or_default())
                        .on_text_changed(context.callback(Message::ServerHostChanged))
                        .vertical_alignment(VerticalAlignment::Center),
                ),
            ));

        // The keychip names this PC to the server and in the rooms, whatever game it runs.
        let keychip_row = settings_row(
            SymbolIcon::new().symbol(Symbol::Admin).into(),
            "Keychip",
            TextBox::new()
                .width(225.0)
                .text(self.keychip.clone().unwrap_or_default())
                .placeholder_text("A69E-01A99999999")
                .on_text_changed(context.callback(Message::KeychipChanged))
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
        );
        let keychip_card = settings_card(vec![KeyedView::new("keychip", keychip_row)]);

        let nick_card = Border::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(settings_row(
                SymbolIcon::new().symbol(Symbol::Contact).into(),
                &t!("page_settings.room_nickname"),
                View::from(
                    TextBox::new()
                        .width(360.0)
                        .placeholder_text(t!("page_settings.nickname_placeholder"))
                        .text(self.room.nickname.clone().unwrap_or_default())
                        .on_text_changed(context.callback(Message::RoomNickChanged))
                        .vertical_alignment(VerticalAlignment::Center),
                ),
            ));

        let update_card = Border::new()
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
                        SymbolIcon::new().symbol(Symbol::Manage).into(),
                        &t!("page_settings.automatically_check_for_updates"),
                        settings_toggle(
                            self.settings.auto_check_updates,
                            true,
                            context.callback(Message::AutoUpdateToggled),
                        ),
                    ),
                    separator(),
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Repair).into(),
                        &t!("page_settings.check_for_updates"),
                        Button::new()
                            .on_click(context.message(Message::CheckUpdatesNow))
                            .content(t!("page_settings.check_now").to_string())
                            .into(),
                    ),
                    separator(),
                )),
            );

        let components_title: View = Grid::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .margin(Thickness::new(16.0, 0.0, 16.0, 0.0))
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(16.0)
            .children((
                TextBlock::new()
                    .text(t!("page_settings.component_updates"))
                    .font_size(20.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(0),
                Button::new()
                    .on_click(context.message(Message::UpdateAllClicked))
                    .style(ButtonStyle::Accent)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(1)
                    .content(t!("page_settings.update_all").to_string()),
            ))
            .into();

        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(

                StackPanel::new()
                    .margin(Thickness::new(28.0, 16.0, 28.0, 24.0))
                    .spacing(14.0)
                    .children((
                        title,
                        version_card,
                        lang_card,
                        server_card,
                        keychip_card,
                        nick_card,
                        update_card,
                        components_title,
                        self.loaders_card(context),
                    )),
            )
            .into()
    }

    pub(in crate::reactor_app) fn apply_updates_checked(
        &mut self,
        res: Result<HashMap<UpdateTarget, TargetStatus>, String>,
    ) {
        match res {
        Ok(results) => {
            update::apply_check(&self.update_shared, results);

            let outdated: Vec<String> = {
                let s = self.update_shared.lock().unwrap();
                update::ALL_TARGETS
                    .iter()
                    .filter_map(|t| {
                        s.per_target.get(t).and_then(|ts| {
                            (ts.state == UpdateState::Outdated).then(|| {
                                t!(
                                    "page_settings.component_with_version",
                                    component = t.display_name(),
                                    version = ts.remote_version.as_deref().unwrap_or("?")
                                )
                                .to_string()
                            })
                        })
                    })
                    .collect()
            };
            if !outdated.is_empty() {
                let key = outdated.join("|");
                if self.update_prompt_key.as_deref() != Some(key.as_str()) {
                    self.update_prompt_key = Some(key);
                    self.dialog = Dialog::UpdateAvailable {
                        detail: outdated.join("\n"),
                    };
                }
            }
        }
        Err(e) => {
            if let Ok(mut s) = self.update_shared.lock() {
                for ts in s.per_target.values_mut() {
                    ts.state = UpdateState::Error;
                    ts.phase = UpdatePhase::Error;
                    ts.detail = Some(e.clone());
                }
            }
            self.dialog = Dialog::Error {
                message: t!("page_settings.update_check_failed", error = e).to_string(),
            };
        }
        }
    }

    pub(in crate::reactor_app) fn cancel_loader_update(&mut self, target: UpdateTarget) {
        if let Some(flag) = self.update_cancel.get(&target) {
            flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }

        if let Some(abort) = self.update_abort.get(&target) {
            abort.close_all();
        }

        if let Ok(mut s) = self.update_shared.lock() {
            if let Some(ts) = s.per_target.get_mut(&target) {
                ts.phase = UpdatePhase::Idle;
                ts.download_done = 0;
                ts.download_total = 0;
            }
        }
    }

    pub(in crate::reactor_app) fn start_loader_update(&mut self, target: UpdateTarget, context: &ComponentContext<Self>) {
        let (url, mirrors, sha) = {
            let s = self.update_shared.lock().unwrap();
            let ts = s.per_target.get(&target).cloned().unwrap_or_default();
            (ts.url.clone(), ts.mirrors.clone(), ts.sha256.clone())
        };
        if url.is_none() {
            self.dialog = Dialog::Error {
                message: t!("page_settings.no_download_url_available").to_string(),
            };
            return;
        }

        if self.game_running {
            self.dialog = Dialog::Error {
                message: t!("page_settings.cannot_update_while_game_is")
                .to_string(),
            };
            return;
        }
        if target != UpdateTarget::Launcher && self.launcher_update_pending() {
            self.dialog = Dialog::Error {
                message: t!("page_settings.launcher_update_available")
                .to_string(),
            };
            return;
        }
        let shared = self.update_shared.clone();
        let base = update::tools_dir();
        let progress = {
            let shared = shared.clone();
            move |phase: UpdatePhase, done: u64, total: u64| {
                if let Ok(mut s) = shared.lock() {
                    if let Some(ts) = s.per_target.get_mut(&target) {
                        ts.phase = phase;
                        ts.download_done = done;
                        ts.download_total = total;
                    }
                }
            }
        };
        {
            let mut s = shared.lock().unwrap();
            if let Some(ts) = s.per_target.get_mut(&target) {
                ts.phase = UpdatePhase::Downloading;
                ts.download_done = 0;
                ts.download_total = 0;
            }
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.update_cancel.insert(target, cancel.clone());
        let abort = Arc::new(update::HttpAbort::new());
        self.update_abort.insert(target, abort.clone());
        self.ensure_poll(context);
        context.spawn_background(move |_| {
            let r = update::download_and_install(
                target,
                url.as_deref().unwrap(),
                &mirrors,
                sha.as_deref(),
                &base,
                &cancel,
                &abort,
                &progress,
            );
            Message::UpdateFinished(target, r)
        });
    }

    pub(in crate::reactor_app) fn finish_update(&mut self, target: UpdateTarget, result: Result<(), String>) {
        let was_cancelled = self
            .update_cancel
            .remove(&target)
            .map(|f| f.load(std::sync::atomic::Ordering::Relaxed))
            .unwrap_or(false);
        if was_cancelled && result.is_err() {
            if let Ok(mut s) = self.update_shared.lock() {
                if let Some(ts) = s.per_target.get_mut(&target) {

                    ts.phase = UpdatePhase::Idle;
                    ts.download_done = 0;
                    ts.download_total = 0;
                }
            }
            return;
        }

        match result {
        Ok(()) => {
            let ver = {
                let s = self.update_shared.lock().unwrap();
                s.per_target
                    .get(&target)
                    .and_then(|t| t.remote_version.clone())
            };
            if let Some(v) = ver {
                update::set_last_version(&mut self.settings, target, &v);
                self.store.save_settings(&self.settings);
                if let Ok(mut s) = self.update_shared.lock() {
                    if let Some(ts) = s.per_target.get_mut(&target) {
                        ts.state = UpdateState::UpToDate;
                        ts.phase = UpdatePhase::Done;
                        ts.download_done = ts.download_total.max(1);
                        ts.detail = Some(v.clone());
                    }
                }
            }

            self.detected = crate::loaders::detect_loaders();
        }
        Err(e) => {
            if let Ok(mut s) = self.update_shared.lock() {
                if let Some(ts) = s.per_target.get_mut(&target) {
                    ts.state = UpdateState::Error;
                    ts.phase = UpdatePhase::Error;
                }
            }
            self.dialog = Dialog::Error {
                message: t!("page_settings.update_failed", error = e).to_string(),
            };
        }
        }
    }

    pub(in crate::reactor_app) fn poll_update_progress(&mut self, context: &ComponentContext<Self>) {
        let active = {
            let s = self.update_shared.lock().unwrap();
            s.per_target.values().any(|t| {
                matches!(
                    t.phase,
                    UpdatePhase::Checking
                        | UpdatePhase::Downloading
                        | UpdatePhase::Extracting
                        | UpdatePhase::Installing
                )
            })
        };
        if active {
            self.ensure_poll(context);
        } else {
            self.poll_running = false;
        }
    }

}
