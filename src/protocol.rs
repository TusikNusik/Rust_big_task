use std::fmt;

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
    /// Wire serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            AlertDirection::Above => "ABOVE",
            AlertDirection::Below => "BELOW",
        }
    }

    /// Wire deserialization.
    pub fn from_str(token: &str) -> Option<Self> {
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

/// Client -> Server messages.
#[derive(Debug, Clone)]
pub enum ClientMsg {
    /// Wire: ADD <SYMBOL> <DIR> <THRESHOLD>
    AddAlert(AlertRequest),

    /// Wire: DEL <SYMBOL> <DIR>
    RemoveAlert {
        symbol: String,
        direction: AlertDirection,
    },
}

/// Server -> Client messages.
#[derive(Debug, Clone)]
pub enum ServerMsg {
    /// Wire: TRIGGER <SYMBOL> <DIR> <THRESHOLD> <CURRENT_PRICE>
    AlertTriggered {
        symbol: String,
        direction: AlertDirection,
        threshold: f64,
        current_price: Price,
    },

    /// Wire: ERR <MESSAGE>
    Error(String),
}

// Wire protocol command tokens.
pub const CMD_ADD: &str = "ADD";
pub const CMD_DEL: &str = "DEL";
pub const CMD_TRIGGER: &str = "TRIGGER";
pub const CMD_ERR: &str = "ERR";

impl ClientMsg {
    /// Serializes message for TCP transmission (newline delimited).
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
        }
    }
}

/// Deserializes server messages.
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
            let direction = AlertDirection::from_str(parts.next()?)?;
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

/// Deserializes a single client command line into ClientMsg.
/// Expected wire format:
///   ADD <SYMBOL> <ABOVE|BELOW> <THRESHOLD>
///   DEL <SYMBOL> <ABOVE|BELOW>
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
            let direction = AlertDirection::from_str(direction_str)?;
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
            let direction = AlertDirection::from_str(direction_str)?;

            Some(ClientMsg::RemoveAlert { symbol, direction })
        }

        _ => None,
    }
}

impl ServerMsg {
    /// Serializes ServerMsg into wire format (newline-terminated).
    /// Format:
    ///   TRIGGER <SYMBOL> <DIR> <THRESHOLD> <CURRENT_PRICE>
    ///   ERR <MESSAGE>
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

/// Helper for the server to quickly create an error message.
pub fn wire_error(msg: impl Into<String>) -> String {
    format!("{CMD_ERR} {}\n", msg.into())
}

