use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use rust_i18n::t;
use windows_reactor::*;

use crate::cards::{self, CardEntry, CardScan};
use crate::games::wmmt3::{self, BuiltInCardReader};
use crate::games::{CardKind, family_of, game_dir_of};
use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::state::{CardBaseCache, CardImage};
use crate::reactor_app::widgets::placeholder_page;
use crate::storage::GameRecord;

pub(in crate::reactor_app) const CARD_BASE_W: f64 = 698.0;
pub(in crate::reactor_app) const CARD_BASE_H: f64 = 441.0;

pub(in crate::reactor_app) const CARD_W: f64 = 380.0;

pub(in crate::reactor_app) const CARD_DISPLAY_SCALE: f64 = 0.8;

pub(in crate::reactor_app) const CARD_GAP: f64 = 24.0;
pub(in crate::reactor_app) const CARD_ROW_SPACING: f64 = 16.0;

pub(in crate::reactor_app) const CARD_PAGE_MARGIN: f64 = 28.0;

pub(in crate::reactor_app) const CARD_SCROLL_RESERVE: f64 = 20.0;

pub(in crate::reactor_app) const CARD_ACTIVE_TINT_OPACITY: f64 = 0.14;

pub(in crate::reactor_app) const CARD_BASE_PNG: &[u8] = include_bytes!("../../assets/id5_base.png");

pub(in crate::reactor_app) const AIME_BASE_PNG: &[u8] = include_bytes!("../../assets/aime.png");
pub(in crate::reactor_app) const AIME_BASE_W: f64 = 682.0;
pub(in crate::reactor_app) const AIME_BASE_H: f64 = 432.0;

pub(in crate::reactor_app) const ID8_BASE_PNG: &[u8] = include_bytes!("../../assets/id8_base.png");
pub(in crate::reactor_app) const ID8_BASE_W: f64 = 698.0;
pub(in crate::reactor_app) const ID8_BASE_H: f64 = 440.0;

pub(in crate::reactor_app) const ID4_BASE_PNG: &[u8] = include_bytes!("../../assets/id4_base.png");
pub(in crate::reactor_app) const ID4_BASE_W: f64 = 698.0;
pub(in crate::reactor_app) const ID4_BASE_H: f64 = 441.0;

pub(in crate::reactor_app) const ID6_BASE_PNG: &[u8] = include_bytes!("../../assets/id6_base.png");
pub(in crate::reactor_app) const ID6_BASE_W: f64 = 698.0;
pub(in crate::reactor_app) const ID6_BASE_H: f64 = 440.0;

const WMMT3_CARD_W: f64 = 156.0;
const WMMT3_CARD_H: f64 = WMMT3_CARD_W * 1019.0 / 640.0;

pub(in crate::reactor_app) const COVER_ASSETS: &[(&str, &[u8])] = &[
    ("logo_D4.png", include_bytes!("../../assets/logo_D4.png")),
    ("logo_D4kai.png", include_bytes!("../../assets/logo_D4kai.png")),
    ("logo_D5.png", include_bytes!("../../assets/logo_D5.png")),
    ("logo_D6.png", include_bytes!("../../assets/logo_D6.png")),
    (
        "logo_D6chn.png",
        include_bytes!("../../assets/logo_D6chn.png"),
    ),
    ("logo_D7.png", include_bytes!("../../assets/logo_D7.png")),
    ("logo_D7chn.png", include_bytes!("../../assets/logo_D7chn.png")),
    ("logo_D8.png", include_bytes!("../../assets/logo_D8.png")),
    ("logo_dac.png", include_bytes!("../../assets/logo_dac.png")),
    ("logo_idz.png", include_bytes!("../../assets/logo_idz.png")),
    ("logo_wm3.png", include_bytes!("../../assets/logo_wm3.png")),
    ("logo_wm3dx.png", include_bytes!("../../assets/logo_wm3dx.png")),
    ("logo_wm3dxplus.png", include_bytes!("../../assets/logo_wm3dxplus.png")),

    ("logo_D4_big.png", include_bytes!("../../assets/logo_D4_big.png")),
    ("logo_D4kai_big.png", include_bytes!("../../assets/logo_D4kai_big.png")),
    ("logo_D5_big.png", include_bytes!("../../assets/logo_D5_big.png")),
    ("logo_D6_big.png", include_bytes!("../../assets/logo_D6_big.png")),
    ("logo_D6chn_big.png", include_bytes!("../../assets/logo_D6chn_big.png")),
    ("logo_D7_big.png", include_bytes!("../../assets/logo_D7_big.png")),
    ("logo_D7chn_big.png", include_bytes!("../../assets/logo_D7chn_big.png")),
    ("logo_D8_big.png", include_bytes!("../../assets/logo_D8_big.png")),
    ("logo_dac_big.png", include_bytes!("../../assets/logo_dac_big.png")),
    ("logo_idz_big.png", include_bytes!("../../assets/logo_idz_big.png")),
    ("logo_wm3_big.png", include_bytes!("../../assets/logo_wm3_big.png")),
    ("logo_wm3dx_big.png", include_bytes!("../../assets/logo_wm3dx_big.png")),
    ("logo_wm3dxplus_big.png", include_bytes!("../../assets/logo_wm3dxplus_big.png")),
];

pub(in crate::reactor_app) const CARD_CAR_OFFSET_X: i64 = 0;
pub(in crate::reactor_app) const CARD_CAR_OFFSET_Y: i64 = 4;

impl Launcher {

    pub(in crate::reactor_app) fn cards_page(&self, context: &mut ViewContext<Self>) -> View {

        let seq = self.card_enter_seq;
        let sender = context.sender();
        context.use_effect("card-enter-anim", seq, move || {
            let _ = sender.send(Message::CardSlideTick(0.0));
            None
        });
        let Some(gi) = self.cards_game else {
            return placeholder_page(&t!("common.card_select"), &t!("page_cards.go_to_games_page_and"));
        };
        let Some(game) = self.library.games.get(gi) else {
            return placeholder_page(&t!("common.card_select"), &t!("page_cards.game_no_longer_exists"));
        };
        let _ = game;
        let aime_mode = self.card_kind == CardKind::Aime;

        let aime_item_w = CARD_W * CARD_DISPLAY_SCALE;
        let aime_item_h = aime_item_w * AIME_BASE_H / AIME_BASE_W;

        let wrap_cards = |items: Vec<KeyedView>, cols: usize| -> View {
            let rows: Vec<KeyedView> = items
                .chunks(cols)
                .enumerate()
                .map(|(row, chunk)| {
                    KeyedView::new(
                        format!("card-row-{row}"),
                        StackPanel::new()
                            .orientation(Orientation::Horizontal)
                            .spacing(CARD_GAP)
                            .margin(Thickness::new(0.0, 0.0, 0.0, CARD_ROW_SPACING))
                            .keyed_children(chunk.to_vec()),
                    )
                })
                .collect();
            ScrollViewer::new()
                .margin(Thickness::new(0.0, 12.0, 0.0, 0.0))
                .vertical_scroll_bar_visibility(ScrollBarVisibility::Visible)

                .horizontal_scroll_bar_visibility(ScrollBarVisibility::Auto)

                .content(
                    ItemsRepeater::new()
                        .items(rows),
                )
                .into()
        };

        let body: View = if self.cards_loading {

            StackPanel::new()
                .spacing(12.0)
                .margin(Thickness::new(0.0, 48.0, 0.0, 0.0))
                .horizontal_alignment(HorizontalAlignment::Center)
                .children((
                    ProgressRing::new()
                        .is_indeterminate(true)
                        .is_active(true)
                        .width(40.0)
                        .height(40.0)
                        .horizontal_alignment(HorizontalAlignment::Center),
                    TextBlock::new()
                        .text(t!("page_cards.loading_cards"))
                        .font_size(14.0)
                        .opacity(0.7)
                        .horizontal_alignment(HorizontalAlignment::Center),
                ))
                .into()
        } else if aime_mode {

            let cols = self.card_columns(aime_item_w);
            let mut items: Vec<KeyedView> = vec![KeyedView::new(
                "aime-create",
                self.aime_create_tile(context, aime_item_w, aime_item_h),
            )];
            for (i, c) in self.cards.cards.iter().enumerate() {
                items.push(KeyedView::new(
                    format!("aime-{i}"),
                    self.aime_item(context, i, c, aime_item_w, aime_item_h),
                ));
            }
            wrap_cards(items, cols)
        } else if self.card_kind == CardKind::Wmmt3 {
            let blank_active = self.cards.active.is_some() && self.cards.active_index().is_none();
            let mut items = vec![KeyedView::new(
                "blank-card",
                self.blank_card_tile(context, WMMT3_CARD_W, WMMT3_CARD_H, blank_active),
            )];
            for (index, card) in self.cards.cards.iter().enumerate() {
                items.push(KeyedView::new(
                    card.name.clone(),
                    self.wmmt3_card_item(context, index, card),
                ));
            }
            wrap_cards(items, self.card_columns(WMMT3_CARD_W))
        } else {

            let mut failed = 0usize;
            let mut first_reason = String::new();
            for card in &self.cards.cards {
                if let Err(reason) = self.card_image(&card.path) {
                    failed += 1;
                    if first_reason.is_empty() {
                        first_reason = reason.to_string();
                    }
                }
            }
            if failed > 0 {
                let summary = if failed == 1 {
                    t!("page_cards.card_cannot_be_displayed", first_reason = first_reason).to_string()
                } else {
                    t!("page_cards.cards_cannot_be_displayed", failed = failed, first_reason = first_reason).to_string()
                };

                let sender = context.sender();
                context.use_effect(
                    "card-compose-error",
                    (self.cards_game, summary.clone()),
                    move || {
                        let _ = sender.send(Message::CardComposeFailed(summary));
                        None
                    },
                );
            }

            let (bw, bh) = if self.card_kind == CardKind::Id4 {
                (ID4_BASE_W, ID4_BASE_H)
            } else if self.card_kind == CardKind::Id6 {
                (ID6_BASE_W, ID6_BASE_H)
            } else if self.card_kind == CardKind::Id8 {
                (ID8_BASE_W, ID8_BASE_H)
            } else {
                (CARD_BASE_W, CARD_BASE_H)
            };
            let s = self.dpi_scale.max(0.01);
            let blank_w = bw / s * CARD_DISPLAY_SCALE;
            let blank_h = bh / s * CARD_DISPLAY_SCALE;
            let blank_active = self
                .cards
                .active
                .as_deref()
                .is_some_and(|a| a.starts_with("InidCrd") && a.ends_with(".png"));

            let cols = self.card_columns(self.card_display_w());
            let mut items: Vec<KeyedView> = vec![KeyedView::new(
                "blank-card",
                self.blank_card_tile(context, blank_w, blank_h, blank_active),
            )];
            for (i, c) in self.cards.cards.iter().enumerate() {
                items.push(KeyedView::new(
                    format!("card-{i}"),
                    self.card_item(context, i, c),
                ));
            }
            wrap_cards(items, cols)
        };

        let active = self.card_slide;

        let title: View = TextBlock::new()
            .text(t!("common.card_select"))
            .font_size(26.0)
            .font_weight(FontWeight::SEMI_BOLD)
            .into();
        let content = Grid::new()
            .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
            .rows([GridLength::Auto, GridLength::Star(1.0)])
            .row_spacing(8.0)
            .children((
                Grid::new()
                    .columns([GridLength::Auto, GridLength::Star(1.0)])
                    .column_spacing(16.0)
                    .grid_row(0)
                    .children((
                        Button::new()
                            .on_click(context.message(Message::BackToGames))
                            .vertical_alignment(VerticalAlignment::Center)
                            .grid_column(0)
                            .content(SymbolIcon::new().symbol(Symbol::Back)),
                        StackPanel::new()
                            .spacing(4.0)
                            .margin(Thickness::new(8.0, 0.0, 0.0, 0.0))
                            .grid_column(1)
                            .children((title,)),
                    )),
                Border::new().grid_row(1).content(body),
            ));

        let slide = active;
        Border::new()
            .opacity(slide)
            .margin(Thickness::new((1.0 - slide) * 80.0, 0.0, 0.0, 0.0))
            .background(ThemeBrush::CardBackground)
            .content(content)
            .into()
    }

    fn blank_card_tile(
        &self,
        context: &mut ViewContext<Self>,
        w: f64,
        h: f64,
        active: bool,
    ) -> View {
        let mut labels = vec![KeyedView::new(
            "title",
            TextBlock::new()
                .text(t!("page_cards.blank_card"))
                .font_size(15.0)
                .font_weight(FontWeight::SEMI_BOLD)
                .horizontal_alignment(HorizontalAlignment::Center),
        )];
        if self.card_kind == CardKind::Wmmt3 {
            labels.push(KeyedView::new(
                "hint",
                TextBlock::new()
                    .text(t!("page_cards.wmmt3_blank_hint"))
                    .font_size(12.0)
                    .text_wrapping(TextWrapping::Wrap),
            ));
        }
        Border::new()
            .width(w)
            .height(h)
            .corner_radius(6.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(if active {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardStroke
            })
            .border_thickness(if active { 2.0 } else { 1.0 })
            .on_pointer_pressed(context.callback(|_| Message::BlankCardSelect))
            .content(
                StackPanel::new()
                    .margin(Thickness::new(10.0, 0.0, 10.0, 0.0))
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
                    .spacing(6.0)
                    .keyed_children(labels),
            )
            .into()
    }

    fn aime_base_arc() -> Arc<[u8]> {
        static AIME: OnceLock<Arc<[u8]>> = OnceLock::new();
        AIME.get_or_init(|| Arc::from(AIME_BASE_PNG.to_vec())).clone()
    }

    fn aime_create_tile(&self, context: &mut ViewContext<Self>, w: f64, h: f64) -> View {
        Border::new()
            .width(w)
            .height(h)
            .corner_radius(6.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .on_pointer_pressed(context.callback(|_| Message::AimeCardCreate))
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center)
                    .spacing(6.0)
                    .children((
                        TextBlock::new().text("＋").font_size(30.0).opacity(0.7),
                        TextBlock::new()
                            .text(t!("page_cards.new_card"))
                            .font_size(13.0),
                    )),
            )
            .into()
    }

    fn aime_item(
        &self,
        context: &mut ViewContext<Self>,
        index: usize,
        card: &CardEntry,
        w: f64,
        h: f64,
    ) -> View {
        let active = self.cards.active_index() == Some(index);

        let content_view: View = Grid::new()
            .children((
                Image::new()
                    .source_data(EncodedImage::new(Self::aime_base_arc()))
                    .width(w)
                    .height(h)
                    .stretch(Stretch::Uniform),
                TextBlock::new()
                    .text(cards::format_access_code(&card.name))
                    .font_size(14.0)
                    .font_weight(FontWeight::SEMI_BOLD)

                    .foreground(Brush::Solid(Color::argb(255, 0x1A, 0x1A, 0x1A)))
                    .margin(Thickness::new(14.0, 10.0, 0.0, 0.0))
                    .horizontal_alignment(HorizontalAlignment::Left)
                    .vertical_alignment(VerticalAlignment::Top),
            ))
            .into();

        let content_view: View = if active {
            let tint: View = Border::new()
                .width(w)
                .height(h)
                .corner_radius(6.0)
                .background(ThemeBrush::Accent)
                .opacity(CARD_ACTIVE_TINT_OPACITY)
                .into();
            Grid::new().children((content_view, tint)).into()
        } else {
            content_view
        };

        Border::new()
            .width(w)
            .height(h)
            .corner_radius(6.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(if active {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardStroke
            })
            .border_thickness(if active { 2.0 } else { 1.0 })
            .on_pointer_pressed(context.callback(move |_| Message::CardClicked(index)))
            .content(content_view)
            .into()
    }

    pub(in crate::reactor_app) fn aime_device_dir(&self) -> Option<PathBuf> {
        let amdaemon = Path::new(self.card_game()?.amdaemon.as_deref()?);
        Some(amdaemon.parent()?.join("DEVICE"))
    }

    pub(in crate::reactor_app) fn builtin_cover_bytes(name: &str) -> Option<&'static [u8]> {
        COVER_ASSETS
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, bytes)| *bytes)
    }

    fn ensure_card_base(&self) {
        if self.base_cache.borrow().is_some() {
            return;
        }
        let png = if self.card_kind == CardKind::Id4 {
            ID4_BASE_PNG
        } else if self.card_kind == CardKind::Id6 {
            ID6_BASE_PNG
        } else if self.card_kind == CardKind::Id8 {
            ID8_BASE_PNG
        } else {
            CARD_BASE_PNG
        };
        *self.base_cache.borrow_mut() = Some(CardBaseCache {
            result: Self::decode_builtin_base(png),
        });
    }

    fn decode_builtin_base(png: &[u8]) -> Result<image::RgbaImage, String> {
        image::load_from_memory(png)
            .map(|img| img.to_rgba8())
            .map_err(|e| t!("page_cards.failed_to_decode_built_in", error = e).to_string())
    }

    fn card_car_offset(&self) -> (i64, i64) {
        if self.card_kind == CardKind::Id4 {
            (-5, CARD_CAR_OFFSET_Y)
        } else {
            (CARD_CAR_OFFSET_X, CARD_CAR_OFFSET_Y)
        }
    }

    fn overlay_car(
        mut base: image::RgbaImage,
        car: &image::RgbaImage,
        dpi_scale: f64,
        offset_x: i64,
        offset_y: i64,
    ) -> image::RgbaImage {

        let scale = dpi_scale.max(0.01);
        let x = (base.width() as i64 - car.width() as i64) / 2
            + (offset_x as f64 * scale) as i64;
        let y = (base.height() as i64 - car.height() as i64) / 2
            + (offset_y as f64 * scale) as i64;
        image::imageops::overlay(&mut base, car, x, y);
        base
    }

    fn card_image(&self, path: &Path) -> Result<CardImage, Arc<str>> {
        if let Some(cached) = self.card_image_cache.borrow().get(path) {
            return cached.clone();
        }

        let result = match self.card_kind {
            CardKind::Wmmt3 => load_wmmt3_card_image(path),
            _ => self.compose_initial_d_card_image(path),
        }
        .map_err(Arc::from);
        self.card_image_cache
            .borrow_mut()
            .insert(path.to_path_buf(), result.clone());
        result
    }

    fn compose_initial_d_card_image(&self, car_path: &Path) -> Result<CardImage, String> {
        self.ensure_card_base();
        let base = match self.base_cache.borrow().as_ref().map(|c| &c.result) {
            Some(Ok(img)) => img.clone(),
            Some(Err(e)) => return Err(t!("page_cards.base_image_load_failed", error = e).to_string()),
            None => return Err(t!("page_cards.base_image_not_loaded").to_string()),
        };
        let (canvas_w, canvas_h) = (base.width(), base.height());

        let car = image::open(car_path)
            .map_err(|e| t!("page_cards.card_image_decode_failed", error = e).to_string())?
            .to_rgba8();

        let (ox, oy) = self.card_car_offset();
        let composed = Self::overlay_car(base, &car, self.dpi_scale, ox, oy);

        let mut out = Cursor::new(Vec::new());
        let encoder = image::codecs::png::PngEncoder::new_with_quality(
            &mut out,
            image::codecs::png::CompressionType::Fast,
            image::codecs::png::FilterType::NoFilter,
        );
        composed
            .write_with_encoder(encoder)
            .map_err(|e| t!("page_cards.composite_image_encode_failed", error = e).to_string())?;
        Ok(CardImage {
            bytes: out.into_inner().into(),
            width: f64::from(canvas_w),
            height: f64::from(canvas_h),
        })
    }

    fn card_display_w(&self) -> f64 {
        let s = self.dpi_scale.max(0.01);
        self.cards
            .cards
            .first()
            .and_then(|card| {
                let cache = self.card_image_cache.borrow();
                let image = cache.get(&card.path)?.as_ref().ok()?;
                Some(image.width / s * CARD_DISPLAY_SCALE)
            })
            .unwrap_or(CARD_W * CARD_DISPLAY_SCALE)
    }

    fn card_columns(&self, card_w: f64) -> usize {
        let avail = self.win_size.width - CARD_PAGE_MARGIN * 2.0 - CARD_SCROLL_RESERVE;

        if avail <= 0.0 || card_w <= 0.0 {
            return 2;
        }
        ((avail + CARD_GAP) / (card_w + CARD_GAP))
            .floor()
            .max(1.0) as usize
    }

    fn card_fallback_size(&self) -> (f64, f64) {
        let (bw, bh) = if self.card_kind == CardKind::Id4 {
            (ID4_BASE_W, ID4_BASE_H)
        } else if self.card_kind == CardKind::Id6 {
            (ID6_BASE_W, ID6_BASE_H)
        } else if self.card_kind == CardKind::Id8 {
            (ID8_BASE_W, ID8_BASE_H)
        } else {
            (CARD_BASE_W, CARD_BASE_H)
        };
        (
            CARD_W * CARD_DISPLAY_SCALE,
            CARD_W * bh / bw * CARD_DISPLAY_SCALE,
        )
    }

    fn card_item(&self, context: &mut ViewContext<Self>, index: usize, card: &CardEntry) -> View {
        let (fallback_w, fallback_h) = self.card_fallback_size();

        let s = self.dpi_scale.max(0.01);
        let (content_view, card_w, card_h): (View, f64, f64) =
            match self.card_image(&card.path) {
                Ok(image) => {
                    let cw = image.width / s * CARD_DISPLAY_SCALE;
                    let ch = image.height / s * CARD_DISPLAY_SCALE;
                    (
                        Image::new()
                            .source_data(EncodedImage::new(image.bytes))
                            .width(cw)
                            .height(ch)
                            .stretch(Stretch::Uniform)
                            .into(),
                        cw,
                        ch,
                    )
                }
                Err(reason) => (

                    Border::new()
                        .width(fallback_w)
                        .height(fallback_h)
                        .padding(Thickness::new(10.0, 10.0, 10.0, 10.0))
                        .background(ThemeBrush::CardStroke)
                        .content(
                            TextBlock::new()
                                .text(t!("page_cards.cannot_display", reason = reason).to_string())
                                .font_size(11.0)
                                .opacity(0.85)
                                .text_wrapping(TextWrapping::Wrap)
                                .text_trimming(TextTrimming::CharacterEllipsis)
                                .max_lines(4)
                                .horizontal_alignment(HorizontalAlignment::Center)
                                .vertical_alignment(VerticalAlignment::Center),
                        )
                        .into(),
                    fallback_w,
                    fallback_h,
                ),
            };

        self.card_tile(context, index, card, content_view, (card_w, card_h))
    }

    fn wmmt3_card_item(&self, context: &mut ViewContext<Self>, index: usize, card: &CardEntry) -> View {
        let content = match self.card_image(&card.path) {
            Ok(image) => Border::new()
                .background(Brush::Solid(Color::argb(255, 0xFA, 0xF8, 0xF2)))
                .corner_radius(6.0)
                .content(
                    Image::new()
                        .source_data(EncodedImage::new(image.bytes))
                        .width(WMMT3_CARD_W)
                        .height(WMMT3_CARD_H)
                        .stretch(Stretch::Uniform),
                )
                .into(),
            Err(_) => Grid::new()
                .children((SymbolIcon::new()
                    .symbol(Symbol::Contact)
                    .horizontal_alignment(HorizontalAlignment::Center)
                    .vertical_alignment(VerticalAlignment::Center),))
                .into(),
        };
        self.card_tile(context, index, card, content, (WMMT3_CARD_W, WMMT3_CARD_H))
    }

    fn card_tile(
        &self,
        context: &mut ViewContext<Self>,
        index: usize,
        card: &CardEntry,
        content_view: View,
        (card_w, card_h): (f64, f64),
    ) -> View {
        let active = self.cards.active_index() == Some(index);
        let content_view: View = if active {
            let tint: View = Border::new()
                .width(card_w)
                .height(card_h)
                .corner_radius(6.0)
                .background(ThemeBrush::Accent)
                .opacity(CARD_ACTIVE_TINT_OPACITY)
                .into();
            Grid::new().children((content_view, tint)).into()
        } else {
            content_view
        };

        let composed = Border::new()
            .width(card_w)
            .height(card_h)
            .corner_radius(6.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(if active {
                ThemeBrush::Accent
            } else {
                ThemeBrush::CardStroke
            })
            .border_thickness(if active { 2.0 } else { 1.0 })
            .on_pointer_pressed(context.callback(move |_| Message::CardClicked(index)))
            .content(content_view);

        let name_label: View = if active {
            TextBlock::new()
                .text(t!("page_cards.name_in_use", name = card.name).to_string())
                .font_size(12.0)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .font_weight(FontWeight::SEMI_BOLD)
                .horizontal_alignment(HorizontalAlignment::Center)
                .into()
        } else {
            TextBlock::new()
                .text(card.name.clone())
                .font_size(12.0)
                .text_trimming(TextTrimming::CharacterEllipsis)
                .opacity(0.7)
                .horizontal_alignment(HorizontalAlignment::Center)
                .into()
        };

        StackPanel::new()
            .spacing(6.0)
            .width(card_w)
            .children((composed, name_label))
            .into()
    }

    pub(in crate::reactor_app) fn selected_card(&self, game: &GameRecord) -> Option<String> {
        game.card.clone().or_else(|| match family_of(game).card_kind(game) {
            CardKind::Wmmt3 => None,
            _ => self.store.load_card(),
        })
    }

    fn card_game(&self) -> Option<&GameRecord> {
        self.library.games.get(self.cards_game?)
    }

    pub(in crate::reactor_app) fn is_current_card_page(&self, seq: u32) -> bool {
        self.nav_tag == "cards" && seq == self.card_enter_seq
    }

    pub(in crate::reactor_app) fn select_card(&mut self, name: String) {
        let Some(game) = self.cards_game.and_then(|index| self.library.games.get_mut(index)) else {
            self.dialog = Dialog::Error {
                message: t!("app.failed_to_save_card_selection").to_string(),
            };
            return;
        };
        game.card = Some(name.clone());
        self.store.save_library(&self.library);
        self.cards.active = Some(name);
    }

    pub(in crate::reactor_app) fn select_blank_card(&mut self) {
        let Some(game) = self.card_game() else {
            return;
        };
        let name = match self.card_kind {
            CardKind::Wmmt3 => {
                let Some(reader) = BuiltInCardReader::load(&game_dir_of(game)) else {
                    self.dialog = Dialog::Error {
                        message: t!("page_cards.wmmt3_external_reader").to_string(),
                    };
                    return;
                };
                reader.new_card_name()
            }
            CardKind::Aime => return,
            _ => cards::blank_card_name(),
        };
        self.select_card(name);
    }

    pub(in crate::reactor_app) fn enter_cards(
        &mut self,
        index: usize,
        context: &ComponentContext<Self>,
    ) {
        let Some(game) = self.library.games.get(index) else {
            return;
        };
        let path = PathBuf::from(&game.path);
        let kind = family_of(game).card_kind(game);
        let include_wmmt2_cards = wmmt3::takes_over_wmmt2_cards(game);
        let wmmt3_reader = if kind == CardKind::Wmmt3 {
            let Some(reader) = BuiltInCardReader::load(&game_dir_of(game)) else {
                self.dialog = Dialog::Error {
                    message: t!("page_cards.wmmt3_external_reader").to_string(),
                };
                return;
            };
            Some(reader)
        } else {
            None
        };

        self.cards_game = Some(index);
        self.card_kind = kind;
        self.nav_tag = "cards".to_string();
        self.card_slide = 0.0;
        self.card_enter_seq += 1;
        let seq = self.card_enter_seq;
        self.cards = CardScan::default();

        self.card_image_cache.borrow_mut().clear();
        *self.base_cache.borrow_mut() = None;

        self.cards_loading = true;
        self.cards_warm = 0;

        let aime_dir = self.aime_device_dir();
        context.spawn_background(move |_| {
            let scan = match (kind, wmmt3_reader) {
                (_, Some(reader)) => reader.scan(include_wmmt2_cards),
                (CardKind::Aime, None) => {
                    aime_dir.map_or_else(CardScan::default, |dir| cards::scan_aime_cards_in(&dir))
                }
                (_, None) => cards::scan_cards(&path),
            };
            Message::CardsScanned(seq, Box::new(scan))
        });
    }

    pub(in crate::reactor_app) fn apply_cards_scanned(
        &mut self,
        mut scan: CardScan,
        context: &ComponentContext<Self>,
    ) {
        let selected = self.card_game().and_then(|game| self.selected_card(game));
        scan.active = selected.or(scan.active);
        self.cards = scan;

        if !matches!(self.card_kind, CardKind::Aime | CardKind::Wmmt3)
            && self.cards.cards.is_empty()
            && self.cards.active.is_none()
        {
            self.select_card(cards::blank_card_name());
        }

        self.cards_loading = self.card_kind != CardKind::Aime && !self.cards.cards.is_empty();
        self.cards_warm = 0;
        if self.cards_loading {
            self.schedule_card_cache_tick(context, Duration::from_millis(30));
        }
    }

    pub(in crate::reactor_app) fn warm_card_cache(&mut self, context: &ComponentContext<Self>) {
        if !self.cards_loading {
            return;
        }

        let len = self.cards.cards.len();
        let end = (self.cards_warm + 2).min(len);
        for i in self.cards_warm..end {
            let path = self.cards.cards[i].path.clone();
            let _ = self.card_image(&path);
        }
        self.cards_warm = end;
        if end >= len {
            self.cards_loading = false;
        } else {
            self.schedule_card_cache_tick(context, Duration::from_millis(1));
        }
    }

    fn schedule_card_cache_tick(&self, context: &ComponentContext<Self>, delay: Duration) {
        let seq = self.card_enter_seq;
        context.spawn_background(move |_| {
            std::thread::sleep(delay);
            Message::CardsLoadTick(seq)
        });
    }

    pub(in crate::reactor_app) fn reload_aime_cards(&mut self) {
        let mut scan = match self.aime_device_dir() {
            Some(d) => cards::scan_aime_cards_in(&d),
            None => CardScan::default(),
        };

        scan.active = self.card_game().and_then(|game| self.selected_card(game));
        self.cards = scan;
    }
}

fn load_wmmt3_card_image(card_path: &Path) -> Result<CardImage, String> {
    let mut preview = card_path.as_os_str().to_os_string();
    preview.push(".png");
    let bytes = std::fs::read(preview).map_err(|error| error.to_string())?;
    let reader = image::ImageReader::with_format(Cursor::new(&bytes), image::ImageFormat::Png);
    let (width, height) = reader.into_dimensions().map_err(|error| error.to_string())?;
    Ok(CardImage {
        bytes: bytes.into(),
        width: f64::from(width),
        height: f64::from(height),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_card_base_matches_constants() {
        let img = image::load_from_memory(CARD_BASE_PNG).expect("the built-in base image should decode");
        assert_eq!(img.width(), CARD_BASE_W as u32, "the base image width does not match the constant");
        assert_eq!(img.height(), CARD_BASE_H as u32, "the base image height does not match the constant");
    }

    #[test]
    fn id8_card_base_matches_constants() {
        let img = image::load_from_memory(ID8_BASE_PNG).expect("the ID8 base image should decode");
        assert_eq!(img.width(), ID8_BASE_W as u32, "the ID8 base image width does not match the constant");
        assert_eq!(img.height(), ID8_BASE_H as u32, "the ID8 base image height does not match the constant");
    }

    #[test]
    fn id4_card_base_matches_constants() {
        let img = image::load_from_memory(ID4_BASE_PNG).expect("the ID4 base image should decode");
        assert_eq!(img.width(), ID4_BASE_W as u32, "the ID4 base image width does not match the constant");
        assert_eq!(img.height(), ID4_BASE_H as u32, "the ID4 base image height does not match the constant");
    }

    #[test]
    fn id6_card_base_matches_constants() {
        let img = image::load_from_memory(ID6_BASE_PNG).expect("the ID6 base image should decode");
        assert_eq!(img.width(), ID6_BASE_W as u32, "the ID6 base image width does not match the constant");
        assert_eq!(img.height(), ID6_BASE_H as u32, "the ID6 base image height does not match the constant");
    }

    #[test]
    fn every_db_cover_is_embedded() {
        for name in crate::db::cover_images() {
            assert!(
                Launcher::builtin_cover_bytes(&name).is_some(),
                "games_db.json refers to the cover {name}, but COVER_ASSETS does not list that image"
            );
        }
    }

    #[test]
    fn car_is_centered_with_offset() {
        let base = image::RgbaImage::from_pixel(
            CARD_BASE_W as u32,
            CARD_BASE_H as u32,
            image::Rgba([255, 0, 0, 255]),
        );
        let car = image::RgbaImage::from_pixel(100, 100, image::Rgba([0, 0, 255, 255]));
        let out = Launcher::overlay_car(base, &car, 1.0, CARD_CAR_OFFSET_X, CARD_CAR_OFFSET_Y);

        let x = (CARD_BASE_W as i64 - 100) / 2;
        let y = (CARD_BASE_H as i64 - 100) / 2 + CARD_CAR_OFFSET_Y;

        assert_eq!(
            out.get_pixel(x as u32, y as u32),
            &image::Rgba([0, 0, 255, 255])
        );

        assert_eq!(
            out.get_pixel(x as u32, (y - CARD_CAR_OFFSET_Y) as u32),
            &image::Rgba([255, 0, 0, 255])
        );
    }

    #[test]
    fn car_taller_than_base_is_clipped_safely() {
        let base = image::RgbaImage::from_pixel(
            CARD_BASE_W as u32,
            CARD_BASE_H as u32,
            image::Rgba([255, 0, 0, 255]),
        );
        let car = image::RgbaImage::from_pixel(512, 512, image::Rgba([0, 0, 255, 255]));
        let out = Launcher::overlay_car(base, &car, 1.0, CARD_CAR_OFFSET_X, CARD_CAR_OFFSET_Y);

        assert_eq!(out.width(), CARD_BASE_W as u32);
        assert_eq!(out.height(), CARD_BASE_H as u32);
        assert_eq!(
            out.get_pixel(CARD_BASE_W as u32 / 2, CARD_BASE_H as u32 / 2),
            &image::Rgba([0, 0, 255, 255])
        );
    }
}
