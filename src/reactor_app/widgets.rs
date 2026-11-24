use std::time::{SystemTime, UNIX_EPOCH};

use windows_reactor::*;

use crate::db;
use crate::rawinput;
use crate::storage::{DPAD_CODE_BASE, GameRecord};
use rust_i18n::t;

use super::message::Message;
use super::Launcher;

pub(super) fn row_chip(label: &str) -> View {
    TextBlock::new()
        .text(label.to_string())
        .font_size(11.0)
        .opacity(0.65)
        .into()
}

pub(super) fn binding_label(value: u32) -> String {
    use rawinput::{DPAD_DOWN, DPAD_LEFT, DPAD_RIGHT, DPAD_UP};
    match value.checked_sub(DPAD_CODE_BASE) {
        Some(DPAD_UP) => t!("widgets.d_pad_up").to_string(),
        Some(DPAD_RIGHT) => t!("widgets.d_pad_right").to_string(),
        Some(DPAD_DOWN) => t!("widgets.d_pad_down").to_string(),
        Some(DPAD_LEFT) => t!("widgets.d_pad_left").to_string(),
        _ => t!("widgets.button", value = value).to_string(),
    }
}

pub(super) fn record_db_match_ref(game: &GameRecord) -> Option<&'static db::DbMatch> {
    db::lookup_record(game)
}

pub(super) fn record_db_match(game: &GameRecord) -> Option<db::DbMatch> {
    record_db_match_ref(game).cloned()
}

/// The name of a library game's built-in cover: the database's, which may be newer than
/// the one saved when the game was added, else the saved one.
pub(super) fn builtin_cover_name(game: &GameRecord) -> Option<String> {
    record_db_match_ref(game).and_then(|matched| matched.image.clone()).or_else(|| game.cover.clone())
}

/// The name of a library game's platform: the database's, which may be newer than the one
/// saved when the game was added, else the saved one, else its loader's.
pub(super) fn platform_name(game: &GameRecord) -> String {
    record_db_match_ref(game)
        .and_then(|matched| matched.platform.clone())
        .or_else(|| game.platform.clone())
        .unwrap_or_else(|| game.loader.platform().to_string())
}

pub(super) fn record_matches_product(game: &GameRecord, product: &str) -> bool {
    if product.is_empty() {
        return false;
    }
    if let Some(m) = record_db_match_ref(game) {
        return m.name == product;
    }
    let id = game
        .game_id
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let version = game
        .version
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    match (id, version) {
        (Some(id), Some(v)) => db::lookup_by_game_id(id, v).is_some_and(|m| m.name == product),

        _ => game.name.trim() == product,
    }
}

/// The cover shown for a game that has neither a cover of its own nor a built-in one.
const FALLBACK_COVER_PNG: &[u8] = include_bytes!("../assets/cover_wip.png");

/// The fallback cover, fitted into `width` × `height`.
pub(super) fn fallback_cover(width: f64, height: f64) -> View {
    Image::new()
        .source_data(EncodedImage::new(std::sync::Arc::<[u8]>::from(FALLBACK_COVER_PNG)))
        .width(width)
        .height(height)
        .stretch(Stretch::Uniform)
        .into()
}

pub(super) fn settings_section(title: &str, rows: Vec<KeyedView>) -> View {
    StackPanel::new()
        .spacing(8.0)
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
        .children((
            TextBlock::new()
                .text(title)
                .font_size(16.0)
                .font_weight(FontWeight::SEMI_BOLD),
            settings_card(rows),
        ))
        .into()
}

/// A card of settings rows, in the Windows 11 Settings style.
pub(super) fn settings_card(rows: Vec<KeyedView>) -> View {
    Border::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .corner_radius(4.0)
        .background(ThemeBrush::CardBackground)
        .border_brush(ThemeBrush::CardStroke)
        .border_thickness(1.0)
        .content(StackPanel::new().horizontal_alignment(HorizontalAlignment::Stretch).keyed_children(rows))
        .into()
}

pub(super) fn settings_row(icon: View, title: &str, control: View) -> View {
    settings_row_sub(icon, title, None, control)
}

pub(super) fn settings_row_sub(
    icon: View,
    title: &str,
    subtitle: Option<String>,
    control: View,
) -> View {
    let mut texts = vec![KeyedView::new(
        "title",
        TextBlock::new().text(title).font_size(14.0),
    )];
    if let Some(sub) = subtitle {
        texts.push(KeyedView::new(
            "subtitle",
            TextBlock::new().text(sub).font_size(12.0).opacity(0.6),
        ));
    }
    let text: View = StackPanel::new()
        .spacing(2.0)
        .vertical_alignment(VerticalAlignment::Center)
        .keyed_children(texts)
        .into();
    Grid::new()
        .horizontal_alignment(HorizontalAlignment::Stretch)
        .margin(Thickness::new(16.0, 12.0, 16.0, 12.0))
        .columns([GridLength::Star(1.0), GridLength::Auto])
        .children((
            StackPanel::new()
                .orientation(Orientation::Horizontal)
                .spacing(14.0)
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(0)
                .children((icon, text)),
            Border::new()
                .vertical_alignment(VerticalAlignment::Center)
                .grid_column(1)
                .content(control),
        ))
        .into()
}

pub(super) fn selectable_text(text: impl Into<String>) -> TextBlock {
    TextBlock::new().text(text).is_text_selection_enabled(true)
}

pub(super) fn settings_toggle<C: IntoPayloadCallback<bool>>(on: bool, enabled: bool, callback: C) -> View {
    let state_text = if on { t!("widgets.on") } else { t!("widgets.off") };
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(8.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            TextBlock::new()
                .text(state_text)
                .font_size(14.0)
                .vertical_alignment(VerticalAlignment::Center),
            ToggleSwitch::new()
                .is_on(on)
                .is_enabled(enabled)
                .on_toggled(callback)
                .width(56.0)
                .min_width(0.0)
                .vertical_alignment(VerticalAlignment::Center)
                .slots([
                    SlotView::new(ToggleSwitchSlot::OnContent, TextBlock::new()),
                    SlotView::new(ToggleSwitchSlot::OffContent, TextBlock::new()),
                ]),
        ))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::Loader;

    fn record(loader: Loader, crc: Option<&str>, game_id: Option<&str>, version: Option<&str>, name: &str) -> GameRecord {
        GameRecord {
            id: "t".into(),
            path: "C:/game".into(),
            name: name.into(),
            crc: crc.map(Into::into),
            version: version.map(Into::into),
            arch: None,
            loader,
            platform: None,
            image: None,
            cover: None,
            serverbox: None,
            reader: None,
            amdaemon: None,
            game_exe: None,
            game_id: game_id.map(Into::into),
            gfx_patch: None,
            test_mode: None,
            card: None,
            region: None,
            play_count: 0,
            last_played: None,
            added_at: 0,
        }
    }

    #[test]
    fn crc_records_match_by_lookup() {

        let r = record(Loader::Mice, Some("0x1fd6136a"), Some("SBZZ"), Some("2.01"), "InitialD Arcade Stage 8 JPN");
        assert!(record_matches_product(&r, "InitialD Arcade Stage 8 JPN"));
        assert!(!record_matches_product(&r, "InitialD Arcade Stage 8 EXP"));
        assert!(!record_matches_product(&r, ""), "an empty product name does not match");
    }

    #[test]
    fn icf_segatools_records_match_by_game_id_and_version() {

        let s5 = record(Loader::Segatools, None, Some("SDGT"), Some("3.21.00"), "InitialD The Arcade Season 5");
        let s4 = record(Loader::Segatools, None, Some("SDGT"), Some("2.80.00"), "InitialD The Arcade Season 4");
        assert!(record_matches_product(&s5, "InitialD The Arcade Season 5"));
        assert!(!record_matches_product(&s5, "InitialD The Arcade Season 4"));
        assert!(record_matches_product(&s4, "InitialD The Arcade Season 4"));
        assert!(!record_matches_product(&s4, "InitialD The Arcade Season 5"));

        let z = record(Loader::Segatools, None, Some("SDDF"), Some("1.30.00"), "InitialD Arcade Stage Zero");
        let z2 = record(Loader::Segatools, None, Some("SDDF"), Some("2.31.00"), "InitialD Arcade Stage Zero Ver.2");
        assert!(record_matches_product(&z, "InitialD Arcade Stage Zero"));
        assert!(record_matches_product(&z2, "InitialD Arcade Stage Zero Ver.2"));
        assert!(!record_matches_product(&z2, "InitialD Arcade Stage Zero"));
    }

    #[test]
    fn manual_records_fall_back_to_name() {

        let r = record(Loader::Mice, Some("0xdeadbeef"), None, None, "My Game");
        assert!(record_matches_product(&r, "My Game"));
        assert!(!record_matches_product(&r, "Other"));
    }
}

pub(super) fn separator() -> View {
    Border::new()
        .height(1.0)
        .margin(Thickness::new(16.0, 0.0, 16.0, 0.0))
        .background(ThemeBrush::CardStroke)
        .into()
}

pub(super) fn placeholder_page(title: &str, desc: &str) -> View {
    StackPanel::new()
        .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
        .spacing(16.0)
        .children((
            StackPanel::new().spacing(4.0).children((
                TextBlock::new()
                    .text(title)
                    .font_size(26.0)
                    .font_weight(FontWeight::SEMI_BOLD),
                TextBlock::new().text(desc).font_size(12.0).opacity(0.6),
            )),
            Border::new()
                .corner_radius(6.0)
                .background(ThemeBrush::CardBackground)
                .border_brush(ThemeBrush::CardStroke)
                .border_thickness(1.0)
                .padding(Thickness::new(20.0, 28.0, 20.0, 28.0))
                .content(
                    TextBlock::new()
                        .text(t!("widgets.this_page_is_under_construction"))
                        .font_size(13.0)
                        .opacity(0.6)
                        .horizontal_alignment(HorizontalAlignment::Center),
                ),
        ))
        .into()
}

pub(super) fn same_path(a: &str, b: &std::path::Path) -> bool {
    let pa = std::path::Path::new(a);
    if let (Ok(ca), Ok(cb)) = (pa.canonicalize(), b.canonicalize()) {
        return ca == cb;
    }
    pa.to_string_lossy().to_lowercase() == b.to_string_lossy().to_lowercase()
}

pub(super) fn make_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{nanos:x}{:x}", SEQ.fetch_add(1, Ordering::Relaxed))
}

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

pub(super) const ANIM_FRAME_MS: u64 = 16;

pub(super) fn spawn_anim_tick(
    context: &ComponentContext<Launcher>,
    make: impl FnOnce() -> Message + Send + 'static,
) {
    context.spawn_background(move |_| {
        std::thread::sleep(std::time::Duration::from_millis(ANIM_FRAME_MS));
        make()
    });
}
