// Expected format:

// ADD <SYMBOL> <ABOVE|BELOW> <THRESHOLD>
// DEL <SYMBOL> <ABOVE|BELOW>

// TRIGGER <SYMBOL> <DIRECTION> <THRESHOLD> <CURRENT>
// ERR <MESSAGE>

use std::fmt::format;

#[derive(Debug, Clone, Copy)]
pub struct Price {
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlertDirection {
    Above,
    Below,
}

impl AlertDirection {
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertDirection::Above => "ABOVE",
            AlertDirection::Below => "BELOW",
        }
    }

    pub fn as_msg(token: &str) -> Option<Self> {
        match token {
            "ABOVE" => Some(AlertDirection::Above),
            "BELOW" => Some(AlertDirection::Below),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct AlertRequest {
    pub symbol: String,
    pub direction: AlertDirection,
    pub threshold: f64,
}

#[derive(Debug, Clone)]
pub enum ClientMsg {
    AddAlert(AlertRequest),

    RemoveAlert {
        symbol: String,
        direction: AlertDirection,
    },

    RegisterClient {
        username: String,
        password: String,
    },

    LoginClient {
        username: String,
        password: String,
    },
}

#[derive(Debug, Clone)]
pub enum ServerMsg {
    AlertTriggered {
        symbol: String,
        direction: AlertDirection,
        threshold: f64,
        current_price: Price,
    },

    Error(String),
}

pub const CMD_ADD: &str = "ADD";
pub const CMD_DEL: &str = "DEL";
pub const CMD_TRIGGER: &str = "TRIGGER";
pub const CMD_ERR: &str = "ERR";
pub const CMD_LOGIN: &str = "LOGIN";
pub const CMD_REGISTER: &str = "REGISTER";

impl ClientMsg {
    pub fn to_wire(&self) -> String {
        match self {
            ClientMsg::AddAlert(alert) => {
                format!(
                    "{CMD_ADD} {} {} {}\n",
                    alert.symbol,
                    alert.direction.as_str(),
                    alert.threshold
                )
            }
            ClientMsg::RemoveAlert { symbol, direction } => {
                format!("{CMD_DEL} {} {}\n", symbol, direction.as_str())
            }
            ClientMsg::LoginClient { username, password } => {
                format!("{CMD_LOGIN} {} {}\n", username, password)
            }
            ClientMsg::RegisterClient { username, password } => {
                format!("{CMD_REGISTER} {} {}\n", username, password)
            }

        }
    }
}

pub fn parse_server_msg(line: &str) -> Option<ServerMsg> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let mut parts = line.split_whitespace();
    let cmd = parts.next()?;

    match cmd {
        CMD_TRIGGER => {
            let symbol = parts.next()?.to_string();
            let direction = AlertDirection::as_msg(parts.next()?)?;
            let threshold: f64 = parts.next()?.parse().ok()?;
            let current_value: f64 = parts.next()?.parse().ok()?;

            Some(ServerMsg::AlertTriggered {
                symbol,
                direction,
                threshold,
                current_price: Price {
                    value: current_value,
                },
            })
        }
        CMD_ERR => {
            let rest = parts.collect::<Vec<_>>().join(" ");
            Some(ServerMsg::Error(rest))
        }
        _ => None,
    }
}

pub fn parse_client_msg(line: &str) -> Option<ClientMsg> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }

    let mut parts = line.split_whitespace();
    let cmd = parts.next()?;

    match cmd {
        CMD_ADD => {
            let symbol = parts.next()?.to_string();
            let direction_str = parts.next()?;
            let direction = AlertDirection::as_msg(direction_str)?;
            let threshold: f64 = parts.next()?.parse().ok()?;

            Some(ClientMsg::AddAlert(AlertRequest {
                symbol,
                direction,
                threshold,
            }))
        }

        CMD_DEL => {
            let symbol = parts.next()?.to_string();
            let direction_str = parts.next()?;
            let direction = AlertDirection::as_msg(direction_str)?;

            Some(ClientMsg::RemoveAlert { symbol, direction })
        }
        
        CMD_LOGIN => {
            let username = parts.next()?.to_string();
            let password = parts.next()?.to_string();

            Some(ClientMsg::LoginClient { username, password })
        },


        CMD_REGISTER => {
            let username = parts.next()?.to_string();
            let password = parts.next()?.to_string();

            Some(ClientMsg::RegisterClient { username, password })
        },
        
        _ => None,
    }
}

impl ServerMsg {
    pub fn to_wire(&self) -> String {
        match self {
            ServerMsg::AlertTriggered {
                symbol,
                direction,
                threshold,
                current_price,
            } => format!(
                "{CMD_TRIGGER} {} {} {} {}\n",
                symbol,
                direction.as_str(),
                threshold,
                current_price.value
            ),

            ServerMsg::Error(msg) => {
                format!("{CMD_ERR} {}\n", msg)
            }
        }
    }
}

pub fn wire_error(msg: impl Into<String>) -> String {
    format!("{CMD_ERR} {}\n", msg.into())
}
