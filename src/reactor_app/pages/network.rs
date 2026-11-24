use std::sync::atomic::Ordering;
use std::time::Duration;

use windows_reactor::*;

use crate::netinfo::{self, AdapterInfo};
use rust_i18n::t;
use crate::vpn::{VpnManager, VpnStatus};

use crate::reactor_app::Launcher;
use crate::reactor_app::message::{Dialog, Message};
use crate::reactor_app::widgets::{separator, settings_row, settings_toggle};

pub(in crate::reactor_app) const VPN_AUTO_RETRIES: u32 = 3;

pub(in crate::reactor_app) const VPN_STABLE_RESET: Duration = Duration::from_secs(300);

const VPN_WATCH_INTERVAL: Duration = Duration::from_secs(2);

impl Launcher {

    pub(in crate::reactor_app) fn selected_adapter(&self) -> Option<&AdapterInfo> {
        let guid = self.net.adapter.as_deref()?;
        self.adapters.iter().find(|a| a.guid == guid)
    }

    pub(in crate::reactor_app) fn awl_adapter_selected(&self) -> bool {
        self.selected_adapter()
            .is_some_and(|a| a.friendly == crate::awl::AWL_IFACE_NAME)
    }

    pub(in crate::reactor_app) fn save_net(&mut self) {
        if let Err(e) = self.store.save_network(&self.net) {
            self.dialog = Dialog::Error {
                message: t!("common.failed_to_save_config_ini", error = e).to_string(),
            };
        }
    }

    pub(in crate::reactor_app) fn network_page(&self, context: &mut ViewContext<Self>) -> View {
        let selected = self.selected_adapter();

        let adapter_items: Vec<String> = self.adapters.iter().map(|a| a.label()).collect();
        let adapter_idx =
            selected.and_then(|sel| self.adapters.iter().position(|a| a.guid == sel.guid));
        let adapter_placeholder = if !self.adapters_scanned {
            t!("page_network.scanning_adapters").to_string()
        } else if self.adapters.is_empty() {
            t!("page_network.no_connected_adapter").to_string()
        } else if adapter_idx.is_some() {
            String::new()
        } else if self.net.adapter.is_some() {
            t!("page_network.previously_saved_adapter_unavailable").to_string()
        } else {
            t!("page_network.select_adapter").to_string()
        };

        let (ip_items, ip_idx, ip_enabled, ip_placeholder) = match selected {
            None => (Vec::new(), None, false, t!("page_network.select_adapter_first").to_string()),
            Some(a) if a.ips.is_empty() => {
                (Vec::new(), None, false, t!("page_network.no_ipv4_subnet_on_this").to_string())
            }
            Some(a) => {
                let items = netinfo::subnet_labels(&a.ips);
                let idx = self
                    .net
                    .ip
                    .as_ref()
                    .and_then(|saved| items.iter().position(|c| c == saved));
                let placeholder = if self.net.ip.is_some() && idx.is_none() {
                    t!("page_network.previously_saved_subnet_unavailable").to_string()
                } else {
                    t!("page_network.select_subnet").to_string()
                };
                (items, idx, true, placeholder)
            }
        };

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
                    settings_row(
                        FontIcon::new().glyph("\u{E939}").into(),
                        &t!("page_network.network_adapter"),
                        ComboBox::new()
                            .width(450.0)
                            .items_source(adapter_items)
                            .selected_index(adapter_idx)
                            .placeholder_text(adapter_placeholder)
                            .on_selection_changed(context.callback(Message::AdapterSelected))
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    ),
                    separator(),
                    settings_row(
                        FontIcon::new().glyph("\u{E774}").into(),
                        &t!("page_network.ip_subnet"),
                        ComboBox::new()
                            .width(450.0)
                            .items_source(ip_items)
                            .selected_index(ip_idx)
                            .placeholder_text(ip_placeholder)
                            .is_enabled(ip_enabled)
                            .on_selection_changed(context.callback(Message::IpSelected))
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    ),
                    separator(),
                    settings_row(
                        FontIcon::new().glyph("\u{E9E9}").into(),
                        &t!("page_network.simulated_network"),
                        settings_toggle(
                            self.net.simulate != Some(false),
                            true,
                            context.callback(Message::NetworkSimulateToggled),
                        ),
                    ),
                    separator(),
                    settings_row(
                        FontIcon::new().glyph("\u{E80F}").into(),
                        &t!("page_network.host_ip"),
                        TextBox::new()
                            .width(180.0)
                            .text(self.net.host_ip.clone().unwrap_or_default())
                            .placeholder_text("192.168.100.50")
                            .is_enabled(!self.awl_adapter_selected())
                            .on_text_changed(context.callback(Message::HostIpChanged))
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    ),
                    separator(),
                    settings_row(
                        FontIcon::new().glyph("\u{E8D8}").into(),
                        &t!("page_network.serverbox_ip"),
                        TextBox::new()
                            .width(180.0)
                            .text(self.net.serverbox_ip.clone().unwrap_or_default())
                            .placeholder_text("192.168.100.101")
                            .is_enabled(self.net.simulate == Some(false) && !self.awl_adapter_selected())
                            .on_text_changed(context.callback(Message::ServerboxIpChanged))
                            .vertical_alignment(VerticalAlignment::Center)
                            .into(),
                    ),
                )),
            );

        ScrollViewer::new()
            .vertical_scroll_bar_visibility(ScrollBarVisibility::Auto)
            .content(
                StackPanel::new()
                    .horizontal_alignment(HorizontalAlignment::Stretch)
                    .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
                    .spacing(4.0)
                    .children((
                        Grid::new()
                            .columns([GridLength::Star(1.0), GridLength::Auto])
                            .children((
                                StackPanel::new()
                                    .grid_column(0)
                                    .children([TextBlock::new()
                                        .text(t!("nav.network"))
                                        .font_size(26.0)
                                        .font_weight(FontWeight::SEMI_BOLD)]),
                                Button::new()
                                    .style(ButtonStyle::Subtle)
                                    .on_click(context.message(Message::RefreshAdapters))
                                    .vertical_alignment(VerticalAlignment::Center)
                                    .grid_column(1)
                                    .content(
                                        StackPanel::new()
                                            .orientation(Orientation::Horizontal)
                                            .spacing(8.0)
                                            .children((
                                                FontIcon::new().glyph("\u{E72C}"),
                                                TextBlock::new().text(t!("page_network.refresh")),
                                            )),
                                    ),
                            )),
                        card,
                    )),
            )
            .into()
    }

    pub(in crate::reactor_app) fn vpn_page(&self, context: &mut ViewContext<Self>) -> View {
        use std::path::Path;

        let (status_text, sr, sg, sb) = match self.vpn_status {
            VpnStatus::NotConfigured => (t!("page_network.not_configured"), 0x9E, 0x9E, 0x9E),
            VpnStatus::Disconnected => (t!("page_network.disconnected"), 0xFF, 0x99, 0xA4),
            VpnStatus::Connecting => (t!("page_network.connecting"), 0x9E, 0x9E, 0x9E),
            VpnStatus::Connected => (t!("page_network.connected"), 0x6C, 0xCB, 0x5F),
            VpnStatus::Failed => (t!("page_network.connection_failed"), 0xFF, 0x99, 0xA4),
        };
        let cfg_name = self
            .vpn
            .config_path
            .as_deref()
            .and_then(|p| Path::new(p).file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| t!("page_network.not_imported").to_string());
        let connected = self.vpn_status == VpnStatus::Connected;
        let can_connect = self.vpn.config_path.is_some() && !connected;

        let connect_btn = Button::new()
            .style(ButtonStyle::Accent)
            .is_enabled(can_connect)
            .on_click(context.message(Message::VpnConnectClicked))
            .content(t!("page_network.connect").to_string());
        let disconnect_btn = Button::new()
            .is_enabled(connected)
            .on_click(context.message(Message::VpnDisconnectClicked))
            .content(t!("page_network.disconnect").to_string());

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
                    settings_row(
                        View::from(SymbolIcon::new().symbol(Symbol::MapDrive)),
                        &t!("page_network.config_file"),
                        View::from(
                            Button::new()
                                .on_click(context.message(Message::VpnImportClicked))
                                .content(t!("page_network.import_config").to_string()),
                        ),
                    ),
                    separator(),
                    settings_row(
                        View::from(SymbolIcon::new().symbol(Symbol::ProtectedDocument)),
                        &t!("page_network.current_config"),
                        View::from(
                            TextBlock::new()
                                .text(cfg_name)
                                .opacity(0.8)
                                .vertical_alignment(VerticalAlignment::Center),
                        ),
                    ),
                    separator(),
                    settings_row(
                        View::from(SymbolIcon::new().symbol(Symbol::Permissions)),
                        &t!("page_network.private_key_password"),
                        View::from(
                            PasswordBox::new()
                                .width(168.0)
                                .placeholder_text(t!("page_network.private_key_password"))
                                .password(self.vpn.passphrase.clone().unwrap_or_default())
                                .on_password_changed(context.callback(Message::VpnPassphraseChanged))
                                .vertical_alignment(VerticalAlignment::Center),
                        ),
                    ),
                    separator(),
                    settings_row(
                        View::from(SymbolIcon::new().symbol(Symbol::World)),
                        &t!("page_network.connection_status"),
                        View::from(
                            StackPanel::new()
                                .orientation(Orientation::Horizontal)
                                .spacing(8.0)
                                .vertical_alignment(VerticalAlignment::Center)
                                .children((
                                    TextBlock::new()
                                        .text(format!("● {status_text}"))
                                        .foreground(Brush::Solid(Color::argb(255, sr, sg, sb)))
                                        .vertical_alignment(VerticalAlignment::Center),
                                    connect_btn,
                                    disconnect_btn,
                                )),
                        ),
                    ),
                )),
            );

        StackPanel::new()
            .margin(Thickness::new(28.0, 20.0, 28.0, 16.0))
            .spacing(8.0)
            .children((
                TextBlock::new()
                    .text(t!("nav.vpn"))
                    .font_size(26.0)
                    .font_weight(FontWeight::SEMI_BOLD),
                card,
            ))
            .into()
    }

    pub(in crate::reactor_app) fn start_vpn_connect(&mut self, context: &ComponentContext<Self>) {
        if self.vpn.config_path.is_none() {
            self.vpn_status = VpnStatus::NotConfigured;
            self.dialog = Dialog::Error {
                message: t!("page_network.import_vpn_config_file_first").to_string(),
            };
            return;
        }

        if self.vpn_status == VpnStatus::Connected || self.vpn_status == VpnStatus::Connecting {
            return;
        }

        self.vpn_auto_retries = 0;
        self.spawn_vpn_connect_task(context);
    }

    pub(in crate::reactor_app) fn spawn_vpn_connect_task(&mut self, context: &ComponentContext<Self>) {
        self.vpn_status = VpnStatus::Connecting;
        let cfg = self.vpn.clone();
        let handle = self.vpn_handle.clone();

        let gens = self.vpn_watch_gen.clone();
        let epoch = self.vpn_watch_gen.fetch_add(1, Ordering::Relaxed) + 1;
        context.spawn_background(move |_| {
            match VpnManager::connect_blocking(
                &cfg,
                cfg.passphrase.as_deref(),
                Duration::from_secs(25),
            ) {
                Ok(h) => {
                    if gens.load(Ordering::Relaxed) != epoch {
                        drop(h);
                        return Message::VpnDied(None);
                    }
                    *handle.lock().unwrap() = Some(h);
                    Message::VpnConnectResult(None)
                }
                Err(e) => {
                    if gens.load(Ordering::Relaxed) != epoch {
                        return Message::VpnDied(None);
                    }
                    Message::VpnConnectResult(Some(e))
                }
            }
        });
    }

    pub(in crate::reactor_app) fn spawn_vpn_watch(&self, context: &ComponentContext<Self>) {
        let epoch = self.vpn_watch_gen.fetch_add(1, Ordering::Relaxed) + 1;
        let slot = self.vpn_handle.clone();
        let gens = self.vpn_watch_gen.clone();
        context.spawn_background(move |_| {
            loop {
                std::thread::sleep(VPN_WATCH_INTERVAL);

                if gens.load(Ordering::Relaxed) != epoch {
                    return Message::VpnDied(None);
                }
                let reason = {
                    let mut guard = match slot.lock() {
                        Ok(g) => g,
                        Err(_) => return Message::VpnDied(None),
                    };
                    match guard.as_mut() {

                        None => return Message::VpnDied(None),
                        Some(h) => {
                            if h.is_alive() {
                                continue;
                            }
                            Some(h.exit_detail())
                        }
                    }
                };

                return Message::VpnDied(reason);
            }
        });
    }

}
