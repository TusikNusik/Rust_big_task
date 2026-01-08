use reqwest::header::ACCEPT;
use reqwest::header::USER_AGENT;
use rust_huge_project::database;
use rust_huge_project::protocol::AlertRequest;
use rust_huge_project::protocol::Price;
use rust_huge_project::protocol::parse_client_msg;
use rust_huge_project::protocol::{AlertDirection, ClientMsg, ServerMsg};
use serde::Deserialize;
use sqlx::sqlite;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePool, SqlitePoolOptions};
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;
type MapLock = Arc<RwLock<HashMap<String, f64>>>;
use anyhow::{Context, Result};
use tracing::{error, info, warn};

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    result: Vec<ChartResult>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: Meta,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    currency: String,
    symbol: String,
    regular_market_price: f64,
}

fn read_all_stocks() -> Vec<String> {
    let file = fs::read_to_string("stocks_small.txt").expect("Couldn't open a file");

    file.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

async fn scrap_stocks(stock_map: MapLock, all_stocks: Vec<String>) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let url_base = "https://query1.finance.yahoo.com/v8/finance/chart/";

    loop {
        info!("[server scrapper] STARTING SCRAPPING");
        let mut temp_map = HashMap::new();

        for i in &all_stocks {
            let url = format!("{}{}", url_base, i);

            let request = client
                .get(url)
                .header(
                    USER_AGENT,
                    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)",
                )
                .header(ACCEPT, "application/json")
                .send()
                .await;

            match request {
                Ok(request) => {
                    if request.status().is_success() {
                        let yahoo_response: Result<YahooResponse, _> = request.json().await;
                        match yahoo_response {
                            Ok(yahoo_response) => {
                                let yahoo_chart = yahoo_response.chart;

                                if let Some(stock_data) = yahoo_chart.result.first() {
                                    info!(
                                        "[server scrapper] Stock symbol and currency: {} {}",
                                        stock_data.meta.symbol, stock_data.meta.currency
                                    );
                                    info!(
                                        "[server scrapper] Stock price {}",
                                        stock_data.meta.regular_market_price
                                    );
                                    temp_map.insert(
                                        stock_data.meta.symbol.clone(),
                                        stock_data.meta.regular_market_price,
                                    );
                                }
                            }
                            Err(error) => {
                                error!("[server scrapper] Failed Json convertion: {}", error)
                            }
                        }
                    } else {
                        warn!("[server scrapper] Request not succesfull!");
                    }
                }
                Err(error) => warn!("[server scrapper] Scrapping network error: {}", error),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        if !temp_map.is_empty() {
            let mut writer = stock_map.write().await;

            writer.extend(temp_map);
        }

        info!("[server] Completed scrapping all NASDAQ stocks, clients may join!");

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn client_errors(error_message: &str, write_socket: &mut OwnedWriteHalf) -> io::Result<()> {
    let message = ServerMsg::Error(error_message.to_string()).to_wire();
    write_socket.write_all(message.as_bytes()).await?;
    write_socket.flush().await?;

    Ok(())
}

async fn check_price(
    stock: &str,
    map_pointer: &MapLock,
    write_socket: &mut OwnedWriteHalf,
) -> io::Result<()> {
    let access = map_pointer.read().await;

    match access.get(stock) {
        Some(current_value) => {
            let message = ServerMsg::PriceChecked {
                symbol: stock.to_string(),
                price: *current_value,
            }
            .to_wire();
            write_socket.write_all(message.as_bytes()).await?;
            write_socket.flush().await?;
        }
        None => {
            client_errors("Stock not available!", write_socket).await?;
        }
    }

    Ok(())
}

async fn prepare_new_alert(
    pool: &sqlite::SqlitePool,
    user_id: i64,
    alert: &AlertRequest,
    map_pointer: &MapLock,
    write_socket: &mut OwnedWriteHalf,
) -> io::Result<()> {
    let access = map_pointer.read().await;

    match access.get(&alert.symbol) {
        Some(current_value) => {
            let triggered = match alert.direction {
                AlertDirection::Above => *current_value > alert.threshold,
                AlertDirection::Below => *current_value < alert.threshold,
            };
            if triggered {
                let message = ServerMsg::AlertTriggered {
                    symbol: alert.symbol.clone(),
                    direction: alert.direction,
                    threshold: alert.threshold,
                    current_price: Price {
                        value: *current_value,
                    },
                }
                .to_wire();
                write_socket.write_all(message.as_bytes()).await?;
                write_socket.flush().await?;
            }

            match database::add_alert(pool, user_id, alert).await {
                Ok(_) => {
                    let message = ServerMsg::AlertAdded {
                        symbol: alert.symbol.clone(),
                        direction: alert.direction,
                        threshold: alert.threshold,
                    }
                    .to_wire();
                    send_data(message, write_socket).await?;
                }
                Err(e) => {
                    client_errors(&e, write_socket).await?;
                }
            }
        }
        None => {
            client_errors("Stock not available!", write_socket).await?;
        }
    }
    Ok(())
}

async fn check_price_of_stock(map_pointer: &MapLock, stock: &str) -> Option<f64> {
    let access = map_pointer.read().await;

    access.get(stock).copied()
}

async fn send_data(message: String, write_socket: &mut OwnedWriteHalf) -> io::Result<()> {
    write_socket.write_all(message.as_bytes()).await?;
    write_socket.flush().await?;

    Ok(())
}

async fn check_alerts_for_user(
    pool: &SqlitePool,
    user_id: i64,
    map_lock: &MapLock,
    write_socket: &mut OwnedWriteHalf,
) -> io::Result<()> {
    let alerts = match database::get_user_alerts(pool, user_id).await {
        Ok(a) => a,
        Err(e) => {
            error!("[server-database] Database error! {}", e);
            return Ok(());
        }
    };

    let prices = map_lock.read().await;

    for alert in alerts.iter() {
        if let Some(current_price) = prices.get(&alert.symbol) {
            let triggered = match alert.direction {
                AlertDirection::Above => *current_price > alert.threshold,
                AlertDirection::Below => *current_price < alert.threshold,
            };

            if triggered {
                let message = ServerMsg::AlertTriggered {
                    symbol: alert.symbol.clone(),
                    direction: alert.direction,
                    threshold: alert.threshold,
                    current_price: Price {
                        value: *current_price,
                    },
                }
                .to_wire();
                write_socket.write_all(message.as_bytes()).await?;
                write_socket.flush().await?;
            }
        }
    }
    Ok(())
}

async fn handle_client(socket: TcpStream, map_pointer: MapLock, pool: sqlx::SqlitePool) {
    let (read_socket, mut write_socket) = socket.into_split();

    let mut buffered_reads = BufReader::new(read_socket).lines();

    let mut user_logged_in: Option<i64> = None;

    loop {
        tokio::select! {
            read_input = buffered_reads.next_line() => {
                match read_input {
                    Ok(Some(line)) => {
                        if let Some(id) = user_logged_in  {
                            match parse_client_msg(&line) {
                                Some(ClientMsg::AddAlert(alert)) => {
                                    info!("[user: {}] Alert Request:  {:?}{}{}", id, alert.direction, alert.symbol, alert.threshold);
                                    if let Err(e) = prepare_new_alert(&pool, id, &alert, &map_pointer, &mut write_socket).await {
                                        error!("[server-database] Failed to add alert to database! {}", e);
                                    }
                                },
                                Some(ClientMsg::RemoveAlert{symbol, direction}) => {
                                    info!("[user: {}] Remove Alert: {}{:?}", id, symbol, direction);
                                    if let Err(e) = database::remove_alert(&pool, id, &symbol, direction).await {
                                        error!("[server-database] Failed to remove from database! {}", e);
                                        if let Err(socket_err) = client_errors(&e, &mut write_socket).await {
                                            error!("[server] Socket error: {}", socket_err);
                                            break;
                                        }
                                    }
                                    let message = ServerMsg::AlertRemoved{symbol, direction}.to_wire();
                                    if let Err(e) = send_data(message, &mut write_socket).await {
                                        error!("[server] Network error: {}", e);
                                    }
                                },
                                Some(ClientMsg::LoginClient{username, password: _}) => {
                                    warn!("[user: {}] User already logged-in: {}", id, username);
                                    if let Err(z) = client_errors("You are arleady logged-in!", &mut write_socket).await {
                                        error!("[server] Network error: {}", z);
                                    }
                                },
                                Some(ClientMsg::RegisterClient{username, password: _}) => {
                                    warn!("[user: {}] User already registered: {}", id, username);
                                    if let Err(z) = client_errors("You are arleady logged-in!", &mut write_socket).await {
                                        error!("[server] Network error: {}", z);
                                    }
                                },
                                Some(ClientMsg::CheckPrice{symbol}) => {
                                    info!("[user: {}] Check price: {}", id, symbol);
                                    if let Err(z) = check_price(&symbol, &map_pointer, &mut write_socket).await {
                                        error!("[server] Network error: {}", z);
                                    }
                                },
                                Some(ClientMsg::SellStock{symbol, quantity}) => {
                                    info!("[user: {}] Sell stock: {} {}", id, symbol, quantity);
                                    if let Some(price) = check_price_of_stock(&map_pointer, &symbol).await {
                                        if let Err(e) = database::sell_stock(&pool, id, &symbol, quantity, price).await {
                                            error!("[server-database] Database error! {}", e);
                                            if let Err(z) = client_errors(&e, &mut write_socket).await {
                                                error!("[server] Network error: {}", z);
                                            }
                                        }
                                        else {
                                            let message = ServerMsg::StockSold { symbol, quantity }.to_wire();
                                            if let Err(e) = send_data(message, &mut write_socket).await {
                                                error!("[server] Network error: {}", e);
                                            }
                                        }
                                    }
                                    else if let Err(z) = client_errors("Stock not available!", &mut write_socket).await {
                                            error!("[server] Network error: {}", z);

                                    }

                                },
                                Some(ClientMsg::BuyStock{symbol, quantity}) => {
                                    info!("[user: {}] Buy stock: {} {}", id, symbol, quantity);
                                    if let Some(price) = check_price_of_stock(&map_pointer, &symbol).await {
                                        if let Err(e) = database::buy_stock(&pool, id, &symbol, quantity, price).await {
                                            error!("[server-database] Database error! {}", e);
                                            if let Err(z) = client_errors(&e, &mut write_socket).await {
                                                error!("[server] Network error: {}", z);
                                            }
                                        }

                                        let message = ServerMsg::StockBought { symbol, quantity }.to_wire();
                                        if let Err(e) = send_data(message, &mut write_socket).await {
                                            error!("[server] Network error: {}", e);
                                        }
                                    }
                                    else if let Err(z) = client_errors("Stock not available!", &mut write_socket).await {
                                            error!("[server] Network error: {}", z);

                                    }
                                },
                                Some(ClientMsg::GetAllClientData) => {
                                    info!("[user: {}] DATA", id);
                                    let stocks_fut = database::get_portfolio(&pool, id);
                                    let alerts_fut = database::get_user_alerts(&pool, id);

                                    match tokio::try_join!(stocks_fut, alerts_fut) {
                                        Ok((stocks, alerts)) => {
                                            let message = ServerMsg::AllClientData { stocks, alerts }.to_wire();

                                            if let Err(e) = send_data(message, &mut write_socket).await {
                                                error!("[server] Network error: {}", e);
                                            }
                                        },
                                        Err(e) => {
                                            error!("[server-database] Database error! {}", e);
                                            if let Err(z) = client_errors(&e, &mut write_socket).await {
                                                error!("[server] Network error sending error msg: {}", z);
                                            }
                                        }
                                    }
                                },
                                None => {
                                    warn!("[user: {}] Wrong command!", id);
                                    if let Err(e) = client_errors("Wrong command!", &mut write_socket).await {
                                        error!("[server] Network error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                        else {
                            match parse_client_msg(&line) {
                                Some(ClientMsg::LoginClient{username, password}) => {
                                    info!("New log-in request!");
                                    match database::login_user(&pool, &username, &password).await {
                                        Ok(id) => {
                                            user_logged_in = Some(id);
                                            let message = ServerMsg::UserLogged.to_wire();
                                            if let Err(e) = send_data(message, &mut write_socket).await {
                                                error!("[server] Network error: {}", e);
                                                break;
                                            }
                                        },
                                        Err(e) => {
                                            if let Err(z) = client_errors("Failed to log-in!", &mut write_socket).await {
                                                error!("[server] Network error: {}", z);
                                            }
                                            warn!("[server] Failed to log-in the client {}", e);
                                        }
                                    }
                                },
                                Some(ClientMsg::RegisterClient{username, password}) => {
                                    info!("New register request!");
                                    match database::register_user(&pool, &username, &password).await {
                                        Ok(_) => {
                                            let message = ServerMsg::UserRegistered.to_wire();
                                            if let Err(e) = send_data(message, &mut write_socket).await {
                                                error!("[server] Network error: {}", e);
                                                break;
                                            }
                                        },
                                        Err(e) => {
                                            if let Err(z) = client_errors("Failed to register!", &mut write_socket).await {
                                                error!("[server] Network error: {}", z);
                                            }
                                            warn!("[server] Failed to register client {}", e);
                                        }
                                    }
                                },
                                _ => {
                                      if let Err(e) = client_errors("User not logged in!", &mut write_socket).await {
                                        error!("[server] Network error: {}", e);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                    Ok(None) => {
                       info!("[server] Client gracefully disconnected, ending current connection!");
                       break;
                    }
                    Err(e) => {
                        error!("[server] Network error: {}", e);
                        break;
                    }
                }
            }
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                info!("[server] Sending alerts to client!");
                if let Some(uid) = user_logged_in {
                    info!("[server] Checking alerts for user {}", uid);
                    if let Err(e) = check_alerts_for_user(&pool, uid, &map_pointer, &mut write_socket).await {
                        error!("[server] Network error: {}", e);
                        break;
                    }
                }
            }

        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();

    let db_opts = SqliteConnectOptions::new()
        .filename("database.db")
        .create_if_missing(true);

    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(db_opts)
        .await
        .context("[server-database] Failed to connect to the database!")?;

    if let Err(e) = database::init_database(&pool).await {
        error!("[server-database] Database Init error! {}", e);
    }

    let stock_symbols = read_all_stocks();

    let stock_map: MapLock = Arc::new(RwLock::new(HashMap::new()));

    let stock_map_clone = stock_map.clone();
    tokio::spawn(async move {
        if let Err(e) = scrap_stocks(stock_map_clone, stock_symbols).await {
            error!("[server-scrapper] Scrapper failed {}", e);
        }
    });

    info!("[server] Server runs. Press CTR + C to stop it.");

    let listener = TcpListener::bind("127.0.0.1:1234")
        .await
        .context("[server] Failed to bind")?;

    // Waiting for either new client or closing argument.
    loop {
        tokio::select! {
            listener = listener.accept() => {
                match listener {
                    Ok((socket, addr)) => {
                        info!("[server] New connection from: {}", addr);
                        let stock_map_client_clone = stock_map.clone();
                        let pool_client = pool.clone();

                        tokio::spawn(async move {
                            handle_client(socket, stock_map_client_clone, pool_client).await;
                        });
                    }
                    Err(_) => warn!("[server] Invlid incoming connection!")
                }
            }
            _ = tokio::signal::ctrl_c() => {
                break;
            }
        }
    }
    Ok(())
}
