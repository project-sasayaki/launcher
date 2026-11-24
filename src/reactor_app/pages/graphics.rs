use windows_reactor::*;

use rust_i18n::t;

use crate::reactor_app::Launcher;
use crate::reactor_app::message::Message;
use crate::reactor_app::widgets::{separator, settings_row, settings_toggle};

pub(in crate::reactor_app) const GFX_RESOLUTIONS: &[&str] = &["1360x768", "1920x1080", "2560x1440", "3840x2160"];

pub(in crate::reactor_app) const GFX_MSAA_LEVELS: &[&str] = &["2x", "4x", "8x", "16x"];

pub(in crate::reactor_app) const GFX_RENDER_SCALE_LEVELS: &[&str] = &["2x", "4x", "8x"];

pub(in crate::reactor_app) const GFX_AF_LEVELS: &[&str] = &["2x", "4x", "8x", "16x"];

impl Launcher {

    /// Saves the graphics settings into the file of the game whose settings page is open.
    pub(in crate::reactor_app) fn save_graphics(&mut self) {
        if let Some(game) = self.settings_game() {
            let result = self.store.save_game_graphics(game, &self.graphics);
            self.report_save(result);
        }
    }

    /// The Graphics tab of an Initial D game's settings page.
    pub(in crate::reactor_app) fn graphics_settings(&self, context: &mut ViewContext<Self>) -> View {
        let gfx = &self.graphics;

        let locked = gfx.default != Some(false);

        let resolution_items: Vec<String> = GFX_RESOLUTIONS.iter().map(|v| v.to_string()).collect();
        let resolution_idx = GFX_RESOLUTIONS
            .iter()
            .position(|v| Some(*v) == gfx.resolution.as_deref());

        let render_scale_items: Vec<String> = GFX_RENDER_SCALE_LEVELS
            .iter()
            .map(|v| v.to_string())
            .collect();
        let render_scale_idx = GFX_RENDER_SCALE_LEVELS
            .iter()
            .position(|v| Some(*v) == gfx.render_scale_level.as_deref());
        let msaa_level_items: Vec<String> = GFX_MSAA_LEVELS.iter().map(|v| v.to_string()).collect();
        let msaa_level_idx = GFX_MSAA_LEVELS
            .iter()
            .position(|v| Some(*v) == gfx.msaa_level.as_deref());
        let af_level_items: Vec<String> = GFX_AF_LEVELS.iter().map(|v| v.to_string()).collect();
        let af_level_idx = GFX_AF_LEVELS
            .iter()
            .position(|v| Some(*v) == gfx.af_level.as_deref());

        let toggle_row = |icon: Symbol,
                          title: &str,
                          on: bool,
                          enabled: bool,
                          msg: fn(bool) -> Message|
         -> View {
            settings_row(
                SymbolIcon::new().symbol(icon).into(),
                title,
                settings_toggle(on, enabled, context.callback(msg)),
            )
        };
        let combo_row = |icon: Symbol,
                         title: &str,
                         items: Vec<String>,
                         idx: Option<usize>,
                         placeholder: &str,
                         enabled: bool,
                         msg: fn(Option<usize>) -> Message|
         -> View {
            settings_row(
                SymbolIcon::new().symbol(icon).into(),
                title,
                ComboBox::new()
                    .width(180.0)
                    .items_source(items)
                    .selected_index(idx)
                    .placeholder_text(placeholder)
                    .is_enabled(enabled)
                    .on_selection_changed(context.callback(msg))
                    .vertical_alignment(VerticalAlignment::Center)
                    .into(),
            )
        };

        let mut rows: Vec<KeyedView> = vec![
            KeyedView::new(
                "gfx-default",
                toggle_row(
                    Symbol::Repair,
                    &t!("page_graphics.default_settings"),
                    gfx.default != Some(false),
                    true,
                    Message::GraphicsDefaultToggled,
                ),
            ),
            KeyedView::new("gfx-sep0", separator()),
            KeyedView::new(
                "gfx-borderless",
                toggle_row(
                    Symbol::BackToWindow,
                    &t!("page_graphics.borderless"),
                    gfx.borderless == Some(true),
                    !locked,
                    Message::GraphicsBorderlessToggled,
                ),
            ),
            KeyedView::new("gfx-sep1", separator()),
            KeyedView::new(
                "gfx-resolution",
                combo_row(
                    Symbol::View,
                    &t!("page_graphics.resolution"),
                    resolution_items,
                    resolution_idx,
                    &t!("page_graphics.select_resolution"),
                    !locked,
                    Message::GraphicsResolutionSelected,
                ),
            ),
            KeyedView::new("gfx-sep3", separator()),
            KeyedView::new(
                "gfx-render-scale",
                toggle_row(
                    Symbol::Zoom,
                    &t!("page_graphics.render_scale"),
                    gfx.render_scale == Some(true),
                    !locked,
                    Message::GraphicsRenderScaleToggled,
                ),
            ),
        ];

        if gfx.render_scale == Some(true) {
            rows.push(KeyedView::new("gfx-sep-rs", separator()));
            rows.push(KeyedView::new(
                "gfx-render-scale-level",
                combo_row(
                    Symbol::List,
                    &t!("page_graphics.render_scale_level"),
                    render_scale_items,
                    render_scale_idx,
                    &t!("page_graphics.select_render_scale"),
                    !locked,
                    Message::GraphicsRenderScaleLevelSelected,
                ),
            ));
        }
        rows.push(KeyedView::new("gfx-sep4", separator()));
        rows.push(KeyedView::new(
            "gfx-msaa",
            toggle_row(
                Symbol::Filter,
                &t!("page_graphics.msaa_antialiasing"),
                gfx.msaa == Some(true),
                !locked,
                Message::GraphicsMsaaToggled,
            ),
        ));

        if gfx.msaa == Some(true) {
            rows.push(KeyedView::new("gfx-sep5", separator()));
            rows.push(KeyedView::new(
                "gfx-msaa-level",
                combo_row(
                    Symbol::List,
                    &t!("page_graphics.msaa_level"),
                    msaa_level_items,
                    msaa_level_idx,
                    &t!("page_graphics.select_sample_level"),
                    !locked,
                    Message::GraphicsMsaaLevelSelected,
                ),
            ));
        }
        rows.push(KeyedView::new("gfx-sep6", separator()));
        rows.push(KeyedView::new(
            "gfx-af",
            toggle_row(
                Symbol::Rotate,
                &t!("page_graphics.anisotropic_filtering"),
                gfx.af == Some(true),
                !locked,
                Message::GraphicsAfToggled,
            ),
        ));

        if gfx.af == Some(true) {
            rows.push(KeyedView::new("gfx-sep7", separator()));
            rows.push(KeyedView::new(
                "gfx-af-level",
                combo_row(
                    Symbol::List,
                    &t!("page_graphics.filter_level"),
                    af_level_items,
                    af_level_idx,
                    &t!("page_graphics.select_sample_level"),
                    !locked,
                    Message::GraphicsAfLevelSelected,
                ),
            ));
        }

        Border::new()
            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
            .horizontal_alignment(HorizontalAlignment::Stretch)
            .corner_radius(4.0)
            .background(ThemeBrush::CardBackground)
            .border_brush(ThemeBrush::CardStroke)
            .border_thickness(1.0)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .keyed_children(rows),
            )
            .into()
    }

}
