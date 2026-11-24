use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use windows_reactor::*;

use crate::db;
use crate::games::{family_of, Series};
use crate::detect::{self, IdentifyResult};
use crate::icf;
use rust_i18n::t;
use crate::storage::GameRecord;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::{CompanionKind, Dialog, IdentifyPayload, IcfPayload, Message, RowAction};
use crate::reactor_app::state::PendingAdd;
use super::game_settings::SettingsTab;
use crate::reactor_app::widgets::{builtin_cover_name, make_id, now_secs, platform_name, record_db_match, fallback_cover, row_chip, same_path, separator, settings_row, settings_row_sub, settings_toggle, spawn_anim_tick};

pub(in crate::reactor_app) const ROW_COVER_W: f64 = 104.0;
pub(in crate::reactor_app) const ROW_COVER_H: f64 = 68.0;

pub(in crate::reactor_app) const GAME_REGIONS: &[&str] = &["JPN", "EXP", "CHN"];

fn cover_file_exists(path: &str) -> bool {
    static CACHE: OnceLock<Mutex<HashMap<String, bool>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = cache.lock().unwrap_or_else(|e| e.into_inner());
    match guard.get(path) {
        Some(ok) => *ok,
        None => {
            let ok = Path::new(path).is_file();
            guard.insert(path.to_string(), ok);
            ok
        }
    }
}

/// The library's games as the Games page lists them: by series, and within one by release
/// date (see [`db::release_order`]); a game the database does not know comes last in its
/// series, in the order it was added. Each game keeps its index in the library, which every
/// row action uses: the library itself is never reordered.
fn games_by_series(games: &[GameRecord]) -> Vec<(Series, Vec<(usize, &GameRecord)>)> {
    let mut rows: Vec<(Series, Option<(&str, usize)>, usize, &GameRecord)> = games
        .iter()
        .enumerate()
        .map(|(index, game)| (family_of(game).ui_features().series, db::release_order(game), index, game))
        .collect();
    // `None` (not in the database) sorts after every release; the index keeps the order added.
    rows.sort_by_key(|(series, release, index, _)| (*series, release.is_none(), *release, *index));

    rows.chunk_by(|a, b| a.0 == b.0)
        .map(|group| (group[0].0, group.iter().map(|(_, _, index, game)| (*index, *game)).collect()))
        .collect()
}

/// The heading of a series on the Games page.
fn series_title(series: Series) -> String {
    match series {
        Series::InitialD => t!("page_games.series_initial_d"),
        Series::WanganMidnight => t!("page_games.series_wangan_midnight"),
    }
    .to_string()
}

impl Launcher {

    pub(in crate::reactor_app) fn games_page(&self, context: &mut ViewContext<Self>) -> View {
        let mut rows: Vec<KeyedView> = Vec::new();
        for (series, games) in games_by_series(&self.library.games) {
            rows.push(KeyedView::new(
                format!("series-{series:?}"),
                TextBlock::new()
                    .text(series_title(series))
                    .font_size(16.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .margin(Thickness::new(0.0, 12.0, 0.0, 2.0)),
            ));
            rows.extend(games.into_iter().map(|(i, g)| self.game_row(context, i, g)));
        }

        let subtitle = t!("page_games.game_count", games = self.library.games.len()).to_string();

        let games_view: View = if rows.is_empty() {

            StackPanel::new()
                .spacing(8.0)
                .margin(Thickness::new(0.0, 48.0, 0.0, 0.0))
                .horizontal_alignment(HorizontalAlignment::Center)
                .vertical_alignment(VerticalAlignment::Center)
                .children((
                    TextBlock::new()
                        .text(t!("page_games.no_games_added_yet"))
                        .font_size(20.0)
                        .font_weight(FontWeight::SEMI_BOLD),
                    TextBlock::new()
                        .text(t!("page_games.click_add_game_at_top"))
                        .opacity(0.6)
                        .text_wrapping(TextWrapping::Wrap)
                        .horizontal_alignment(HorizontalAlignment::Center),
                ))
                .into()
        } else {
            ScrollViewer::new()
                .vertical_scroll_bar_visibility(ScrollBarVisibility::Visible)
                .content(
                    StackPanel::new()
                        .horizontal_alignment(HorizontalAlignment::Stretch)
                        .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
                        .spacing(4.0)
                        .children((

                            StackPanel::new()
                                .spacing(4.0)
                                .children((
                                    TextBlock::new()
                                        .text(t!("nav.games"))
                                        .font_size(26.0)
                                        .font_weight(FontWeight::SEMI_BOLD),
                                    TextBlock::new().text(subtitle).font_size(12.0).opacity(0.6),
                                )),

                            StackPanel::new()
                                .horizontal_alignment(HorizontalAlignment::Stretch)
                                .spacing(6.0)
                                .keyed_children(rows),
                        )),
                )
                .into()
        };

        games_view
    }

    fn game_row(
        &self,
        context: &mut ViewContext<Self>,
        index: usize,
        game: &GameRecord,
    ) -> KeyedView {

        let cover: View = {
            let mut rendered: Option<View> = None;
            if let Some(path) = &game.image {
                if cover_file_exists(path) {
                    if let Ok(img) = Image::new().source_file(path) {
                        rendered = Some(
                            img.width(ROW_COVER_W)
                                .height(ROW_COVER_H)
                                .stretch(Stretch::Uniform)
                                .into(),
                        );
                    }
                }
            }
            if rendered.is_none() {
                if let Some(bytes) = builtin_cover_name(game).as_deref().and_then(Self::builtin_cover_bytes) {
                    rendered = Some(
                        Image::new()
                            .source_data(EncodedImage::new(Arc::<[u8]>::from(bytes)))
                            .width(ROW_COVER_W)
                            .height(ROW_COVER_H)
                            .stretch(Stretch::Uniform)
                            .into(),
                    );
                }
            }
            rendered.unwrap_or_else(|| fallback_cover(ROW_COVER_W, ROW_COVER_H))
        };

        let mut parts: Vec<String> = Vec::new();
        parts.push(game.version.as_deref().unwrap_or(&t!("page_games.unidentified")).to_string());
        if let Some(id) = record_db_match(game).and_then(|m| m.game_id) {
            parts.push(id);
        }
        if game.serverbox.is_some() {
            parts.push(t!("page_games.serverbox_configured").to_string());
        }
        let sub = parts.join(" · ");

        let base_fill = || -> Brush { ThemeBrush::CardBackground.into() };

        let hover_fill = || -> Brush { ThemeBrush::SolidBackground.into() };

        let dropping = self.drop_index == Some(index);
        let drop_chevron = FontIcon::new().glyph(if dropping {
            "\u{E70D}"
        } else {
            "\u{E76C}"
        });

        let platform_label = platform_name(game);
        let right: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(8.0)
            .vertical_alignment(VerticalAlignment::Center)
            .grid_column(2)
            .children((row_chip(&platform_label), drop_chevron));

        let header: View = Grid::new()
            .columns([GridLength::Auto, GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(12.0)
            .children((
                Border::new()
                    .width(ROW_COVER_W)
                    .height(ROW_COVER_H)
                    .corner_radius(4.0)
                    .grid_column(0)
                    .content(cover),
                StackPanel::new()
                    .spacing(3.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(1)
                    .children((
                        TextBlock::new()
                            .text(game.name.clone())
                            .font_size(15.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_trimming(TextTrimming::CharacterEllipsis),
                        TextBlock::new().text(sub).font_size(12.0).opacity(0.6),
                    )),
                right,
            ))
            .into();

        let row: View = if dropping {
            let eased = {
                let t = self.drop_anim;
                t * (2.0 - t)
            };

            let file_name = |p: &str| -> String {
                Path::new(p)
                    .file_name()
                    .map(|f| f.to_string_lossy().into_owned())
                    .unwrap_or_else(|| p.to_string())
            };

            let sb_supported = record_db_match(game).and_then(|m| m.sb_crc).is_some();
            let features = crate::games::family_of(game).ui_features();

            let game_running_here = self.game_running
                && self.game_runtime.as_ref().map(|r| r.index) == Some(index);
            let sb_running_here = self
                .serverbox_runtime
                .as_ref()
                .map(|r| r.index)
                == Some(index);

            let cards_desc = match self.selected_card(game) {
                Some(c) => {
                    let extension = features.card_extension;
                    let has_extension = Path::new(&c)
                        .extension()
                        .is_some_and(|ext| ext.eq_ignore_ascii_case(extension));
                    let file = if has_extension {
                        c
                    } else {
                        format!("{c}.{extension}")
                    };
                    t!("page_games.current_card", file = file).to_string()
                }
                None => t!("page_games.not_selected").to_string(),
            };

            let mut rows: Vec<KeyedView> = vec![
                KeyedView::new(
                    "drop-launch",
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Play).into(),
                        &t!("common.launch_game"),

                        if game_running_here {
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .on_click(context.message(Message::LaunchStop(index)))
                                .content(t!("common.close").to_string())
                                .into()
                        } else if !features.test_mode {
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .on_click(context.message(Message::RowMenu {
                                    index,
                                    action: RowAction::Launch,
                                }))
                                .content(t!("page_games.launch").to_string())
                                .into()
                        } else {
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(16.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    CheckBox::new()
                                        .is_checked(game.test_mode == Some(true))
                                        .on_is_checked_changed(context.callback(
                                            move |on: bool| Message::RowTestModeToggled(index, on),
                                        ))
                                        .vertical_alignment(VerticalAlignment::Center)

                                        .content(t!("page_games.test_mode").to_string()),
                                    Button::new()
                                        .style(ButtonStyle::Accent)
                                        .on_click(context.message(Message::RowMenu {
                                            index,
                                            action: RowAction::Launch,
                                        }))
                                        .content(t!("page_games.launch").to_string()),
                                ))
                                .into()
                        },
                    ),
                ),
            ];

            rows.push(KeyedView::new("drop-sep-settings", separator()));
            rows.push(KeyedView::new(
                "drop-settings",
                settings_row(
                    SymbolIcon::new().symbol(Symbol::Setting).into(),
                    &t!("page_games.game_settings"),
                    Button::new()
                        .on_click(context.message(Message::RowMenu {
                            index,
                            action: RowAction::GameSettings,
                        }))
                        .content(t!("page_games.enter").to_string())
                        .into(),
                ),
            ));

            if features.cards {
                rows.push(KeyedView::new("drop-sep-cards", separator()));
                rows.push(KeyedView::new(
                    "drop-cards",
                    settings_row_sub(
                        SymbolIcon::new().symbol(Symbol::List).into(),
                        &t!("common.card_select"),
                        Some(cards_desc),
                        Button::new()
                            .on_click(context.message(Message::RowMenu {
                                index,
                                action: RowAction::Cards,
                            }))
                            .content(t!("page_games.enter").to_string()),
                    ),
                ));
            }

            if features.region {
                let region_items: Vec<String> = GAME_REGIONS.iter().map(|v| v.to_string()).collect();
                let region_idx = GAME_REGIONS
                    .iter()
                    .position(|v| Some(*v) == game.region.as_deref());
                rows.push(KeyedView::new("drop-sep-region", separator()));
                rows.push(KeyedView::new(
                    "drop-region",
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::World).into(),
                        "Region",
                        ComboBox::new()
                            .width(180.0)
                            .items_source(region_items)
                            .selected_index(region_idx)
                            .placeholder_text(t!("page_games.select_region"))
                            .on_selection_changed(
                                context.callback(move |i| Message::GameRegionSelected(index, i)),
                            )
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    ),
                ));
            }

                if features.graphics_patch {
                    rows.insert(1, KeyedView::new("drop-sep-gfx", separator()));
                    rows.insert(
                        2,
                        KeyedView::new(
                            "drop-gfx",
                            settings_row(
                            SymbolIcon::new().symbol(Symbol::Repair).into(),
                            &t!("page_games.graphics_patch"),
                            settings_toggle(
                                game.gfx_patch.unwrap_or(true),
                                true,
                                context.callback(move |on: bool| Message::GfxPatchToggled(index, on)),
                            ),
                        ),
                    ),
                );
                }

            if sb_supported {
                let sb_desc = game
                    .serverbox
                    .as_deref()
                    .map(|p| t!("page_games.configured", file_name = file_name(p)).to_string())
                    .unwrap_or_else(|| {
                        t!("page_games.serverbox_not_configured").to_string()
                    });
                let sb_control: View = if sb_running_here {

                    Button::new()
                        .style(ButtonStyle::Accent)
                        .on_click(context.message(Message::ServerBoxStop(index)))
                        .content(t!("common.close").to_string())
                        .into()
                } else if game.serverbox.is_some() {
                    StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(8.0)
                        .children((
                            Button::new()
                                .style(ButtonStyle::Accent)
                                .on_click(context.message(Message::RowMenu {
                                    index,
                                    action: RowAction::LaunchServerBox,
                                }))
                                .content(t!("page_games.launch").to_string()),
                            Button::new()
                                .on_click(context.message(Message::RowMenu {
                                    index,
                                    action: RowAction::AddServerBox,
                                }))
                                .content(t!("page_games.replace").to_string()),
                            Button::new()
                                .on_click(context.message(Message::RowMenu {
                                    index,
                                    action: RowAction::RemoveServerBox,
                                }))
                                .content(t!("common.remove").to_string()),
                        ))
                        .into()
                } else {
                    Button::new()
                        .on_click(context.message(Message::RowMenu {
                            index,
                            action: RowAction::AddServerBox,
                        }))
                        .content(t!("common.add").to_string())
                };
                rows.push(KeyedView::new("drop-sep-sb", separator()));
                rows.push(KeyedView::new(
                    "drop-serverbox",

                    settings_row_sub(
                        SymbolIcon::new().symbol(Symbol::Manage).into(),
                        "ServerBox",
                        Some(sb_desc),
                        sb_control,
                    ),
                ));
            }

            if features.companion_executables {
                let companion_row = |key: &str,
                                     title: String,
                                     configured: Option<&String>,
                                     add: RowAction,
                                     remove: RowAction|
                 -> (KeyedView, KeyedView) {
                    let desc = configured
                        .as_deref()
                        .map(|p| t!("page_games.saved", file_name = file_name(p)).to_string())
                        .unwrap_or_else(|| {
                            t!("page_games.file_not_configured").to_string()
                        });
                    let control: View = if configured.is_some() {
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(8.0)
                            .children((
                                Button::new()
                                    .on_click(context.message(Message::RowMenu { index, action: add }))
                                    .content(t!("page_games.replace").to_string()),
                                Button::new()
                                    .on_click(context.message(Message::RowMenu { index, action: remove }))
                                    .content(t!("common.remove").to_string()),
                            ))
                            .into()
                    } else {
                        Button::new()
                            .on_click(context.message(Message::RowMenu { index, action: add }))
                            .content(t!("common.add").to_string())
                    };

                    let control: View = StackPanel::new()
                        .orientation(Orientation::Horizontal)
                        .spacing(12.0)
                        .vertical_alignment(VerticalAlignment::Center)
                        .children((
                            TextBlock::new()
                                .text(desc)
                                .font_size(12.0)
                                .opacity(0.7)
                                .vertical_alignment(VerticalAlignment::Center),
                            control,
                        ))
                        .into();
                    (
                        KeyedView::new(format!("drop-sep-{key}"), separator()),
                        KeyedView::new(
                            format!("drop-{key}"),
                            settings_row(
                                SymbolIcon::new().symbol(Symbol::Manage).into(),
                                &title,
                                control,
                            ),
                        ),
                    )
                };
                let (sep1, row1) = companion_row(
                    "amdaemon",
                    "amdaemon.exe".to_string(),
                    game.amdaemon.as_ref(),
                    RowAction::AddAmdaemon,
                    RowAction::RemoveAmdaemon,
                );
                rows.push(sep1);
                rows.push(row1);
                let (sep2, row2) = companion_row(
                    "gameexe",
                    t!("page_games.game_executable").to_string(),
                    game.game_exe.as_ref(),
                    RowAction::AddGameExe,
                    RowAction::RemoveGameExe,
                );
                rows.push(sep2);
                rows.push(row2);
            }

            rows.push(KeyedView::new("drop-sep-remove", separator()));
            rows.push(KeyedView::new(
                "drop-remove",
                settings_row(
                    FontIcon::new().glyph("\u{E74D}").into(),
                    &t!("page_games.remove_game"),
                    Button::new()
                        .on_click(context.message(Message::GameRemoveClicked(index)))
                        .content(t!("common.remove").to_string()),
                ),
            ));

            let divider: View = Border::new()
                .height(1.0)
                .background(ThemeBrush::CardStroke)
                .into();
            let rows_panel: View = StackPanel::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .keyed_children(rows)
                .into();
            let drop_area = StackPanel::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .opacity(eased)
                .children((divider, rows_panel));

            let row_header: View = Border::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .corner_radius(6.0)
                .background(if self.hovered == Some(index) { hover_fill() } else { base_fill() })
                .margin(Thickness::new(0.0, 0.0, 0.0, 4.0))
                .padding(Thickness::new(10.0, 10.0, 16.0, 10.0))
                .on_pointer_entered(context.callback(move |_| Message::GameRowHovered(index)))
                .on_pointer_exited(context.callback(move |_| Message::GameRowUnhovered(index)))
                .on_pointer_pressed(context.callback(move |_| Message::GameRowDropToggled(index)))
                .content(header)
                .into();
            StackPanel::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .children((row_header, drop_area))
                .into()
        } else {

            Border::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .corner_radius(6.0)
                .background(if self.hovered == Some(index) { hover_fill() } else { base_fill() })
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(1.0)
                .margin(Thickness::new(0.0, 0.0, 0.0, 4.0))
                .padding(Thickness::new(10.0, 10.0, 16.0, 10.0))
                .on_pointer_entered(context.callback(move |_| Message::GameRowHovered(index)))
                .on_pointer_exited(context.callback(move |_| Message::GameRowUnhovered(index)))
                .on_pointer_pressed(context.callback(move |_| Message::GameRowDropToggled(index)))
                .content(header)
        };

        KeyedView::new(format!("game-{}", game.id), row)
    }

    pub(in crate::reactor_app) fn handle_file_picked(&mut self, path: PathBuf, context: &ComponentContext<Self>) {
        if self.library.games.iter().any(|g| same_path(&g.path, &path)) {
            self.dialog = Dialog::Error {
                message: t!("page_games.this_game_is_already_in").to_string(),
            };
            return;
        }

        let is_icf = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.eq_ignore_ascii_case("ICF1"));
        context.spawn_background(move |_| {
            if is_icf {
                let result = icf::parse_icf(&path)
                    .map_err(|e| e.to_string())
                    .and_then(|info| {
                        match db::lookup_by_game_id(&info.id, &info.version) {
                            Some(m) => Ok((info, m)),
                            None => Err(t!("page_games.no_library_entry_found_for", id = info.id, version = info.version).to_string()),
                        }
                    });
                Message::IcfIdentified(Box::new(IcfPayload { path, result }))
            } else {
                let result = detect::identify(&path).map_err(|e| e.to_string());
                Message::Identified(Box::new(IdentifyPayload { path, result }))
            }
        });
    }

    /// Adds a game to the library. A game added while `config.ini` still holds the settings
    /// it kept for every game gets them in a file of its own.
    pub(in crate::reactor_app) fn add_game(&mut self, game: GameRecord) {
        self.library.games.push(game);
        self.store.save_library(&self.library);
        let moved = self.store.move_shared_settings(&self.library.games);
        self.report_moved_settings(moved);
    }

    pub(in crate::reactor_app) fn handle_identified(&mut self, payload: IdentifyPayload) {
        let IdentifyPayload { path, result } = payload;
        match result {
            Err(e) => self.dialog = Dialog::Error { message: t!("page_games.identification_failed", error = e).to_string() },
            Ok(ir) => {
                if let Some(d) = &ir.db {

                    self.add_game(GameRecord {
                        id: make_id(),
                        path: path.display().to_string(),
                        name: d.name.clone(),
                        crc: Some(format!("0x{:08x}", ir.crc)),
                        version: Some(d.version.clone()),
                        game_id: d.game_id.clone(),
                        arch: Some(ir.kind.arch().to_string()),
                        loader: d.loader,
                        platform: d.platform.clone(),
                        image: None,
                        cover: d.image.clone(),
                        serverbox: None,
                        reader: None,
                        amdaemon: None,
                        game_exe: None,
            gfx_patch: None,
                        test_mode: None,
                        card: None,
                        region: None,
                        play_count: 0,
                        last_played: None,
                        added_at: now_secs(),
                    });
                } else {

                    let default = path
                        .file_stem()
                        .map(|s| s.to_string_lossy().to_string())
                        .unwrap_or_default();
                    self.pending = Some(PendingAdd { path, ir });
                    self.dialog = Dialog::Naming { name: default };
                }
            }
        }
    }

    pub(in crate::reactor_app) fn handle_icf_identified(&mut self, payload: IcfPayload) {
        let IcfPayload { path, result } = payload;
        match result {
            Err(e) => self.dialog = Dialog::Error { message: t!("page_games.identification_failed", error = e).to_string() },
            Ok((_info, m)) => {

                self.add_game(GameRecord {
                    id: make_id(),
                    path: path.display().to_string(),
                    name: m.name.clone(),
                    crc: None,
                    version: Some(m.version.clone()),
                    game_id: Some(_info.id.clone()),
                    arch: None,
                    loader: m.loader,
                    platform: m.platform.clone(),
                    image: None,
                    cover: m.image.clone(),
                    serverbox: None,
                    reader: None,
                    amdaemon: None,
                    game_exe: None,
            gfx_patch: None,
                    test_mode: None,
                    card: None,
                    region: None,
                    play_count: 0,
                    last_played: None,
                    added_at: now_secs(),
                });
            }
        }
    }

    pub(in crate::reactor_app) fn toggle_row_drop(&mut self, index: usize, context: &ComponentContext<Self>) {

        if self.drop_index == Some(index) && self.drop_open {
            self.drop_open = false;
        } else {
            self.drop_index = Some(index);
            self.drop_open = true;
            self.drop_anim = 0.0;
        }

        self.drop_anim_gen += 1;
        let (epoch, step) = (self.drop_anim_gen, 0.016 / 0.2);
        spawn_anim_tick(context, move || Message::GameRowDropTick(epoch, step));
    }

    pub(in crate::reactor_app) fn tick_row_drop(&mut self, epoch: u32, step: f64, context: &ComponentContext<Self>) {

        if epoch != self.drop_anim_gen {
            return;
        }
        if self.drop_open {
            self.drop_anim = (self.drop_anim + step).min(1.0);
        } else {
            self.drop_anim = (self.drop_anim - step).max(0.0);
        }
        let done = if self.drop_open {
            self.drop_anim >= 1.0
        } else {
            if self.drop_anim <= 0.0 {

                self.drop_index = None;
            }
            self.drop_anim <= 0.0
        };
        if !done {
            spawn_anim_tick(context, move || Message::GameRowDropTick(epoch, step));
        }
    }

    pub(in crate::reactor_app) fn handle_row_menu(&mut self, index: usize, action: RowAction, context: &ComponentContext<Self>) {
        if self.library.games.get(index).is_none() {
            return;
        }
        match action {
            RowAction::Launch => self.do_launch(context, index),
            RowAction::LaunchServerBox => self.do_launch_serverbox(context, index),
            RowAction::Cards => {
                self.dialog = Dialog::None;
                self.enter_cards(index, context);
            }
            RowAction::GameSettings => self.open_game_settings(index, SettingsTab::default(), context),
            RowAction::AddServerBox => {
                self.pick_companion(context, CompanionKind::ServerBox, index);
            }
            RowAction::RemoveServerBox => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.serverbox = None;
                }
                self.store.save_library(&self.library);
            }
            RowAction::AddAmdaemon => {
                self.pick_companion(context, CompanionKind::Amdaemon, index);
            }
            RowAction::RemoveAmdaemon => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.amdaemon = None;
                }
                self.store.save_library(&self.library);
            }
            RowAction::AddGameExe => {
                self.pick_companion(context, CompanionKind::GameExe, index);
            }
            RowAction::RemoveGameExe => {
                if let Some(g) = self.library.games.get_mut(index) {
                    g.game_exe = None;
                }
                self.store.save_library(&self.library);
            }
        }
    }

    pub(in crate::reactor_app) fn handle_companion_picked(
        &mut self,
        kind: CompanionKind,
        index: usize,
        path: PathBuf,
        context: &ComponentContext<Self>,
    ) {

        if matches!(kind, CompanionKind::Amdaemon | CompanionKind::GameExe) {
            if let Some(g) = self.library.games.get_mut(index) {
                let slot = match kind {
                    CompanionKind::Amdaemon => &mut g.amdaemon,
                    CompanionKind::GameExe => &mut g.game_exe,
                    _ => return,
                };
                *slot = Some(path.display().to_string());
                self.store.save_library(&self.library);
            }
            return;
        }

        context.spawn_background(move |_| {
            let result = detect::identify(&path).map_err(|e| e.to_string());
            Message::CompanionIdentified {
                kind,
                index,
                path,
                result,
            }
        });
    }

    pub(in crate::reactor_app) fn handle_companion_identified(
        &mut self,
        kind: CompanionKind,
        index: usize,
        path: PathBuf,
        result: Result<IdentifyResult, String>,
    ) {
        let Some(game) = self.library.games.get(index) else {
            return;
        };
        match kind {
            CompanionKind::ServerBox => {

                if let (Some(gd), Some(sd)) = (Path::new(&game.path).parent(), path.parent()) {
                    if same_path(&gd.to_string_lossy(), sd) {
                        self.dialog = Dialog::Error {
                            message: t!("page_games.serverbox_cannot_be_in_same")
                                .to_string(),
                        };
                        return;
                    }
                }

                match record_db_match(game).and_then(|m| m.sb_crc) {
                    None => {
                        self.dialog = Dialog::Error {
                            message: t!("page_games.currently_matched_version_of_this").to_string()
                        }
                    }
                    Some(expected) => match result {
                        Err(e) => self.dialog = Dialog::Error {
                            message: t!("page_games.serverbox_identification_failed", error = e).to_string(),
                        },
                        Ok(ir) if ir.crc == expected => {
                            self.library.games[index].serverbox =
                                Some(path.display().to_string());
                            self.store.save_library(&self.library);
                        }
                        Ok(ir) => {
                            self.dialog = Dialog::Error {
                                message: t!("page_games.selected_file_is_not_this", crc = ir.crc : {:08x}, expected = expected : {:08x}).to_string()
                            }
                        }
                    },
                }
            }

            CompanionKind::Amdaemon | CompanionKind::GameExe => {}
        }
    }

}

#[cfg(test)]
mod order_tests {
    use super::*;
    use crate::games::test_input::record;
    use crate::storage::Loader;

    /// A game added from its executable, which the database knows by `crc`.
    fn by_crc(loader: Loader, crc: &str) -> GameRecord {
        GameRecord { crc: Some(crc.into()), ..record(loader, None) }
    }

    /// A WMMT3 game added from its `main`; the library records the game id the database gave it.
    fn wmmt3(crc: &str, game_id: &str) -> GameRecord {
        GameRecord { crc: Some(crc.into()), ..record(Loader::Kitten, Some(game_id)) }
    }

    /// The library indices of the games, series by series.
    fn order(games: &[GameRecord]) -> Vec<(Series, Vec<usize>)> {
        games_by_series(games)
            .into_iter()
            .map(|(series, members)| (series, members.into_iter().map(|(i, _)| i).collect()))
            .collect()
    }

    #[test]
    fn games_are_listed_by_series_then_release_date() {
        let games = vec![
            wmmt3("0x85a0e7ab", "W3P"),           // 0: Maximum Tune 3DX+ JPN, 2010
            by_crc(Loader::Kitten, "0xda04e5e0"), // 1: Arcade Stage 5 JPN, 2009
            wmmt3("0xcf5dde93", "WM3"),           // 2: Maximum Tune 3 JPN, 2007
            by_crc(Loader::Mice, "0x1fd6136a"),   // 3: Arcade Stage 8 JPN, 2014
            wmmt3("0xfa4b36bc", "W3X"),           // 4: Maximum Tune 3DX JPN, 2008
            by_crc(Loader::Kitten, "0x361d1cbe"), // 5: Arcade Stage 4 JPN, 2007
        ];
        assert_eq!(order(&games), [(Series::InitialD, vec![5, 1, 3]), (Series::WanganMidnight, vec![2, 4, 0])]);
    }

    #[test]
    fn versions_out_the_same_day_follow_the_database_jpn_exp_chn() {
        let games = vec![
            wmmt3("0x4753d367", "W3P"), // 3DX+ CHN
            wmmt3("0x827c956b", "W3P"), // 3DX+ EXP
            wmmt3("0x85a0e7ab", "W3P"), // 3DX+ JPN
        ];
        assert_eq!(order(&games), [(Series::WanganMidnight, vec![2, 1, 0])]);
    }

    #[test]
    fn a_game_added_from_its_icf_is_placed_by_game_id_and_version() {
        let mut season_2 = record(Loader::Segatools, Some("SDGT"));
        season_2.version = Some("1.70.00".into());
        let kai = by_crc(Loader::Kitten, "0x6cb19701"); // Arcade Stage 4 kai, 2008
        assert_eq!(order(&[season_2, kai]), [(Series::InitialD, vec![1, 0])]);
    }

    #[test]
    fn unknown_games_come_last_in_their_series_in_the_order_added() {
        let mut unknown_a = record(Loader::Kitten, None);
        unknown_a.version = None;
        let unknown_b = unknown_a.clone();
        let games = vec![unknown_a, by_crc(Loader::Kitten, "0xda04e5e0"), unknown_b];
        assert_eq!(order(&games), [(Series::InitialD, vec![1, 0, 2])]);
    }
}
