use std::io::{self, BufRead, BufReader, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

use crossbeam_channel::{unbounded, Receiver, Sender};

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
}

#[derive(Debug, Clone)]
enum ClientEvent {
    Connected,
    Disconnected { reason: String },
    AlertTriggered { symbol: String, dir: AlertDirection, threshold: f64, current: f64 },
    ServerError(String),
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
    username_input: String,
    password_input: String,
    command_kind: CommandKind,
    auth_mode: AuthMode,
    authenticated: bool,
    auth_notice: Option<String>,
    alerts: Vec<AlertRow>,
    logs: Vec<LogRow>,
    max_logs: usize,
}

#[derive(Clone)]
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
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AuthMode {
    Login,
    Register,
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
            username_input: "user".into(),
            password_input: "pass".into(),
            command_kind: CommandKind::AddAlert,
            auth_mode: AuthMode::Login,
            authenticated: false,
            auth_notice: None,
            alerts: Vec::new(),
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
                    self.push_log(
                        LogKind::Alert,
                        format!("[ALERT] {symbol} {:?} threshold={threshold} current={current}", dir),
                    );
                }
                ClientEvent::ServerError(msg) => {
                    let msg_lower = msg.to_ascii_lowercase();
                    if msg_lower.contains("logged in") && msg_lower.contains("succes") {
                        self.authenticated = true;
                        self.auth_notice = Some("Logged in successfully.".into());
                    } else if msg_lower.contains("registered") && msg_lower.contains("succes") {
                        self.auth_notice = Some("Registered successfully. You can log in now.".into());
                    } else {
                        self.auth_notice = Some(msg.clone());
                    }
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
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.command_kind, CommandKind::AddAlert, "ADD");
                            ui.selectable_value(&mut self.command_kind, CommandKind::RemoveAlert, "DEL");
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
                                    self.alerts.push(AlertRow {
                                        symbol: symbol.clone(),
                                        dir: self.dir_input,
                                        threshold: th,
                                    });
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
                }

                ui.add_space(16.0);
                ui.label("Notes:");
                ui.small("You must be connected to send commands.");
            });

            cols[1].group(|ui| {
                ui.heading("Active alerts");

                if self.alerts.is_empty() {
                    ui.label("No alerts added yet.");
                } else {
                    egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
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
        });
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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

        ctx.request_repaint_after(Duration::from_millis(50));
    }
}

fn now_hhmmss() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    format!("{:02}:{:02}:{:02}", h, m, s)
}
