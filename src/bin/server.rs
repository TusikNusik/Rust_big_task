use axum::Error;
use axum::Json;
use reqwest;
use reqwest::header::ACCEPT;
use reqwest::header::USER_AGENT;
use rust_huge_project::protocol::Price;
use rust_huge_project::protocol::parse_client_msg;
use rust_huge_project::protocol::{
    AlertDirection, AlertRequest, ClientMsg, ServerMsg, parse_server_msg,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::fs::read;
use std::hash::Hash;
use std::sync::Arc;
use std::time::Duration;
use tokio::io;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::tcp::OwnedWriteHalf;
use tokio::net::tcp::WriteHalf;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

type MapLock = Arc<RwLock<HashMap<String, f64>>>;

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart: Chart,
}

#[derive(Debug, Deserialize)]
struct Chart {
    result: Vec<ChartResult>,
    error: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta: Meta,
    timestamp: Option<Vec<i64>>,
    //indicators: Indicators,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    currency: String,
    symbol: String,
    regular_market_price: f64,
    previous_close: f64,
    regular_market_time: i64,
}

fn read_all_stocks() -> Vec<String> {
    let file = fs::read_to_string("stocks.txt").expect("Couldn't open a file");

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
        println!("STARTING SCRAPPING");
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
                    let request_code = request.status();
                    if request.status().is_success() {
                        let yahoo_response: Result<YahooResponse, _> = request.json().await;
                        match yahoo_response {
                            Ok(yahoo_response) => {
                                let yahoo_chart = yahoo_response.chart;

                                if let Some(stock_data) = yahoo_chart.result.first() {
                                    println!(
                                        "Stock symbol and currency: {} {}",
                                        stock_data.meta.symbol, stock_data.meta.currency
                                    );
                                    println!(
                                        "Stock price {}",
                                        stock_data.meta.regular_market_price
                                    );
                                    temp_map.insert(
                                        stock_data.meta.symbol.clone(),
                                        stock_data.meta.regular_market_price,
                                    );
                                }
                                //println!("Response code : {}", request_code);
                            }
                            Err(error) => println!("Fialed Json convertion: {}", error),
                        }
                    } else {
                        println!("Request not succesfull!");
                    }
                }
                Err(error) => println!("Network error: {}", error),
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        if temp_map.len() != 0 {
            let mut writer = stock_map.write().await;

            writer.extend(temp_map);
        }

        println!("[server] Completed scrapping all NASDAQ stocks, clients may join!");

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn handle_client_requests(
    user_list: &HashMap<String, (AlertDirection, f64)>,
    map_pointer: &MapLock,
    write_socket: &mut OwnedWriteHalf,
) -> io::Result<()> {
    let access = map_pointer.read().await;

    for (stock, (direction, price)) in user_list {
        println!("{:?}", access);
        match access.get(stock) {
            Some(current_value) => {
                let triggered = match direction {
                    AlertDirection::Above => *current_value > *price,
                    AlertDirection::Below => *current_value < *price,
                };
                if triggered {
                    let message = ServerMsg::AlertTriggered {
                        symbol: stock.to_string(),
                        direction: *direction,
                        threshold: *price,
                        current_price: Price {
                            value: *current_value,
                        },
                    }
                    .to_wire();
                    write_socket.write_all(message.as_bytes()).await?;
                    write_socket.flush().await?;
                }
            }
            None => println!("Stock not available!"),
        }
    }
    Ok(())
}

async fn handle_client(socket: TcpStream, map_pointer: MapLock) -> io::Result<()> {
    println!("[server] New client connected!");
    let (read_socket, mut write_socket) = socket.into_split();

    let mut buffered_reads = BufReader::new(read_socket).lines();

    let mut user_list: HashMap<String, (AlertDirection, f64)> = HashMap::new();

    loop {
        tokio::select! {
            read_input = buffered_reads.next_line() => {
                match read_input? {
                    Some(line) => {
                        match parse_client_msg(&line) {
                            Some(ClientMsg::AddAlert(alert)) => {
                                println!("AlertRequest :  {:?}{}{}", alert.direction, alert.symbol, alert.threshold);
                                user_list.insert(alert.symbol, (alert.direction, alert.threshold));
                                handle_client_requests(&user_list, &map_pointer, &mut write_socket).await;
                            },
                            Some(ClientMsg::RemoveAlert{symbol, direction}) => {
                                println!("Remove Alert : {}{:?}", symbol, direction);
                                if user_list.contains_key(&symbol) {
                                    user_list.remove(&symbol);
                                }
                            },
                            None => println!("[server] Haven't reeceived a suitable command")
                        }
                    }
                    None => println!("[server] Failed to receive the message")
                }
            }

            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                println!("Checking if sending alert is possible!");
                handle_client_requests(&user_list, &map_pointer, &mut write_socket).await;
            }

        }
    }
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let stock_symbols = read_all_stocks();

    let stock_map: MapLock = Arc::new(RwLock::new(HashMap::new()));

    let stock_map_clone = stock_map.clone();
    tokio::spawn(async move {
        let _ = scrap_stocks(stock_map_clone, stock_symbols).await;
    });

    println!("Program uruchomiony. Naciśnij Ctrl+C aby zakończyć.");

    let listener = TcpListener::bind("127.0.0.1:1234").await.unwrap();

    loop {
    	tokio::select! {
    		new = listener.accept() => {
				let (socket, _) = new.unwrap();

				let stock_map_client_clone = stock_map.clone();

				tokio::spawn(async move {
					handle_client(socket, stock_map_client_clone).await;
				});
			}
			
			_ = tokio::signal::ctrl_c() => {
				break;
			}
		}
    }
    
    Ok(())
}
/*
{"chart":
    {"result": [
        {"meta":
            {"currency": "USD","symbol":"NFLX","exchangeName":"NMS","fullExchangeName":"NasdaqGS","instrumentType":"EQUITY","firstTradeDate":1022160600,"regularMarketTime":1764968400,"hasPrePostMarketData":true,"gmtoffset":-18000,"timezone":"EST","exchangeTimezoneName":"America/New_York","regularMarketPrice":100.24,"fiftyTwoWeekHigh":134.115,"fiftyTwoWeekLow":82.11,"regularMarketDayHigh":104.79,"regularMarketDayLow":97.74,"regularMarketVolume":132718362,"longName":"Netflix, Inc.","shortName":"Netflix, Inc.","chartPreviousClose":103.22,"previousClose":103.22,"scale":3,"priceHint":2,
                "currentTradingPeriod":{
                    "pre": {"timezone":"EST","start":1764925200,"end":1764945000,"gmtoffset":-18000},
                    "regular":{"timezone":"EST","start":1764945000,"end":1764968400,"gmtoffset":-18000},
                    "post":{"timezone":"EST","start":1764968400,"end":1764982800,"gmtoffset":-18000}},
                    "tradingPeriods":[[{"timezone":"EST","start":1764945000,"end":1764968400,"gmtoffset":-18000}]],
                    "dataGranularity":"1m","range":"1d","validRanges":["1d","5d","1mo","3mo","6mo","1y","2y","5y","10y","ytd","max"]
                }



*/
