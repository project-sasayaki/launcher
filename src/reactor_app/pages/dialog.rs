use windows_reactor::*;

use crate::detect::FileKind;
use crate::filedialog;
use rust_i18n::t;
use crate::storage::{GameRecord, Loader};

use crate::reactor_app::Launcher;
use super::game_settings::SettingsTab;
use super::input::INPUT_CAPTURE_SECS;
use crate::reactor_app::message::{CompanionKind, Dialog, Message};
use crate::reactor_app::state::PendingAdd;
use crate::reactor_app::widgets::{make_id, now_secs};

impl Launcher {

    pub(in crate::reactor_app) fn dialog_view(&self, context: &mut ViewContext<Self>) -> View {
        match &self.dialog {
            Dialog::None => StackPanel::new().into(),
            Dialog::Naming { name } => ContentDialog::new()
                .title(t!("page_dialog.add_game"))
                .primary_button_text(t!("common.add"))
                .close_button_text(t!("common.cancel"))
                .is_open(true)
                .on_closed(context.callback(Message::DialogClosed))
                .content(
                    StackPanel::new().spacing(8.0).children((
                        TextBlock::new()
                            .text(t!("page_dialog.could_not_auto_identify_this"))
                            .opacity(0.7)
                            .text_wrapping(TextWrapping::Wrap),
                        TextBox::new()
                            .text(name.clone())
                            .on_text_changed(context.callback(Message::NamingTextChanged)),
                    )),
                )
                .into(),
            Dialog::ConfirmRemove { name, .. } => ContentDialog::new()
                .title(t!("page_dialog.remove_game"))
                .primary_button_text(t!("common.remove"))
                .close_button_text(t!("common.cancel"))
                .is_open(true)
                .on_closed(context.callback(Message::DialogClosed))
                .content(
                    StackPanel::new().spacing(8.0).children((
                        TextBlock::new()
                            .text(t!("page_dialog.remove_from_game_library", name = name).to_string())
                            .text_wrapping(TextWrapping::Wrap),
                        TextBlock::new()
                            .text(t!("page_dialog.game_s_cover_and_settings"))
                            .opacity(0.6)
                            .font_size(12.0)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
                )
                .into(),
            Dialog::InputCapture { label_key, .. } => {
                let remain = self.input_capture_remain.clamp(0.0, 1.0);
                let secs = (remain * INPUT_CAPTURE_SECS).ceil() as u32;
                ContentDialog::new()
                    .title(t!("page_dialog.press_button"))
                    .close_button_text(t!("common.cancel"))
                    .is_open(true)
                    .on_closed(context.callback(Message::DialogClosed))
                    .content(
                        StackPanel::new().spacing(10.0).children((
                            TextBlock::new()
                                .text(t!("page_dialog.press_button_for_on_device", label = t!(*label_key)).to_string())
                                .text_wrapping(TextWrapping::Wrap),

                            ProgressBar::new()
                                .width(320.0)
                                .minimum(0.0)
                                .maximum(100.0)
                                .value(remain * 100.0),
                            TextBlock::new()
                                .text(t!("page_dialog.s_remaining", secs = secs).to_string())
                                .font_size(12.0)
                                .opacity(0.6),
                        )),
                    )
                    .into()
            }
            Dialog::DeviceDiagnostics { text } => {
                let body = if text.is_empty() {
                    t!("page_dialog.no_scan_result_yet")
                        .to_string()
                } else {
                    text.clone()
                };
                ContentDialog::new()
                    .title(t!("page_dialog.device_name_diagnostics"))
                    .primary_button_text(t!("common.ok"))
                    .is_open(true)
                    .on_closed(context.callback(Message::DialogClosed))
                    // A TextBlock, not a TextBox: the report is only read (and copied), and a
                    // TextBox breaks lines at '\r' only, so it showed the '\n'-separated
                    // report as one line.
                    .content(
                        ScrollViewer::new()
                            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
                            .height(260.0)
                            .min_width(520.0)
                            .content(
                                TextBlock::new()
                                    .text(body)
                                    .text_wrapping(TextWrapping::Wrap)
                                    .is_text_selection_enabled(true),
                            ),
                    )
                    .into()
            }
            Dialog::UpdateAvailable { detail } => ContentDialog::new()
                .title(t!("page_dialog.component_updates_available"))
                .primary_button_text(t!("page_dialog.go_to_settings"))
                .close_button_text(t!("page_dialog.later"))
                .is_open(true)
                .on_closed(context.callback(Message::DialogClosed))
                .content(
                    StackPanel::new().spacing(8.0).children((
                        TextBlock::new()
                            .text(t!("page_dialog.following_components_have_new_versions"))
                            .opacity(0.8),
                        TextBlock::new()
                            .text(detail.clone())
                            .text_wrapping(TextWrapping::Wrap),
                    )),
                )
                .into(),
            Dialog::LaunchFailed { message, goto } => {

                let (primary, close) = match goto {
                    Some(_) => (t!("page_dialog.go_to_settings"), Some(t!("common.cancel"))),
                    None => (t!("common.ok"), None),
                };
                let dlg = ContentDialog::new()
                    .title(t!("page_dialog.launch_failed"))
                    .primary_button_text(primary)
                    .is_open(true)
                    .on_closed(context.callback(Message::DialogClosed));
                let dlg = match close {
                    Some(c) => dlg.close_button_text(c),
                    None => dlg,
                };
                dlg
                    .content(
                        StackPanel::new().spacing(8.0).children((
                            TextBlock::new()
                                .text(message.clone())
                                .opacity(0.8)
                                .text_wrapping(TextWrapping::Wrap),
                        )),
                    )
                    .into()
            }
            Dialog::RoomNickPrompt { pending } => {
                let (action_hint, default_note) = match pending {
                    crate::reactor_app::message::PendingRoomAction::Create { .. } => (
                        t!("page_dialog.action_create_room"),
                        t!("page_dialog.skip_to_use_default_nickname"),
                    ),
                    crate::reactor_app::message::PendingRoomAction::Join { .. } => (
                        t!("page_dialog.action_join_room"),
                        t!("page_dialog.skip_to_use_default_nickname"),
                    ),
                };
                ContentDialog::new()
                    .title(t!("page_dialog.nickname_not_set"))
                    .primary_button_text(t!("page_dialog.go_to_settings"))
                    .close_button_text(t!("page_dialog.skip"))
                    .is_open(true)
                    .on_closed(context.callback(Message::DialogClosed))
                    .content(
                        StackPanel::new().spacing(8.0).children((
                            TextBlock::new()
                                .text(t!("page_dialog.no_room_nickname_is_set", action_hint = action_hint).to_string())
                                .text_wrapping(TextWrapping::Wrap),
                            TextBlock::new()
                                .text(default_note)
                                .opacity(0.6)
                                .font_size(12.0)
                                .text_wrapping(TextWrapping::Wrap),
                        )),
                    )
                    .into()
            }
            Dialog::Error { message } => ContentDialog::new()
                .title(t!("common.error"))
                .primary_button_text(t!("common.ok"))
                .is_open(true)
                .on_closed(context.callback(Message::DialogClosed))
                .content(
                    StackPanel::new().spacing(8.0).children((
                        TextBlock::new()
                            .text(message.clone())
                            .opacity(0.8)
                            .text_wrapping(TextWrapping::Wrap),
                    )),
                )
                .into(),
            Dialog::RoomCreate { name, game, password } => {
                let games = crate::db::room_game_names();
                ContentDialog::new()
                    .title(t!("page_dialog.create_room_title"))
                    .primary_button_text(t!("page_dialog.create"))
                    .close_button_text(t!("common.cancel"))
                    .is_open(true)
                    .on_closed(context.callback(Message::DialogClosed))
                    .content(
                        StackPanel::new().spacing(10.0).children((
                            TextBlock::new()
                                .text(t!("page_dialog.game"))
                                .opacity(0.7),
                            ComboBox::new()
                                .width(320.0)
                                .items_source(games)
                                .selected_index(*game)
                                .placeholder_text(t!("page_dialog.select_game"))
                                .on_selection_changed(context.callback(Message::RoomCreateGameChanged))
                                .vertical_alignment(VerticalAlignment::Center),
                            TextBlock::new()
                                .text(t!("page_dialog.room_name"))
                                .opacity(0.7),
                            TextBox::new()
                                .text(name.clone())
                                .on_text_changed(context.callback(Message::RoomCreateNameChanged)),
                            TextBlock::new()
                                .text(t!("page_dialog.password_optional"))
                                .opacity(0.7),
                            PasswordBox::new()
                                .password(password.clone())
                                .on_password_changed(context.callback(Message::RoomCreatePasswordChanged)),
                        )),
                    )
                    .into()
            }
            Dialog::RoomJoin { room_id, password } => ContentDialog::new()
                .title(t!("page_dialog.join_room_title"))
                .primary_button_text(t!("common.join"))
                .close_button_text(t!("common.cancel"))
                .is_open(true)
                .on_closed(context.callback(Message::DialogClosed))
                .content(
                    StackPanel::new().spacing(10.0).children((
                        TextBlock::new()
                            .text(t!("common.room_id"))
                            .opacity(0.7),
                        TextBox::new()
                            .text(room_id.clone())
                            .on_text_changed(context.callback(Message::RoomJoinIdChanged)),
                        TextBlock::new()
                            .text(t!("page_dialog.password"))
                            .opacity(0.7),
                        PasswordBox::new()
                            .password(password.clone())
                            .on_password_changed(context.callback(Message::RoomJoinPasswordChanged)),
                    )),
                )
                .into(),
        }
    }

    pub(in crate::reactor_app) fn apply_dialog(&mut self, context: &ComponentContext<Self>) {
        match std::mem::replace(&mut self.dialog, Dialog::None) {
            Dialog::RoomCreate { name, game, password } => {
                self.room_start_host(name, game, password, context);
            }
            Dialog::RoomJoin { room_id, password } => {
                self.room_start_join(room_id, password, context);
            }
            Dialog::RoomNickPrompt { .. } => {

                self.dialog = Dialog::None;
                self.nav_tag = "settings".to_string();
            }
            Dialog::Naming { name } => {
                let name = name.trim().to_string();
                if name.is_empty() {
                    self.pending = None;
                    return;
                }
                if let Some(add) = self.pending.take() {
                    let PendingAdd { path, ir } = add;
                    let loader = match ir.kind {
                        FileKind::Elf32 => Loader::Kitten,
                        _ => Loader::Mice,
                    };
                    self.add_game(GameRecord {
                        id: make_id(),
                        path: path.display().to_string(),
                        name,
                        crc: Some(format!("0x{:08x}", ir.crc)),
                        version: None,
                        game_id: None,
                        arch: Some(ir.kind.arch().to_string()),
                        loader,
                        platform: None,
                        image: None,
                        cover: None,
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
            Dialog::ConfirmRemove { index, .. } => {
                if self.library.games.get(index).is_some() {
                    let g = self.library.games.remove(index);
                    self.store.remove_image(&g.id);
                    self.store.remove_game_settings(&g);
                    self.store.save_library(&self.library);

                    self.drop_index = None;
                    self.drop_open = false;
                }
            }
            Dialog::UpdateAvailable { .. } => {

                self.nav_tag = "settings".to_string();
                self.update_prompt_key = None;
            }
            Dialog::LaunchFailed { goto, .. } => {

                if let Some((tag, idx)) = goto {
                    self.nav_tag = tag.clone();
                    if let Some(i) = idx {
                        match tag.as_str() {

                            "cards" => self.enter_cards(i, context),

                            // Only an input problem leads to a game's settings.
                            "game_settings" => self.open_game_settings(i, SettingsTab::Input, context),

                            "games" => {
                                self.drop_index = Some(i);
                                self.drop_open = true;
                                self.drop_anim = 1.0;
                            }

                            _ => {}
                        }
                    }
                }
            }
            Dialog::DeviceDiagnostics { .. } => {}
            _ => {}
        }
    }

    pub(in crate::reactor_app) fn pick_companion(&self, context: &ComponentContext<Self>, kind: CompanionKind, index: usize) {
        let exe_pairs = || {
            vec![
                (t!("page_dialog.executable"), "*.exe;*.elf"),
                (t!("common.all_files"), "*.*"),
            ]
        };
        let (pairs, title) = match kind {
            CompanionKind::ServerBox => (exe_pairs(), t!("page_dialog.select_serverbox_file")),
            CompanionKind::Amdaemon => (
                exe_pairs(),
                t!("page_dialog.select_amdaemon_exe"),
            ),
            CompanionKind::GameExe => (
                exe_pairs(),
                t!("page_dialog.select_game_executable"),
            ),
        };
        context.spawn_background(move |_| {
            let filters: Vec<(&str, &str)> = pairs.iter().map(|(label, pattern)| (label.as_ref(), *pattern)).collect();
            let picked = filedialog::pick_open_file(&filters, &title);
            Message::CompanionPicked {
                kind,
                index,
                path: picked,
            }
        });
    }

}
