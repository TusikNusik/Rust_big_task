// Expected format:

// ADD <SYMBOL> <ABOVE|BELOW> <THRESHOLD>
// DEL <SYMBOL> <ABOVE|BELOW>

use serde::{Deserialize, Serialize};

// TRIGGER <SYMBOL> <DIRECTION> <THRESHOLD> <CURRENT>
// ALERTADDED <SYMBOL> <DIRECTION> <THRESHOLD>
// ERR <MESSAGE>
use crate::database::{PortfolioStock, StoredAlert};

#[derive(Debug, Clone, Copy)]
pub struct Price {
    pub value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    CheckPrice {
        symbol: String,
    },

    BuyStock {
        symbol: String,
        quantity: i32,
    },

    SellStock {
        symbol: String,
        quantity: i32,
    },

    GetAllClientData,
}

#[derive(Debug, Clone)]
pub enum ServerMsg {
    AlertTriggered {
        symbol: String,
        direction: AlertDirection,
        threshold: f64,
        current_price: Price,
    },

    AlertAdded {
        symbol: String,
        direction: AlertDirection,
        threshold: f64,
    },

    AlertRemoved {
        symbol: String,
        direction: AlertDirection,
    },

    UserLogged,

    UserRegistered,

    PriceChecked {
        symbol: String,
        price: f64,
    },

    StockBought {
        symbol: String,
        quantity: i32,
    },

    StockSold {
        symbol: String,
        quantity: i32,
    },

    AllClientData {
        stocks: Vec<PortfolioStock>,
        alerts: Vec<StoredAlert>,
    },

    Error(String),
}

pub const CMD_ADD: &str = "ADD";
pub const CMD_DEL: &str = "DEL";
pub const CMD_TRIGGER: &str = "TRIGGER";
pub const CMD_ALERT_ADDED: &str = "ALERTADDED";
pub const CMD_ERR: &str = "ERR";
pub const CMD_LOGIN: &str = "LOGIN";
pub const CMD_REGISTER: &str = "REGISTER";
pub const CMD_PRICE: &str = "PRICE";
pub const CMD_BUY: &str = "BUY";
pub const CMD_SELL: &str = "SELL";
pub const CMD_BOUGHT: &str = "BOUGHT";
pub const CMD_SOLD: &str = "SOLD";
pub const CMD_DATA: &str = "DATA";
pub const CMD_ALERT_DELETED: &str = "ALERTDELETED";

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
            ClientMsg::CheckPrice { symbol } => {
                format!("{CMD_PRICE} {}\n", symbol)
            }
            ClientMsg::BuyStock { symbol, quantity } => {
                format!("{CMD_BUY} {} {}\n", symbol, quantity)
            }
            ClientMsg::SellStock { symbol, quantity } => {
                format!("{CMD_SELL} {} {}\n", symbol, quantity)
            }
            ClientMsg::GetAllClientData => {
                format!("{CMD_DATA}\n")
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
        CMD_ALERT_ADDED => {
            let symbol = parts.next()?.to_string();
            let direction = AlertDirection::as_msg(parts.next()?)?;
            let threshold: f64 = parts.next()?.parse().ok()?;

            Some(ServerMsg::AlertAdded {
                symbol,
                direction,
                threshold,
            })
        }

        CMD_ALERT_DELETED => {
            let symbol = parts.next()?.to_string();
            let direction = AlertDirection::as_msg(parts.next()?)?;

            Some(ServerMsg::AlertRemoved {
                symbol,
                direction,
            })
        }

        CMD_PRICE => {
            let symbol = parts.next()?.to_string();
            let price: f64 = parts.next()?.parse().ok()?;

            Some(ServerMsg::PriceChecked { symbol, price })
        }

        CMD_DATA => {
            let json_content = parts.collect::<Vec<_>>().join(" ");

            #[derive(serde::Deserialize)]
            struct DataPayload {
                stocks: Vec<PortfolioStock>,
                alerts: Vec<StoredAlert>,
            }

            let payload: DataPayload = serde_json::from_str(&json_content).ok()?;

            Some(ServerMsg::AllClientData {
                stocks: payload.stocks,
                alerts: payload.alerts,
            })
        }

        CMD_LOGIN => {
            Some(ServerMsg::UserLogged)
        }

        CMD_REGISTER => {
            Some(ServerMsg::UserRegistered)
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

        CMD_PRICE => {
            let symbol = parts.next()?.to_string();

            Some(ClientMsg::CheckPrice { symbol })
        },

        CMD_BUY => {
            let symbol = parts.next()?.to_string();
            let quantity: i32 = parts.next()?.parse().ok()?;

            Some(ClientMsg::BuyStock { symbol, quantity })
        },
        
        CMD_SELL => {
            let symbol = parts.next()?.to_string();
            let quantity: i32 = parts.next()?.parse().ok()?;

            Some(ClientMsg::SellStock { symbol, quantity })
        },

        CMD_DATA => {
            Some(ClientMsg::GetAllClientData)
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
            ServerMsg::AlertAdded {
                symbol,
                direction,
                threshold,
            } => format!(
                "{CMD_ALERT_ADDED} {} {} {}\n",
                symbol,
                direction.as_str(),
                threshold
            ),

            ServerMsg::AlertRemoved { symbol, direction} => format!(
                "{CMD_ALERT_DELETED} {} {}\n",
                symbol,
                direction.as_str()
            ),

            ServerMsg::PriceChecked { symbol, price } => format!("{CMD_PRICE} {} {}\n", symbol, price),

            ServerMsg::StockBought { symbol, quantity } => format!("{CMD_BOUGHT} {} {}\n", symbol, quantity),

            ServerMsg::StockSold { symbol, quantity } => format!("{CMD_SOLD} {} {}\n", symbol, quantity),

            ServerMsg::Error(msg) => {
                format!("{CMD_ERR} {}\n", msg)
            }

            ServerMsg::AllClientData { stocks, alerts } => {
                let json_data = serde_json::json!({
                    "stocks": stocks,
                    "alerts": alerts
                });

                let json_payload = json_data.to_string();

                format!("{CMD_DATA} {}\n", json_payload)
            }

            ServerMsg::UserLogged => format!("{CMD_LOGIN}\n"),
            ServerMsg::UserRegistered => format!("{CMD_REGISTER}\n"),

        }
    }
}

pub fn wire_error(msg: impl Into<String>) -> String {
    format!("{CMD_ERR} {}\n", msg.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_to_wire() {
        let msg = ClientMsg::LoginClient {
            username: "alice".into(),
            password: "secret".into(),
        };
        assert_eq!(msg.to_wire(), "LOGIN alice secret\n");
    }

    #[test]
    fn register_to_wire() {
        let msg = ClientMsg::RegisterClient {
            username: "bob".into(),
            password: "hunter2".into(),
        };
        assert_eq!(msg.to_wire(), "REGISTER bob hunter2\n");
    }

    #[test]
    fn parse_login_msg() {
        match parse_client_msg("LOGIN user pass") {
            Some(ClientMsg::LoginClient { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            other => panic!("unexpected parse result: {:?}", other),
        }
    }

    #[test]
    fn parse_register_msg() {
        match parse_client_msg("REGISTER user pass") {
            Some(ClientMsg::RegisterClient { username, password }) => {
                assert_eq!(username, "user");
                assert_eq!(password, "pass");
            }
            other => panic!("unexpected parse result: {:?}", other),
        }
    }

}
