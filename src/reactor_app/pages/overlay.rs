use std::path::Path;
use std::sync::Arc;

use windows_reactor::*;

use rust_i18n::t;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::Message;
use crate::reactor_app::widgets::{builtin_cover_name, platform_name};

const PANEL_COVER_W: f64 = 168.0;
const PANEL_COVER_H: f64 = 112.0;

const FALLBACK_WIN_W: f64 = 1180.0;
const FALLBACK_WIN_H: f64 = 720.0;

impl Launcher {

    pub(in crate::reactor_app) fn launch_overlay_view(
        &self,
        context: &mut ViewContext<Self>,
    ) -> View {
        let Some(index) = self.launch_overlay else {
            return Grid::new().into();
        };
        let Some(game) = self.library.games.get(index) else {
            return Grid::new().into();
        };

        let running = self.game_running
            && self.game_runtime.as_ref().map(|r| r.index) == Some(index);

        let win_w = if self.win_size.width > 0.0 { self.win_size.width } else { FALLBACK_WIN_W };
        let win_h = if self.win_size.height > 0.0 { self.win_size.height } else { FALLBACK_WIN_H };
        let panel_w = win_w;
        let panel_h = (win_h / 3.0).max(200.0);

        let cover: View = {
            let mut rendered: Option<View> = None;
            if let Some(path) = &game.image {
                if Path::new(path).is_file() {
                    if let Ok(img) = Image::new().source_file(path) {
                        rendered = Some(
                            img.width(PANEL_COVER_W)
                                .stretch(Stretch::Uniform)
                                .into(),
                        );
                    }
                }
            }
            if rendered.is_none() {

                let cover = builtin_cover_name(game);
                let big_key = cover.as_deref().map(|c| format!("{}_big.png", c.trim_end_matches(".png")));
                let bytes = big_key
                    .and_then(|k| Self::builtin_cover_bytes(&k))
                    .or_else(|| cover.as_deref().and_then(Self::builtin_cover_bytes));
                if let Some(bytes) = bytes {
                    rendered = Some(
                        Image::new()
                            .source_data(EncodedImage::new(Arc::<[u8]>::from(bytes)))
                            .width(PANEL_COVER_W)
                            .stretch(Stretch::Uniform)
                            .into(),
                    );
                }
            }
            rendered.unwrap_or_else(|| crate::reactor_app::widgets::fallback_cover(PANEL_COVER_W, PANEL_COVER_H))
        };

        let status_row: View = if running {
            TextBlock::new()
                .text(t!("page_overlay.game_running"))
                .font_size(14.0)
                .opacity(0.65)
                .horizontal_alignment(HorizontalAlignment::Center)
                .into()
        } else {
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(10.0)
                .horizontal_alignment(HorizontalAlignment::Center)
                .children((
                    ProgressRing::new()
                        .is_indeterminate(true)
                        .is_active(true)
                        .width(18.0)
                        .height(18.0),
                    TextBlock::new()
                        .text(t!("page_overlay.starting_game"))
                        .font_size(14.0)
                        .opacity(0.65)
                        .vertical_alignment(VerticalAlignment::Center),
                ))
                .into()
        };

        let platform_label = platform_name(game);
        let title_row = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(20.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .children((
                Border::new()
                    .corner_radius(8.0)
                    .content(cover),
                StackPanel::new()
                    .spacing(6.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((
                        TextBlock::new()
                            .text(game.name.clone())
                            .font_size(22.0)
                            .font_weight(FontWeight::SEMI_BOLD)
                            .text_wrapping(TextWrapping::Wrap)
                            .max_lines(2),
                        TextBlock::new()
                            .text(platform_label)
                            .font_size(13.0)
                            .opacity(0.55),
                    )),
            ));

        let close_button = Button::new()
            .style(ButtonStyle::Accent)
            .min_width(160.0)
            .min_height(36.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .on_click(context.message(Message::LaunchStop(index)))
            .content(t!("page_overlay.close_game").to_string());

        let panel_fill = if self.light_theme {
            Color::argb(0xE8, 0xF3, 0xF3, 0xF3)
        } else {
            Color::argb(0xE8, 0x2B, 0x2B, 0x2B)
        };

        let action_row: View = if running {
            close_button
        } else {
            Grid::new().into()
        };

        let panel_body = StackPanel::new()
            .spacing(22.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((title_row, status_row, action_row));

        let panel = Border::new()
            .width(panel_w)
            .height(panel_h)
            .background(panel_fill)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .content(panel_body);

        Grid::new()
            .background(Color::argb(0x99, 0x00, 0x00, 0x00))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .grid_row_span(3)
            .children((panel,))
            .into()
    }
}

impl Launcher {

    pub(in crate::reactor_app) fn room_overlay_view(&self) -> View {
        let Some(text) = &self.room_overlay else {
            return Grid::new().into();
        };

        let win_w = if self.win_size.width > 0.0 { self.win_size.width } else { FALLBACK_WIN_W };
        let win_h = if self.win_size.height > 0.0 { self.win_size.height } else { FALLBACK_WIN_H };
        let panel_w = win_w;
        let panel_h = (win_h / 3.0).max(200.0);

        let status_row = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(10.0)
            .horizontal_alignment(HorizontalAlignment::Center)
            .children((
                ProgressRing::new()
                    .is_indeterminate(true)
                    .is_active(true)
                    .width(18.0)
                    .height(18.0),
                TextBlock::new()
                    .text(text.clone())
                    .font_size(14.0)
                    .opacity(0.65)
                    .vertical_alignment(VerticalAlignment::Center),
            ));

        let panel_fill = if self.light_theme {
            Color::argb(0xE8, 0xF3, 0xF3, 0xF3)
        } else {
            Color::argb(0xE8, 0x2B, 0x2B, 0x2B)
        };

        let panel = Border::new()
            .width(panel_w)
            .height(panel_h)
            .background(panel_fill)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(Thickness::uniform(1.0))
            .content(
                StackPanel::new()
                    .spacing(22.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .children((status_row,)),
            );

        Grid::new()
            .background(Color::argb(0x99, 0x00, 0x00, 0x00))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .vertical_alignment(VerticalAlignment::Stretch)
            .grid_row_span(3)
            .children((panel,))
            .into()
    }
}
