//! The WMMT3 tabs of a game's settings page: its Graphics, System and Input tabs for
//! Wangan Midnight Maximum Tune 3 / 3DX / 3DX+. They are drawn from the table in
//! `games::wmmt3::settings`, one row per setting, its control chosen by the setting's kind.

use std::path::Path;

use windows_reactor::*;

use rust_i18n::t;

use crate::filedialog;
use crate::font_file;
use crate::games::wmmt3::settings::{self, KEYS, Kind, Label, Section, Setting, XINPUT_BUTTONS};
use crate::rawinput;
use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::widgets::{binding_label, separator, settings_card, settings_row, settings_row_sub, settings_section, settings_toggle};

use super::input::INPUT_CAPTURE_SECS;

impl Launcher {
    /// The sections that apply to the input mode in use, each with a row per setting that has
    /// an effect.
    pub(in crate::reactor_app) fn wmmt3_sections(&self, context: &mut ViewContext<Self>, sections: &'static [Section]) -> Vec<KeyedView> {
        let mode = self.wmmt3.input_mode();
        let game_id = self.settings_game().and_then(|g| g.game_id.as_deref());
        sections
            .iter()
            .filter(|section| section.input_mode.is_none_or(|m| m == mode))
            .map(|section| {
                let switched_on = section.switch.is_none_or(|switch| self.wmmt3.is_on(switch));
                let mut rows = Vec::new();
                if switched_on {
                    let shown = section.settings.iter().filter(|s| self.wmmt3.applies(s, game_id));
                    for setting in shown {
                        if !rows.is_empty() {
                            rows.push(KeyedView::new(format!("sep-{}", setting.key), separator()));
                        }
                        rows.push(KeyedView::new(setting.key, self.wmmt3_setting_row(context, setting)));
                    }
                }
                let view = match section.switch {
                    Some(switch) => self.wmmt3_switched_section(context, section.title, switch, rows),
                    None => settings_section(&t!(section.title), rows),
                };
                KeyedView::new(section.title, view)
            })
            .collect()
    }

    /// A section with its switch in the heading, as the Initial D force feedback card: the
    /// title and the switch on the card's first row, the section's rows below while it is on.
    fn wmmt3_switched_section(
        &self,
        context: &mut ViewContext<Self>,
        title: &'static str,
        switch: &'static Setting,
        rows: Vec<KeyedView>,
    ) -> View {
        let key = switch.key;
        // The title is bold, as a heading, unlike the rows below; otherwise a settings row.
        let heading: View = Grid::new()
            .margin(Thickness::new(16.0, 12.0, 16.0, 12.0))
            .columns([GridLength::Star(1.0), GridLength::Auto])
            .children((
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(14.0)
                    .vertical_alignment(VerticalAlignment::Center)
                    .grid_column(0)
                    .children((
                        SymbolIcon::new().symbol(Symbol::Rotate),
                        TextBlock::new().text(t!(title)).font_size(14.0).font_weight(FontWeight::SEMI_BOLD),
                    )),
                StackPanel::new().vertical_alignment(VerticalAlignment::Center).grid_column(1).children((
                    settings_toggle(
                        self.wmmt3.is_on(switch),
                        true,
                        context.callback(move |on: bool| Message::Wmmt3SettingChanged(key, Some(on.to_string()))),
                    ),
                )),
            ))
            .into();
        let mut children = vec![KeyedView::new("heading", heading)];
        if !rows.is_empty() {
            children.push(KeyedView::new("sep-heading", separator()));
            children.extend(rows);
        }
        StackPanel::new()
            .margin(Thickness::new(0.0, 16.0, 0.0, 0.0))
            .children((settings_card(children),))
            .into()
    }

    /// The row of one setting: its name, and the control its kind calls for.
    fn wmmt3_setting_row(&self, context: &mut ViewContext<Self>, setting: &'static Setting) -> View {
        let key = setting.key;
        let value = self.wmmt3.value(setting).to_string();
        let changed = move |v: Option<String>| Message::Wmmt3SettingChanged(key, v);

        let control: View = match &setting.kind {
            Kind::Toggle => settings_toggle(
                value == "true",
                true,
                context.callback(move |on: bool| changed(Some(on.to_string()))),
            ),
            Kind::Number { min, max } => number_box(context, &value, *min, *max, key),
            Kind::Choice(items) => {
                let labels = items.iter().map(|(_, label)| label_text(label)).collect();
                let values: Vec<&'static str> = items.iter().map(|(v, _)| *v).collect();
                choice_box(context, labels, values, &value, key)
            }
            Kind::Axis { reverse } => return self.wmmt3_axis_row(context, setting, reverse),
            Kind::Key => {
                let labels = KEYS.iter().map(|(v, name)| if *v == "0x00" { unbound() } else { name.to_string() }).collect();
                let values: Vec<&'static str> = KEYS.iter().map(|(v, _)| *v).collect();
                choice_box(context, labels, values, &value, key)
            }
            Kind::XinputButton => {
                let mut values: Vec<&'static str> = XINPUT_BUTTONS.to_vec();
                values.push("none");
                let labels = values.iter().map(|v| if *v == "none" { unbound() } else { v.to_string() }).collect();
                let choice = choice_box(context, labels, values, &value, key);
                with_capture(context, choice, key)
            }
            Kind::DinputButton => {
                let shown: View = TextBlock::new()
                    .text(match value.parse::<u32>() {
                        Ok(0) | Err(_) => unbound(),
                        Ok(n) => binding_label(n),
                    })
                    .vertical_alignment(VerticalAlignment::Center)
                    .opacity(0.7)
                    .into();
                let clear: View = Button::new()
                    .on_click(context.message(Message::Wmmt3SettingChanged(key, Some("0".to_string()))))
                    .content(t!("wmmt3.clear").to_string())
                    .into();
                let binding: View = StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(12.0)
                    .children((shown, clear))
                    .into();
                with_capture(context, binding, key)
            }
            Kind::Device { empty } => {
                let mut values: Vec<String> = vec![String::new()];
                values.extend(self.wmmt3_devices.iter().map(|d| d.name.clone()));
                if !values.contains(&value) {
                    values.push(value.clone());
                }
                let labels: Vec<String> = values
                    .iter()
                    .map(|v| if v.is_empty() { t!(*empty).to_string() } else { v.clone() })
                    .collect();
                let selected = values.iter().position(|v| *v == value);
                ComboBox::new()
                    .width(260.0)
                    .items_source(labels)
                    .selected_index(selected)
                    .on_selection_changed(context.callback(move |i: Option<usize>| {
                        changed(i.and_then(|i| values.get(i).cloned()))
                    }))
                    .vertical_alignment(VerticalAlignment::Center)
                    .into()
            }
            Kind::Text => TextBox::new()
                .text(value)
                .on_text_changed(context.callback(move |text: String| changed(Some(text))))
                .min_width(220.0)
                .into(),
            Kind::Folder | Kind::FontFile { .. } => {
                let path: View = TextBox::new()
                    .text(value)
                    .on_text_changed(context.callback(move |text: String| changed(Some(text))))
                    .min_width(320.0)
                    .into();
                let browse: View = Button::new()
                    .on_click(context.message(Message::Wmmt3BrowsePath(key)))
                    .content(t!("wmmt3.browse").to_string())
                    .into();
                StackPanel::new()
                    .orientation(Orientation::Horizontal)
                    .spacing(8.0)
                    .children((path, browse))
                    .into()
            }
        };
        settings_row(SymbolIcon::new().symbol(Symbol::Setting).into(), &t!(setting.label), control)
    }

    /// An axis's row, as on the Initial D Input tab: the axis in use under its name, a check
    /// box that turns it around, and a button that captures it from the device.
    fn wmmt3_axis_row(&self, context: &mut ViewContext<Self>, setting: &'static Setting, reverse: &'static Setting) -> View {
        let key = setting.key;
        let reverse_key = reverse.key;
        let controls: View = StackPanel::new()
            .orientation(Orientation::Horizontal)
            .spacing(12.0)
            .vertical_alignment(VerticalAlignment::Center)
            .children((
                CheckBox::new()
                    .is_checked(self.wmmt3.is_on(reverse))
                    .on_is_checked_changed(context.callback(move |on: bool| {
                        Message::Wmmt3SettingChanged(reverse_key, Some(on.to_string()))
                    }))
                    .vertical_alignment(VerticalAlignment::Center)
                    .content(t!("page_input.reverse_axis").to_string()),
                Button::new()
                    .on_click(context.message(Message::Wmmt3CaptureStart(key)))
                    .content(t!("page_input.capture").to_string()),
            ))
            .into();
        settings_row_sub(
            SymbolIcon::new().symbol(Symbol::Accept).into(),
            &t!(setting.label),
            Some(t!("page_input.axis", axis = self.wmmt3.value(setting)).to_string()),
            controls,
        )
    }

    /// Handles the messages of the WMMT3 tabs; hands the others back.
    pub(in crate::reactor_app) fn handle_wmmt3(&mut self, message: Message, context: &ComponentContext<Self>) -> Option<Message> {
        match message {
            Message::Wmmt3SettingChanged(key, Some(value)) => {
                let font_name = font_name_for(key, &value);
                self.set_wmmt3(key, value);
                if let Some((name_key, name)) = font_name {
                    self.set_wmmt3(name_key, name);
                }
            }
            Message::Wmmt3SettingChanged(_, None) => {}
            Message::Wmmt3BrowsePath(key) => browse_wmmt3_path(key, context),
            Message::Wmmt3CaptureStart(key) => self.start_wmmt3_capture(key, context),
            Message::Wmmt3Captured(epoch, key, value) => {
                if epoch == self.input_capture_gen {
                    self.end_capture();
                    self.set_wmmt3(key, value);
                }
            }
            Message::Wmmt3CaptureUnusable(epoch, reason) => {
                if epoch == self.input_capture_gen {
                    self.end_capture();
                    self.dialog = Dialog::Error { message: t!(reason).to_string() };
                }
            }
            Message::Wmmt3DevicesScanned(devices) => self.wmmt3_devices = *devices,
            other => return Some(other),
        }
        None
    }

    /// Looks for DirectInput devices to offer on the WMMT3 Input tab.
    pub(in crate::reactor_app) fn scan_wmmt3_devices(&self, context: &ComponentContext<Self>) {
        context.spawn_background(|_| Message::Wmmt3DevicesScanned(Box::new(rawinput::list_game_devices())));
    }

    /// Saves a WMMT3 setting into the game's settings file, unless it already has that value.
    fn set_wmmt3(&mut self, key: &str, value: String) {
        let Some(setting) = settings::find(key) else {
            return;
        };
        if self.wmmt3.value(setting) == value {
            return;
        }
        let Some(file) = self.settings_game().map(|game| self.store.ensure_game_settings_file(game)) else {
            return;
        };
        let result = file.and_then(|path| self.wmmt3.set(&path, setting, value));
        self.report_save(result);
    }

    /// Waits for a button (or for an axis, for an axis setting) on the device of the input
    /// mode in use and binds it to `key`.
    fn start_wmmt3_capture(&mut self, key: &'static str, context: &ComponentContext<Self>) {
        let Some(setting) = settings::find(key) else {
            return;
        };
        let xinput = matches!(setting.kind, Kind::XinputButton);
        let axis = matches!(setting.kind, Kind::Axis { .. });
        let pad = settings::find("xinput_device")
            .and_then(|s| self.wmmt3.value(s).parse::<u32>().ok())
            .unwrap_or(0);

        self.input_capture_gen += 1;
        let epoch = self.input_capture_gen;
        self.dialog = Dialog::InputCapture { label_key: setting.label };
        self.input_capture_start = Some(std::time::Instant::now());
        self.input_capture_remain = 1.0;
        Self::spawn_capture_tick(context, epoch);

        let timeout_ms = (INPUT_CAPTURE_SECS * 1000.0) as u32;
        context.spawn_background(move |_| {
            if xinput {
                return match rawinput::capture_xi_button(pad, timeout_ms) {
                    Some(c) => Message::Wmmt3Captured(epoch, key, c.name),
                    None => Message::InputCaptureTimeout(epoch),
                };
            }
            if axis {
                // KittenTools reads the axis DirectInput makes of the control that moved.
                return match rawinput::capture_axis(timeout_ms) {
                    Some(c) => match c.dinput {
                        Some(name) => Message::Wmmt3Captured(epoch, key, name.to_string()),
                        None => Message::Wmmt3CaptureUnusable(epoch, "wmmt3.axis_not_assignable"),
                    },
                    None => Message::InputCaptureTimeout(epoch),
                };
            }
            match rawinput::capture_button(timeout_ms) {
                // KittenTools moves the lever with the wheel's POV hat by itself; a hat
                // direction is not a button it can bind.
                Some(c) if c.is_dpad => Message::Wmmt3CaptureUnusable(epoch, "wmmt3.dpad_not_assignable"),
                Some(c) => Message::Wmmt3Captured(epoch, key, c.button.to_string()),
                None => Message::InputCaptureTimeout(epoch),
            }
        });
    }
}

/// The font files a font file dialog offers.
const FONT_FILES: &str = "*.ttf;*.ttc;*.otf";

/// Opens the dialog a path setting calls for, titled with the setting's name; the folder or
/// file chosen becomes its value.
fn browse_wmmt3_path(key: &'static str, context: &ComponentContext<Launcher>) {
    let Some(setting) = settings::find(key) else {
        return;
    };
    let pick_folder = match setting.kind {
        Kind::Folder => true,
        Kind::FontFile { .. } => false,
        _ => return,
    };
    let title = t!(setting.label).to_string();
    let font_files = t!("wmmt3.font_files").to_string();
    let all_files = t!("common.all_files").to_string();
    context.spawn_background(move |_| {
        let picked = if pick_folder {
            filedialog::pick_folder(&title)
        } else {
            filedialog::pick_open_file(&[(&font_files, FONT_FILES), (&all_files, "*.*")], &title)
        };
        Message::Wmmt3SettingChanged(key, picked.map(|path| path.display().to_string()))
    });
}

/// For a font file setting given `path`: the setting its font's name goes into, and the
/// family name read from the file. `None` for any other setting, or a file without a name.
fn font_name_for(key: &str, path: &str) -> Option<(&'static str, String)> {
    let Kind::FontFile { name } = settings::find(key)?.kind else {
        return None;
    };
    Some((name, font_file::family_name(Path::new(path))?))
}

/// The text of a choice.
fn label_text(label: &Label) -> String {
    match label {
        Label::Plain(text) => text.to_string(),
        Label::Text(key) => t!(*key).to_string(),
    }
}

/// The text for "no binding".
fn unbound() -> String {
    t!("page_input.unbound").to_string()
}

/// A drop-down of `labels`, standing for `values`, with `value` selected.
fn choice_box(
    context: &mut ViewContext<Launcher>,
    labels: Vec<String>,
    values: Vec<&'static str>,
    value: &str,
    key: &'static str,
) -> View {
    let selected = values.iter().position(|v| v.eq_ignore_ascii_case(value));
    ComboBox::new()
        .width(180.0)
        .items_source(labels)
        .selected_index(selected)
        .on_selection_changed(context.callback(move |i: Option<usize>| {
            Message::Wmmt3SettingChanged(key, i.and_then(|i| values.get(i)).map(|v| v.to_string()))
        }))
        .vertical_alignment(VerticalAlignment::Center)
        .into()
}

/// A whole number from `min` to `max`.
fn number_box(context: &mut ViewContext<Launcher>, value: &str, min: i64, max: i64, key: &'static str) -> View {
    NumberBox::new()
        .minimum(min as f64)
        .maximum(max as f64)
        .value(value.parse::<f64>().ok())
        .on_value_changed(context.callback(move |v: Option<f64>| {
            Message::Wmmt3SettingChanged(key, v.map(|v| (v.round() as i64).clamp(min, max).to_string()))
        }))
        .into()
}

/// `control` followed by a button that captures the binding from the device.
fn with_capture(context: &mut ViewContext<Launcher>, control: View, key: &'static str) -> View {
    StackPanel::new()
        .orientation(Orientation::Horizontal)
        .spacing(12.0)
        .vertical_alignment(VerticalAlignment::Center)
        .children((
            control,
            Button::new()
                .on_click(context.message(Message::Wmmt3CaptureStart(key)))
                .content(t!("page_input.capture").to_string()),
        ))
        .into()
}
