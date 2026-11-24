use windows_reactor::*;

use rust_i18n::t;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::Message;
use crate::reactor_app::widgets::{separator, settings_card, settings_row, settings_toggle, spawn_anim_tick};

/// The log levels offered: the value stored in the game's settings file and the text key of
/// its label.
pub(in crate::reactor_app) const SYS_LOGLEVELS: &[(Option<&str>, &str)] = &[
    (Some("info"), "page_system.log_level_info"),
    (Some("error"), "page_system.log_level_error"),
    (Some("warn"), "page_system.log_level_warn"),
    (Some("debug"), "page_system.log_level_debug"),
    (Some("trace"), "page_system.log_level_trace"),
];

/// The cabinet seats: the value stored in the game's settings file and the text key of its
/// label.
pub(in crate::reactor_app) const SYS_SEATS: &[(&str, &str)] = &[
    ("SINGLE", "page_system.seat_single"),
    ("A1", "page_system.seat_a1"),
    ("A2", "page_system.seat_a2"),
    ("B1", "page_system.seat_b1"),
    ("B2", "page_system.seat_b2"),
];

impl Launcher {

    /// Saves the system settings into the file of the game whose settings page is open.
    pub(in crate::reactor_app) fn save_system(&mut self) {
        if let Some(game) = self.settings_game() {
            let result = self.store.save_game_system(game, &self.system);
            self.report_save(result);
        }
    }

    /// Saves the coin settings into the file of the game whose settings page is open.
    pub(in crate::reactor_app) fn save_coin(&mut self) {
        if let Some(game) = self.settings_game() {
            let result = self.store.save_game_coin(game, &self.coin);
            self.report_save(result);
        }
    }

    /// The row choosing the loader's log level.
    fn log_level_row(&self, context: &mut ViewContext<Self>) -> View {
        let items: Vec<String> = SYS_LOGLEVELS.iter().map(|&(_, label)| t!(label).to_string()).collect();
        let selected = SYS_LOGLEVELS
            .iter()
            .position(|(v, _)| *v == self.system.loglevel.as_deref())
            .or(Some(0));
        settings_row(
            SymbolIcon::new().symbol(Symbol::List).into(),
            &t!("page_system.log_level"),
            ComboBox::new()
                .width(180.0)
                .items_source(items)
                .selected_index(selected)
                .on_selection_changed(context.callback(Message::LogLevelSelected))
                .vertical_alignment(VerticalAlignment::Center)
                .into(),
        )
    }

    /// The log level row on a card of its own, for the games whose System tab has no other
    /// setting of this page.
    pub(in crate::reactor_app) fn log_level_card(&self, context: &mut ViewContext<Self>) -> View {
        let card = settings_card(vec![KeyedView::new("log-level", self.log_level_row(context))]);
        StackPanel::new().margin(Thickness::new(0.0, 16.0, 0.0, 0.0)).children((card,)).into()
    }

    pub(in crate::reactor_app) fn toggle_coin_expander(&mut self, context: &ComponentContext<Self>) {
        self.coin_expanded = !self.coin_expanded;

        self.coin_anim_gen += 1;
        let (epoch, step) = (self.coin_anim_gen, 0.016 / 0.2);
        spawn_anim_tick(context, move || Message::CoinAnimTick(epoch, step));
    }

    pub(in crate::reactor_app) fn tick_coin_anim(&mut self, epoch: u32, step: f64, context: &ComponentContext<Self>) {
        if epoch != self.coin_anim_gen {
            return;
        }
        if self.coin_expanded {
            self.coin_anim = (self.coin_anim + step).min(1.0);
        } else {
            self.coin_anim = (self.coin_anim - step).max(0.0);
        }
        let done = if self.coin_expanded {
            self.coin_anim >= 1.0
        } else {
            self.coin_anim <= 0.0
        };
        if !done {
            spawn_anim_tick(context, move || Message::CoinAnimTick(epoch, step));
        }
    }

    /// The System tab of an Initial D game's settings page.
    pub(in crate::reactor_app) fn system_settings(&self, context: &mut ViewContext<Self>) -> View {
        let seat_items: Vec<String> = SYS_SEATS
            .iter()
            .map(|&(_, label)| t!(label).to_string())
            .collect();
        let seat_idx = SYS_SEATS
            .iter()
            .position(|(v, _)| Some(*v) == self.system.seat.as_deref())
            .or(Some(0));

        let card = Border::new()
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
                    self.log_level_row(context),
                    separator(),
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Contact).into(),
                        &t!("page_system.skip_reader_initialization"),
                        settings_toggle(
                            self.system.skip_reader_init == Some(true),
                            true,
                            context.callback(Message::SkipReaderInitToggled),
                        ),
                    ),
                    separator(),
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Import).into(),
                        &t!("page_system.auto_card_insert"),
                        settings_toggle(
                            self.system.auto_insert_card == Some(true),
                            true,
                            context.callback(Message::AutoInsertCardToggled),
                        ),
                    ),
                    separator(),
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Directions).into(),
                        &t!("page_system.seat_position"),
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(12.0)
                            .vertical_alignment(VerticalAlignment::Center)
                            .children((
                                CheckBox::new()
                                    .is_checked(
                                        self.system.master.unwrap_or(
                                            self.system.seat.as_deref().unwrap_or("SINGLE")
                                                == "SINGLE",
                                        ),
                                    )
                                    .on_is_checked_changed(
                                        context.callback(Message::SeatMasterToggled),
                                    )
                                    .content(t!("page_system.master").to_string()),
                                ComboBox::new()
                                    .width(140.0)
                                    .items_source(seat_items)
                                    .selected_index(seat_idx)
                                    .on_selection_changed(context.callback(Message::SeatSelected))
                                    .vertical_alignment(VerticalAlignment::Center),
                            ))
                            .into(),
                    ),
                )),
            );

        let coin = &self.coin;
        let coin_number_row =
            |idx: usize, icon: Symbol, title: &str, val: Option<u32>| -> View {
                settings_row(
                    SymbolIcon::new().symbol(icon).into(),
                    title,
                    NumberBox::new()
                        .width(160.0)
                        .minimum(0.0)
                        .maximum(99.0)
                        .value(val.map(|v| v as f64))
                        .on_value_changed(
                            context.callback(move |v| Message::CoinValueChanged(idx, v)),
                        )
                        .vertical_alignment(VerticalAlignment::Center)
                        .into(),
                )
            };

        let coin_header = Grid::new()
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .column_spacing(16.0)
            .children((
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(14.0)
                    .grid_column(0)
                    .children((
                        SymbolIcon::new().symbol(Symbol::Manage),
                        StackPanel::new().children([TextBlock::new()
                            .text(t!("page_system.coins"))
                            .font_size(14.0)
                            .font_weight(FontWeight::SEMI_BOLD)]),
                    )),
                FontIcon::new()
                    .glyph(if self.coin_expanded {
                        "\u{E70D}"
                    } else {
                        "\u{E76C}"
                    })
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(1),
            ));

        let mut coin_rows: Vec<KeyedView> = vec![KeyedView::new(
            "coin-header",
            Border::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .background(Color::transparent())
                .margin(Thickness::new(16.0, 12.0, 16.0, 12.0))
                .on_pointer_pressed(context.callback(move |_| Message::CoinExpanderToggled))
                .content(coin_header),
        )];

        if self.coin_anim > 0.0 || self.coin_expanded {

            let eased = {
                let t = self.coin_anim;
                t * (2.0 - t)
            };
            let items = StackPanel::new()
                .horizontal_alignment(HorizontalAlignment::Stretch)
                .opacity(eased)
                .keyed_children(vec![
                KeyedView::new(
                    "coin-freeplay",
                    settings_row(
                        SymbolIcon::new().symbol(Symbol::Like).into(),
                        &t!("page_system.free_play"),
                        settings_toggle(
                            coin.freeplay == Some(true),
                            true,
                            context.callback(Message::FreeplayToggled),
                        ),
                    ),
                ),
                KeyedView::new("coin-sep0", separator()),
                KeyedView::new(
                    "coin-start",
                    coin_number_row(
                        0,
                        Symbol::Play,
                        &t!("page_system.start_credits"),
                        coin.start_coin,
                    ),
                ),
                KeyedView::new("coin-sep1", separator()),
                KeyedView::new(
                    "coin-continue",
                    coin_number_row(
                        1,
                        Symbol::Redo,
                        &t!("page_system.continue_credits"),
                        coin.continue_coin,
                    ),
                ),
                KeyedView::new("coin-sep2", separator()),
                KeyedView::new(
                    "coin-card",
                    coin_number_row(
                        2,
                        Symbol::Shop,
                        &t!("page_system.card_purchase_credits"),
                        coin.card_coin,
                    ),
                ),
                KeyedView::new("coin-sep3", separator()),
                KeyedView::new(
                    "coin-car",
                    coin_number_row(
                        3,
                        Symbol::Directions,
                        &t!("page_system.car_purchase_credits"),
                        coin.car_coin,
                    ),
                ),
            ]);
            coin_rows.push(KeyedView::new("coin-content", items));
        }

        let coin_card = Border::new()
            .margin(Thickness::new(0.0, 8.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .keyed_children(coin_rows),
            );

        StackPanel::new()
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .spacing(4.0)
            .children((card, coin_card))
            .into()
    }

}
