//! A game's settings page, opened from the game's row: its graphics, system and input
//! settings on three tabs. Each game keeps them in a file of its own
//! (`storage::Store::game_settings_path`); the page loads them into the launcher's
//! `system`, `coin`, `graphics`, `input` and `wmmt3` fields when it opens, and every change
//! is saved into that file.

use windows_reactor::*;

use rust_i18n::t;

use crate::games::wmmt3::settings as wmmt3_settings;
use crate::games::{self, SettingsKind, Wmmt3Settings};
use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::storage::GameRecord;

/// The tabs of a game's settings page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::reactor_app) enum SettingsTab {
    #[default]
    Graphics,
    System,
    Input,
}

impl SettingsTab {
    const ALL: [SettingsTab; 3] = [SettingsTab::Graphics, SettingsTab::System, SettingsTab::Input];

    fn title(self) -> String {
        match self {
            SettingsTab::Graphics => t!("page_game_settings.tab_graphics"),
            SettingsTab::System => t!("page_game_settings.tab_system"),
            SettingsTab::Input => t!("page_game_settings.tab_input"),
        }
        .to_string()
    }
}

impl Launcher {
    /// The game whose settings page is open.
    pub(in crate::reactor_app) fn settings_game(&self) -> Option<&GameRecord> {
        let id = self.settings_game.as_deref()?;
        self.library.games.iter().find(|g| g.id == id)
    }

    /// The settings the open settings page offers.
    pub(in crate::reactor_app) fn settings_kind(&self) -> Option<SettingsKind> {
        self.settings_game().map(settings_kind_of)
    }

    /// Opens the settings page of the game at `index` on `tab`, with that game's settings.
    pub(in crate::reactor_app) fn open_game_settings(&mut self, index: usize, tab: SettingsTab, context: &ComponentContext<Self>) {
        // Changes still waiting to be saved belong to the game shown until now.
        self.save_pending_now();
        let Some(game) = self.library.games.get(index).cloned() else {
            return;
        };
        let saved = self.store.load_game_settings(&game);
        self.system = saved.system;
        self.coin = saved.coin;
        self.graphics = saved.graphics;
        self.input = saved.input;
        self.wmmt3 = Wmmt3Settings::load(&self.store.game_settings_path(&game));

        self.settings_game = Some(game.id);
        self.settings_tab = tab;
        self.dialog = Dialog::None;
        self.nav_tag = "game_settings".to_string();
        if tab == SettingsTab::Input {
            self.refresh_input_devices(context);
        }
    }

    pub(in crate::reactor_app) fn game_settings_page(&self, context: &mut ViewContext<Self>) -> View {
        let Some(game) = self.settings_game() else {
            return self.games_page(context);
        };
        let header: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .children((
                Button::new()
                    .style(ButtonStyle::Subtle)
                    .on_click(context.message(Message::BackToGames))
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(SymbolIcon::new().symbol(Symbol::Back)),
                TextBlock::new()
                    .text(t!("page_game_settings.title", name = game.name))
                    .font_size(26.0)
                    .font_weight(FontWeight::SEMI_BOLD)
                    .vertical_alignment(VerticalAlignment::Center),
            ))
            .into();

        let tabs = SettingsTab::ALL.map(|tab| {
            KeyedView::new(
                format!("{tab:?}"),
                SelectorBarItem::new().text(tab.title()).is_selected(self.settings_tab == tab),
            )
        });
        let tab_bar: View = SelectorBar::new()
            .on_selected_text_changed(context.callback(Message::SettingsTabSelected))
            .collection_slot(SelectorBarSlot::Items, tabs)
            .into();

        let content: View = match (settings_kind_of(game), self.settings_tab) {
            (SettingsKind::InitialD, SettingsTab::Graphics) => self.graphics_settings(context),
            (SettingsKind::InitialD, SettingsTab::System) => self.system_settings(context),
            (SettingsKind::InitialD, SettingsTab::Input) => self.input_settings(context),
            (SettingsKind::Wmmt3, SettingsTab::Graphics) => {
                tab_content(self.wmmt3_sections(context, wmmt3_settings::GRAPHICS_SECTIONS))
            }
            (SettingsKind::Wmmt3, SettingsTab::System) => {
                let mut children = vec![KeyedView::new("log-level", self.log_level_card(context))];
                children.extend(self.wmmt3_sections(context, wmmt3_settings::SYSTEM_SECTIONS));
                tab_content(children)
            }
            (SettingsKind::Wmmt3, SettingsTab::Input) => {
                let mut children = vec![KeyedView::new("input-header", self.input_header(context))];
                children.extend(self.wmmt3_sections(context, wmmt3_settings::INPUT_SECTIONS));
                tab_content(children)
            }
        };

        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
                    .spacing(4.0)
                    .keyed_children(vec![
                        KeyedView::new("header", header),
                        KeyedView::new("tabs", tab_bar),
                        // Keyed by game and tab, so another tab's controls are built anew.
                        KeyedView::new(format!("{}-{:?}", game.id, self.settings_tab), content),
                    ]),
            )
            .into()
    }

    /// Handles the messages of the settings page itself; hands the others back.
    pub(in crate::reactor_app) fn handle_game_settings(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {
            Message::SettingsTabSelected(Some(title)) => {
                let chosen = SettingsTab::ALL.into_iter().find(|tab| tab.title() == title);
                if let Some(tab) = chosen.filter(|tab| *tab != self.settings_tab) {
                    self.settings_tab = tab;
                    if tab == SettingsTab::Input {
                        self.refresh_input_devices(context);
                    }
                }
            }
            Message::SettingsTabSelected(None) => {}
            other => return Some(other),
        }
        None
    }

    /// Reports a failed save of the game's settings.
    pub(in crate::reactor_app) fn report_save(&mut self, result: std::io::Result<()>) {
        if let Err(e) = result {
            self.dialog = Dialog::Error {
                message: t!("common.failed_to_save_game_settings", error = e).to_string(),
            };
        }
    }

    /// Reports a failure to move the settings `config.ini` kept for every game into the
    /// games' own files.
    pub(in crate::reactor_app) fn report_moved_settings(&mut self, result: std::io::Result<()>) {
        if let Err(e) = result {
            self.dialog = Dialog::Error {
                message: t!("app.failed_to_move_settings", error = e).to_string(),
            };
        }
    }
}

/// The settings a game's settings page offers, by its family.
fn settings_kind_of(game: &GameRecord) -> SettingsKind {
    games::family_of(game).ui_features().settings
}

/// A tab's sections, one under the other.
fn tab_content(children: Vec<KeyedView>) -> View {
    StackPanel::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .spacing(4.0)
        .keyed_children(children)
        .into()
}
