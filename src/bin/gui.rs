use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};

use rust_huge_project::database::PortfolioStock;
use rust_huge_project::protocol::{
    AlertDirection, AlertRequest, ClientMsg, ServerMsg, parse_server_msg,
};

use eframe::egui;

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(
        "Stock Alerts GUI",
        native_options,
        Box::new(|_cc| Box::new(App::new())),
    )
}

#[derive(Debug, Clone)]
enum UiCommand {
    Connect { addr: String },
    Disconnect,
    AddAlert { symbol: String, dir: AlertDirection, threshold: f64 },
    RemoveAlert { symbol: String, dir: AlertDirection },
    LoginClient { username: String, password: String },
    RegisterClient { username: String, password: String },
    CheckPrice { symbol: String },
    BuyStock { symbol: String, quantity: i32 },
    SellStock { symbol: String, quantity: i32 },
    GetAllClientData,
}

#[derive(Debug, Clone)]
enum ClientEvent {
    Connected,
    Disconnected { reason: String },
    AlertTriggered { symbol: String, dir: AlertDirection, threshold: f64, current: f64 },
    AlertAdded { symbol: String, dir: AlertDirection, threshold: f64 },
    AlertRemoved { symbol: String, dir: AlertDirection },
    AllClientData { stocks: Vec<PortfolioStock>, alerts: Vec<AlertRow> },
    UserLogged,
    UserRegistered,
    ServerError(String),
    PriceChecked { symbol: String, price: f64},
    Log(String),
}

fn spawn_network_worker() -> (Sender<UiCommand>, Receiver<ClientEvent>) {
    let (cmd_tx, cmd_rx) = unbounded::<UiCommand>();
    let (ev_tx, ev_rx) = unbounded::<ClientEvent>();

    thread::spawn(move || network_thread(cmd_rx, ev_tx));

    (cmd_tx, ev_rx)
}

fn network_thread(cmd_rx: Receiver<UiCommand>, ev_tx: Sender<ClientEvent>) {
    let mut state = NetState::Disconnected;

    loop {
        match &mut state {
            NetState::Disconnected => {
                match cmd_rx.recv() {
                    Ok(UiCommand::Connect { addr }) => {
                        match TcpStream::connect(&addr) {
                            Ok(stream) => {
                                let _ = stream.set_read_timeout(Some(Duration::from_millis(100)));
                                let _ = stream.set_nodelay(true);

                                let reader = match stream.try_clone() {
                                    Ok(s) => BufReader::new(s),
                                    Err(e) => {
                                        let _ = ev_tx.send(ClientEvent::Disconnected {
                                            reason: format!("try_clone failed: {e}"),
                                        });
                                        continue;
                                    }
                                };

                                state = NetState::Connected {
                                    addr,
                                    stream,
                                    reader,
                                };
                                let _ = ev_tx.send(ClientEvent::Connected);
                                let _ = ev_tx.send(ClientEvent::Log("Connected.".into()));
                            }
                            Err(e) => {
                                let _ = ev_tx.send(ClientEvent::Disconnected {
                                    reason: format!("connect failed: {e}"),
                                });
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
            }

            NetState::Connected { addr: _, stream, reader } => {
                match cmd_rx.recv_timeout(Duration::from_millis(25)) {
                    Ok(cmd) => {
                        if handle_command_connected(cmd, stream, &ev_tx).is_err() {
                            state = NetState::Disconnected;
                            let _ = ev_tx.send(ClientEvent::Disconnected {
                                reason: "write to server failed".into(),
                            });
                            continue;
                        }
                    }
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                    Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                }

                match read_one_line(reader) {
                    Ok(Some(line)) => {
                        handle_server_line(&line, &ev_tx);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        if e.kind() != io::ErrorKind::WouldBlock && e.kind() != io::ErrorKind::TimedOut {
                            state = NetState::Disconnected;
                            let _ = ev_tx.send(ClientEvent::Disconnected {
                                reason: format!("server read failed: {e}"),
                            });
                        }
                    }
                }
            }
        }
    }
}

enum NetState {
    Disconnected,
    Connected {
        addr: String,
        stream: TcpStream,
        reader: BufReader<TcpStream>,
    },
}

fn handle_command_connected(
    cmd: UiCommand,
    stream: &mut TcpStream,
    ev_tx: &Sender<ClientEvent>,
) -> io::Result<()> {
    match cmd {
        UiCommand::Disconnect => {
            let _ = stream.shutdown(std::net::Shutdown::Both);
            let _ = ev_tx.send(ClientEvent::Disconnected {
                reason: "Disconnected by user".into(),
            });
            Ok(())
        }

        UiCommand::Connect { .. } => Ok(()),

        UiCommand::AddAlert { symbol, dir, threshold } => {
            let msg = ClientMsg::AddAlert(AlertRequest {
                symbol,
                direction: dir,
                threshold,
            });
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::RemoveAlert { symbol, dir } => {
            let msg = ClientMsg::RemoveAlert {
                symbol,
                direction: dir,
            };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::LoginClient { username, password } => {
            let msg = ClientMsg::LoginClient { username, password };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::RegisterClient { username, password } => {
            let msg = ClientMsg::RegisterClient { username, password };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::CheckPrice { symbol } => {
            let msg = ClientMsg::CheckPrice { symbol };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::BuyStock { symbol, quantity } => {
            let msg = ClientMsg::BuyStock { symbol, quantity };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::SellStock { symbol, quantity } => {
            let msg = ClientMsg::SellStock { symbol, quantity };
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }

        UiCommand::GetAllClientData => {
            let msg = ClientMsg::GetAllClientData;
            let wire = msg.to_wire();
            stream.write_all(wire.as_bytes())?;
            Ok(())
        }
    }
}

fn read_one_line(reader: &mut BufReader<TcpStream>) -> io::Result<Option<String>> {
    let mut s = String::new();
    match reader.read_line(&mut s) {
        Ok(0) => Err(io::Error::new(io::ErrorKind::UnexpectedEof, "server closed")),
        Ok(_) => Ok(Some(s.trim_end().to_string())),
        Err(e) => Err(e),
    }
}

fn handle_server_line(line: &str, ev_tx: &Sender<ClientEvent>) {
    match parse_server_msg(line) {
        Some(ServerMsg::AlertTriggered { symbol, direction, threshold, current_price }) => {
            let _ = ev_tx.send(ClientEvent::AlertTriggered {
                symbol,
                dir: direction,
                threshold,
                current: current_price.value,
            });
        }
        Some(ServerMsg::AlertAdded { symbol, direction, threshold }) => {
            let _ = ev_tx.send(ClientEvent::AlertAdded {
                symbol,
                dir: direction,
                threshold,
            });
        }
        Some(ServerMsg::AlertRemoved { symbol, direction }) => {
            let _ = ev_tx.send(ClientEvent::AlertRemoved {
                symbol,
                dir: direction,
            });
        }
        Some(ServerMsg::StockBought { symbol, quantity }) => {
            let msg = format!("Bought {quantity}x {symbol}");
            let _ = ev_tx.send(ClientEvent::Log(msg));
        }
        Some(ServerMsg::StockSold { symbol, quantity }) => {
            let msg = format!("Sold {quantity}x {symbol}");
            let _ = ev_tx.send(ClientEvent::Log(msg));
        }
        Some(ServerMsg::PriceChecked{ symbol, price}) => {
            let _ = ev_tx.send(ClientEvent::PriceChecked { symbol, price });
        }
        Some(ServerMsg::AllClientData { stocks, alerts }) => {
            let mapped_alerts = alerts
                .into_iter()
                .map(|alert| AlertRow {
                    symbol: alert.symbol,
                    dir: alert.direction,
                    threshold: alert.threshold,
                })
                .collect::<Vec<_>>();
            let _ = ev_tx.send(ClientEvent::AllClientData {
                stocks,
                alerts: mapped_alerts,
            });
        }
        Some(ServerMsg::UserLogged) => {
            let _ = ev_tx.send(ClientEvent::UserLogged);
        }
        Some(ServerMsg::UserRegistered) => {
            let _ = ev_tx.send(ClientEvent::UserRegistered);
        }
        Some(ServerMsg::Error(msg)) => {
            let _ = ev_tx.send(ClientEvent::ServerError(msg));
        }
        None => {
            let _ = ev_tx.send(ClientEvent::Log(format!("Unparsed: {line}")));
        }
    }
}

struct App {
    cmd_tx: Sender<UiCommand>,
    ev_rx: Receiver<ClientEvent>,
    addr: String,
    connected: bool,
    conn_status: String,
    symbol_input: String,
    dir_input: AlertDirection,
    threshold_input: String,
    quantity_input: String,
    username_input: String,
    password_input: String,
    command_kind: CommandKind,
    auth_mode: AuthMode,
    authenticated: bool,
    auth_notice: Option<String>,
    alert_popup_open: bool,
    alert_popup_message: Option<String>,
    alert_popup_data: Option<AlertRow>,
    alerts: Vec<AlertRow>,
    portfolio: Vec<PortfolioStock>,
    pending_trade: Option<PendingTrade>,
    style_initialized: bool,
    logs: Vec<LogRow>,
    max_logs: usize,
}

#[derive(Debug, Clone)]
struct AlertRow {
    symbol: String,
    dir: AlertDirection,
    threshold: f64,
}

#[derive(Clone)]
struct LogRow {
    ts: String,
    msg: String,
    kind: LogKind,
}

#[derive(Clone, Copy)]
enum LogKind {
    Info,
    Error,
    Alert,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandKind {
    AddAlert,
    RemoveAlert,
    CheckPrice,
    BuyStock,
    SellStock,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Login,
    Register,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TradeKind {
    Buy,
    Sell,
}

#[derive(Clone)]
struct PendingTrade {
    symbol: String,
    quantity: i32,
    kind: TradeKind,
}

impl App {
    fn new() -> Self {
        let (cmd_tx, ev_rx) = spawn_network_worker();
        Self {
            cmd_tx,
            ev_rx,
            addr: "127.0.0.1:1234".into(),
            connected: false,
            conn_status: "Disconnected".into(),
            symbol_input: "AAPL".into(),
            dir_input: AlertDirection::Above,
            threshold_input: "200".into(),
            quantity_input: "1".into(),
            username_input: "user".into(),
            password_input: "pass".into(),
            command_kind: CommandKind::AddAlert,
            auth_mode: AuthMode::Login,
            authenticated: false,
            auth_notice: None,
            alert_popup_open: false,
            alert_popup_message: None,
            alert_popup_data: None,
            alerts: Vec::new(),
            portfolio: Vec::new(),
            pending_trade: None,
            style_initialized: false,
            logs: Vec::new(),
            max_logs: 500,
        }
    }

    fn push_log(&mut self, kind: LogKind, msg: impl Into<String>) {
        let ts = now_hhmmss();
        self.logs.push(LogRow {
            ts,
            msg: msg.into(),
            kind,
        });
        if self.logs.len() > self.max_logs {
            let overflow = self.logs.len() - self.max_logs;
            self.logs.drain(0..overflow);
        }
    }

    fn drain_events(&mut self) {
        while let Ok(ev) = self.ev_rx.try_recv() {
            match ev {
                ClientEvent::Connected => {
                    self.connected = true;
                    self.conn_status = "Connected".into();
                    self.push_log(LogKind::Info, "Connected to server.");
                }
                ClientEvent::Disconnected { reason } => {
                    self.connected = false;
                    self.conn_status = format!("Disconnected: {reason}");
                    self.authenticated = false;
                    self.auth_notice = Some("Disconnected from server.".into());
                    self.push_log(LogKind::Error, format!("Disconnected: {reason}"));
                }
                ClientEvent::AlertTriggered { symbol, dir, threshold, current } => {
                    self.alert_popup_message = Some(format!(
                        "[ALERT] {symbol} {:?} threshold={threshold} current={current}",
                        dir
                    ));
                    self.alert_popup_data = Some(AlertRow {
                        symbol: symbol.clone(),
                        dir,
                        threshold,
                    });
                    self.alert_popup_open = true;
                    play_alert_sound();
                    self.push_log(
                        LogKind::Alert,
                        format!("[ALERT] {symbol} {:?} threshold={threshold} current={current}", dir),
                    );
                }
                ClientEvent::AlertAdded { symbol, dir, threshold } => {
                    let popup_msg = format!("Alert added: {symbol} {:?} threshold={threshold}", dir);
                    if !self.alerts.iter().any(|a| a.symbol == symbol && a.dir == dir && a.threshold == threshold) {
                        self.alerts.push(AlertRow {
                            symbol: symbol.clone(),
                            dir,
                            threshold,
                        });
                    }
                    self.alert_popup_message = Some(popup_msg);
                    self.alert_popup_data = Some(AlertRow {
                        symbol: symbol.clone(),
                        dir,
                        threshold,
                    });
                    self.alert_popup_open = true;
                    self.push_log(
                        LogKind::Info,
                        format!("Alert added: {symbol} {:?} threshold={threshold}", dir),
                    );
                }
                ClientEvent::AlertRemoved { symbol, dir } => {
                    self.remove_local_alert(&symbol, dir);
                    self.push_log(LogKind::Info, format!("Alert removed: {symbol} {:?}", dir));
                }
                ClientEvent::PriceChecked { symbol, price } => {
                    if let Some(pending) = self.pending_trade.clone() {
                        if pending.symbol == symbol {
                            self.pending_trade = None;
                            match pending.kind {
                                TradeKind::Buy => {
                                    self.send(UiCommand::BuyStock {
                                        symbol: pending.symbol.clone(),
                                        quantity: pending.quantity,
                                    });
                                    self.push_log(
                                        LogKind::Info,
                                        format!(
                                            "[BUY] {symbol} qty={} price={price}",
                                            pending.quantity
                                        ),
                                    );
                                }
                                TradeKind::Sell => {
                                    self.send(UiCommand::SellStock {
                                        symbol: pending.symbol.clone(),
                                        quantity: pending.quantity,
                                    });
                                    self.push_log(
                                        LogKind::Info,
                                        format!(
                                            "[SELL] {symbol} qty={} price={price}",
                                            pending.quantity
                                        ),
                                    );
                                }
                            }
                            return;
                        }
                    }
                    self.push_log(LogKind::Info, format!("[PRICE] {symbol} price={price}"));
                }
                ClientEvent::AllClientData { stocks, alerts } => {
                    self.alerts = alerts;
                    self.portfolio = stocks;
                    self.push_log(
                        LogKind::Info,
                        format!(
                            "Loaded {} portfolio entries and {} alerts.",
                            self.portfolio.len(),
                            self.alerts.len()
                        ),
                    );
                }
                ClientEvent::UserLogged => {
                    self.authenticated = true;
                    self.auth_notice = Some("Logged in successfully.".into());
                    self.push_log(LogKind::Info, "Logged in successfully.");
                    self.send(UiCommand::GetAllClientData);
                }
                ClientEvent::UserRegistered => {
                    self.authenticated = false;
                    self.auth_notice = Some("Registered successfully. You can log in now.".into());
                    self.push_log(LogKind::Info, "Registered successfully.");
                }
                ClientEvent::ServerError(msg) => {
                    self.auth_notice = Some(msg.clone());
                    self.push_log(LogKind::Error, format!("[SERVER ERR] {msg}"));
                }
                ClientEvent::Log(s) => {
                    self.push_log(LogKind::Info, s);
                }
            }
        }
    }

    fn send(&mut self, cmd: UiCommand) {
        if self.cmd_tx.send(cmd).is_err() {
            self.push_log(LogKind::Error, "Network worker not available.");
        }
    }

    fn normalize_symbol(&self) -> String {
        let mut symbol = self.symbol_input.trim().to_string();
        symbol.make_ascii_uppercase();
        symbol
    }

    fn remove_local_alert(&mut self, symbol: &str, dir: AlertDirection) {
        self.alerts.retain(|row| !(row.symbol == symbol && row.dir == dir));
    }

    fn render_auth_screen(&mut self, ui: &mut egui::Ui) {
        ui.heading("Login / Register");

        ui.horizontal(|ui| {
            ui.selectable_value(&mut self.auth_mode, AuthMode::Login, "Login");
            ui.selectable_value(&mut self.auth_mode, AuthMode::Register, "Register");
        });

        ui.separator();

        ui.horizontal(|ui| {
            ui.label("Username:");
            ui.text_edit_singleline(&mut self.username_input);
        });

        ui.horizontal(|ui| {
            ui.label("Password:");
            ui.add(egui::TextEdit::singleline(&mut self.password_input).password(true));
        });

        ui.add_space(8.0);

        let action_label = match self.auth_mode {
            AuthMode::Login => "Login",
            AuthMode::Register => "Register",
        };
        let auth_enabled = self.connected;
        if ui.add_enabled(auth_enabled, egui::Button::new(action_label)).clicked() {
            let username = self.username_input.trim().to_string();
            let password = self.password_input.trim().to_string();
            self.auth_notice = Some("Waiting for server response...".into());
            match self.auth_mode {
                AuthMode::Login => self.send(UiCommand::LoginClient { username, password }),
                AuthMode::Register => self.send(UiCommand::RegisterClient { username, password }),
            }
        }

        if let Some(msg) = &self.auth_notice {
            ui.add_space(6.0);
            ui.label(msg);
        }

        ui.add_space(16.0);
        ui.small("You must be connected to log in or register.");
    }

    fn render_main_screen(&mut self, ui: &mut egui::Ui) {
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.heading("Command");

                ui.horizontal(|ui| {
                    ui.label("Command:");
                    egui::ComboBox::from_id_source("cmd_combo")
                        .selected_text(match self.command_kind {
                            CommandKind::AddAlert => "ADD",
                            CommandKind::RemoveAlert => "DEL",
                            CommandKind::CheckPrice => "PRICE",
                            CommandKind::BuyStock => "BUY",
                            CommandKind::SellStock => "SELL",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.command_kind, CommandKind::AddAlert, "ADD");
                            ui.selectable_value(&mut self.command_kind, CommandKind::RemoveAlert, "DEL");
                            ui.selectable_value(&mut self.command_kind, CommandKind::CheckPrice, "PRICE");
                            ui.selectable_value(&mut self.command_kind, CommandKind::BuyStock, "BUY");
                            ui.selectable_value(&mut self.command_kind, CommandKind::SellStock, "SELL");
                        });
                });

                match self.command_kind {
                    CommandKind::AddAlert => {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut self.symbol_input);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Direction:");
                            egui::ComboBox::from_id_source("dir_combo")
                                .selected_text(match self.dir_input {
                                    AlertDirection::Above => "ABOVE",
                                    AlertDirection::Below => "BELOW",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.dir_input, AlertDirection::Above, "ABOVE");
                                    ui.selectable_value(&mut self.dir_input, AlertDirection::Below, "BELOW");
                                });
                        });

                        ui.horizontal(|ui| {
                            ui.label("Threshold:");
                            ui.text_edit_singleline(&mut self.threshold_input);
                        });

                        ui.add_space(8.0);

                        let add_enabled = self.connected;
                        if ui.add_enabled(add_enabled, egui::Button::new("Send")).clicked() {
                            let symbol = self.normalize_symbol();
                            let threshold = self.threshold_input.trim().parse::<f64>();
                            match threshold {
                                Ok(th) => {
                                    if self.alerts.iter().any(|a| a.symbol == symbol && a.dir == self.dir_input && a.threshold == th) {
                                        self.push_log(LogKind::Error, "Alert already exists.");
                                        return;
                                    }
                                    self.send(UiCommand::AddAlert {
                                        symbol,
                                        dir: self.dir_input,
                                        threshold: th,
                                    });
                                }
                                Err(_) => {
                                    self.push_log(LogKind::Error, "Invalid threshold (expected number).");
                                }
                            }
                        }
                    }
                    CommandKind::RemoveAlert => {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut self.symbol_input);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Direction:");
                            egui::ComboBox::from_id_source("dir_combo")
                                .selected_text(match self.dir_input {
                                    AlertDirection::Above => "ABOVE",
                                    AlertDirection::Below => "BELOW",
                                })
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut self.dir_input, AlertDirection::Above, "ABOVE");
                                    ui.selectable_value(&mut self.dir_input, AlertDirection::Below, "BELOW");
                                });
                        });

                        ui.add_space(8.0);

                        let del_enabled = self.connected;
                        if ui.add_enabled(del_enabled, egui::Button::new("Send")).clicked() {
                            let symbol = self.normalize_symbol();
                            self.send(UiCommand::RemoveAlert {
                                symbol: symbol.clone(),
                                dir: self.dir_input,
                            });
                            self.remove_local_alert(&symbol, self.dir_input);
                        }
                    }
                    CommandKind::CheckPrice => {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut self.symbol_input);
                        });

                        ui.add_space(8.0);

                        let price_enabled = self.connected;
                        if ui.add_enabled(price_enabled, egui::Button::new("Send")).clicked() {
                            let symbol = self.normalize_symbol();
                            self.send(UiCommand::CheckPrice { symbol });
                        }
                    }
                    CommandKind::BuyStock => {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut self.symbol_input);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Quantity:");
                            ui.text_edit_singleline(&mut self.quantity_input);
                        });

                        ui.add_space(8.0);

                        let buy_enabled = self.connected;
                        if ui.add_enabled(buy_enabled, egui::Button::new("Send")).clicked() {
                            let symbol = self.normalize_symbol();
                            let quantity = self.quantity_input.trim().parse::<i32>();
                            match quantity {
                                Ok(qty) => {
                                    self.pending_trade = Some(PendingTrade {
                                        symbol: symbol.clone(),
                                        quantity: qty,
                                        kind: TradeKind::Buy,
                                    });
                                    self.send(UiCommand::CheckPrice { symbol });
                                }
                                Err(_) => {
                                    self.push_log(LogKind::Error, "Invalid quantity (expected number).");
                                }
                            }
                        }
                    }
                    CommandKind::SellStock => {
                        ui.horizontal(|ui| {
                            ui.label("Symbol:");
                            ui.text_edit_singleline(&mut self.symbol_input);
                        });

                        ui.horizontal(|ui| {
                            ui.label("Quantity:");
                            ui.text_edit_singleline(&mut self.quantity_input);
                        });

                        ui.add_space(8.0);

                        let sell_enabled = self.connected;
                        if ui.add_enabled(sell_enabled, egui::Button::new("Send")).clicked() {
                            let symbol = self.normalize_symbol();
                            let quantity = self.quantity_input.trim().parse::<i32>();
                            match quantity {
                                Ok(qty) => {
                                    self.pending_trade = Some(PendingTrade {
                                        symbol: symbol.clone(),
                                        quantity: qty,
                                        kind: TradeKind::Sell,
                                    });
                                    self.send(UiCommand::CheckPrice { symbol });
                                }
                                Err(_) => {
                                    self.push_log(LogKind::Error, "Invalid quantity (expected number).");
                                }
                            }
                        }
                    }
                }

                ui.add_space(16.0);
                ui.label("Notes:");
                ui.small("You must be connected to send commands.");
            });

            cols[1].group(|ui| {
                ui.heading("Active alerts");
                if self.authenticated {
                    let refresh_enabled = self.connected;
                    if ui.add_enabled(refresh_enabled, egui::Button::new("Refresh data")).clicked() {
                        self.send(UiCommand::GetAllClientData);
                    }
                    ui.add_space(6.0);
                }

                if self.alerts.is_empty() {
                    ui.label("No alerts added yet.");
                } else {
                    egui::ScrollArea::vertical()
                        .id_source("alerts_scroll")
                        .max_height(240.0)
                        .show(ui, |ui| {
                        for (idx, a) in self.alerts.clone().into_iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.label(format!("{} {:?} {}", a.symbol, a.dir, a.threshold));

                                let del_enabled = self.connected;
                                if ui.add_enabled(del_enabled, egui::Button::new("Del")).clicked() {
                                    self.send(UiCommand::RemoveAlert {
                                        symbol: a.symbol.clone(),
                                        dir: a.dir,
                                    });
                                    if idx < self.alerts.len() {
                                        self.alerts.remove(idx);
                                    }
                                }
                            });
                            ui.separator();
                        }
                    });
                }
            });

            cols[1].group(|ui| {
                ui.heading("Portfolio");

                if self.portfolio.is_empty() {
                    ui.label("No portfolio entries.");
                } else {
                    egui::ScrollArea::vertical()
                        .id_source("portfolio_scroll")
                        .max_height(240.0)
                        .show(ui, |ui| {
                        for stock in &self.portfolio {
                            let (amount_label, amount_value) = if stock.total_price >= 0.0 {
                                ("spent", stock.total_price)
                            } else {
                                ("earned", -stock.total_price)
                            };
                            ui.label(format!(
                                "{} quantity={} {} {:.3}",
                                stock.symbol, stock.quantity, amount_label, amount_value
                            ));
                            ui.separator();
                        }
                    });
                }
            });
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.style_initialized {
            configure_dashboard_light_style(ctx);
            self.style_initialized = true;
        }

        self.drain_events();

        egui::TopBottomPanel::top("top_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("Server:");
                ui.text_edit_singleline(&mut self.addr);

                if !self.connected {
                    if ui.button("Connect").clicked() {
                        let addr = self.addr.trim().to_string();
                        self.conn_status = "Connecting...".into();
                        self.push_log(LogKind::Info, format!("Connecting to {addr}..."));
                        self.send(UiCommand::Connect { addr });
                    }
                } else {
                    if ui.button("Disconnect").clicked() {
                        self.send(UiCommand::Disconnect);
                    }
                }

                ui.separator();
                ui.label(format!("Status: {}", self.conn_status));
            });
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.authenticated {
                self.render_main_screen(ui);
            } else {
                self.render_auth_screen(ui);
            }

            ui.add_space(10.0);
            ui.separator();
            ui.heading("Logs");

            ui.horizontal(|ui| {
                if ui.button("Clear").clicked() {
                    self.logs.clear();
                }
                ui.label(format!("{} entries", self.logs.len()));
            });

            egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
                for row in &self.logs {
                    let prefix = format!("[{}] ", row.ts);
                    match row.kind {
                        LogKind::Info => ui.label(format!("{prefix}{}", row.msg)),
                        LogKind::Error => ui.colored_label(egui::Color32::LIGHT_RED, format!("{prefix}{}", row.msg)),
                        LogKind::Alert => ui.colored_label(egui::Color32::LIGHT_YELLOW, format!("{prefix}{}", row.msg)),
                    };
                }
            });
        });

        if self.alert_popup_open {
            let mut open = self.alert_popup_open;
            let mut should_close = false;
            egui::Window::new("Alert")
                .collapsible(false)
                .resizable(false)
                .open(&mut open)
                .show(ctx, |ui| {
                    if let Some(msg) = &self.alert_popup_message {
                        ui.label(msg);
                    } else {
                        ui.label("Alert added.");
                    }
                    ui.label("You can remove this alert if you no longer want it, or keep it.");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if ui.button("Remove alert").clicked() {
                            if let Some(alert) = self.alert_popup_data.clone() {
                                self.send(UiCommand::RemoveAlert {
                                    symbol: alert.symbol.clone(),
                                    dir: alert.dir,
                                });
                                self.remove_local_alert(&alert.symbol, alert.dir);
                            }
                            should_close = true;
                        }
                        if ui.button("Keep alert").clicked() {
                            should_close = true;
                        }
                    });
                });
            if should_close {
                open = false;
            }
            self.alert_popup_open = open;
            if !self.alert_popup_open {
                self.alert_popup_message = None;
                self.alert_popup_data = None;
            }
        }

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn configure_dashboard_light_style(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    style.visuals = egui::Visuals::light();
    style.visuals.window_fill = egui::Color32::from_rgb(244, 247, 251);
    style.visuals.panel_fill = egui::Color32::from_rgb(236, 242, 248);
    style.visuals.extreme_bg_color = egui::Color32::from_rgb(228, 236, 244);
    style.visuals.selection.bg_fill = egui::Color32::from_rgb(26, 110, 192);
    style.visuals.hyperlink_color = egui::Color32::from_rgb(20, 120, 200);
    style.visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(246, 249, 252);
    style.visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(35, 45, 55));
    style.visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(220, 234, 248);
    style.visuals.widgets.active.bg_fill = egui::Color32::from_rgb(200, 224, 246);
    style.visuals.widgets.active.fg_stroke = egui::Stroke::new(1.2, egui::Color32::from_rgb(25, 35, 45));
    style.visuals.window_rounding = egui::Rounding::same(10.0);
    style.visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(236, 242, 248);
    style.visuals.widgets.noninteractive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(55, 65, 75));

    style.spacing.button_padding = egui::vec2(12.0, 8.0);
    style.spacing.item_spacing = egui::vec2(10.0, 10.0);
    style.spacing.window_margin = egui::Margin::same(12.0);

    style.text_styles.insert(
        egui::TextStyle::Heading,
        egui::FontId::new(22.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Body,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Button,
        egui::FontId::new(16.0, egui::FontFamily::Proportional),
    );
    style.text_styles.insert(
        egui::TextStyle::Small,
        egui::FontId::new(12.0, egui::FontFamily::Proportional),
    );

    ctx.set_style(style);
}

fn now_hhmmss() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{:02}:{:02}:{:02}", h, m, s)
}

fn play_alert_sound() {
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-Command", "[console]::beep(880,200)"])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(b"\x07");
        let _ = stdout.flush();
    }
}
