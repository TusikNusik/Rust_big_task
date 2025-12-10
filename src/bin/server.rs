use std::fs::read;
use std::hash::Hash;
use std::time::Duration;
use axum::Error;
use reqwest::header::USER_AGENT;
use reqwest::header::ACCEPT;
use axum::{Json};
use reqwest;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use rust_huge_project::protocol::parse_client_msg;

use rust_huge_project::protocol::{
    parse_server_msg, AlertDirection, AlertRequest, ClientMsg, ServerMsg,
};


type MapLock = Arc<RwLock<HashMap<String, f64>>>;

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart : Chart
}

#[derive(Debug, Deserialize)]
struct Chart {
    result : Vec<ChartResult>,
    error : Option<serde_json::Value>
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta : Meta,
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

/* 
#[derive(Debug, Deserialize)]
struct Indicators {
    quote: Vec<Quote>,
}

// Tablice z danymi historycznymi (do wykresu)
#[derive(Debug, Deserialize)]
struct Quote {
    // Używamy Option<f64>, bo w JSONie mogą być "null" (np. przerwa w handlu)
    open: Vec<Option<f64>>,
    close: Vec<Option<f64>>,
    high: Vec<Option<f64>>,
    low: Vec<Option<f64>>,
    volume: Vec<Option<i64>>, // Wolumen to zazwyczaj liczby całkowite
}
*/
fn read_all_stocks() -> Vec<String> {
    let file = fs::read_to_string("stocks.txt").expect("Couldn't open a file");

    file.lines()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect()
}

async fn scrap_stocks(stock_map : MapLock, all_stocks : Vec<String>) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let url_base = "https://query1.finance.yahoo.com/v8/finance/chart/";

    loop {
        println!("STARTING SCRAPPING");
        let mut temp_map = HashMap::new();

        for i in &all_stocks {
            let url = format!("{}{}", url_base, i);

            let request = client.get(url)
                .header(USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
                .header(ACCEPT, "application/json")
                .send()
                .await;

            match request {
                Ok(request) => {
                    let request_code = request.status();
                    if request.status().is_success() {
                        let yahoo_response : Result<YahooResponse, _> = request.json().await;
                        match yahoo_response {
                            Ok(yahoo_response) => {
                                let yahoo_chart = yahoo_response.chart;

                                if let Some(stock_data) = yahoo_chart.result.first() {
                                    println!("Stock symbol and currency: {} {}", stock_data.meta.symbol, stock_data.meta.currency);
                                    println!("Stock price {}", stock_data.meta.regular_market_price);
                                    temp_map.insert(stock_data.meta.symbol.clone(), stock_data.meta.regular_market_price);
                                }
                                println!("Response code : {}", request_code);
                            }
                            Err(error) => println!("Fialed Json convertion: {}", error)
                        }
                    }
                    else {
                        println!("Request not succesfull!");
                    }
                }
                Err(error) => println!("Network error: {}", error)

            }
            tokio::time::sleep(Duration::from_millis(50)).await;
            
        }

        if temp_map.len() != 0 {
            let mut writer = stock_map.write().unwrap();

            writer.extend(temp_map);
        }

        tokio::time::sleep(Duration::from_mins(1)).await;
    }
}

async fn handle_client(socket : TcpStream, map_pointer : MapLock) -> io::Result<()> {
    let (read_socket, mut write_socket) = io::split(socket);

    let mut buffered_reads = BufReader::new(read_socket).lines();

    loop {
        tokio::select! {
            read_input = buffered_reads.next_line() => {
                match read_input? {
                    Some(line) => {
                        match parse_client_msg(&line) {
                            Some(ClientMsg::AddAlert(alert)) => {
                                println!("AlertRequest :  {:?}{}{}", alert.direction, alert.symbol, alert.threshold);
                            },  
                            Some(ClientMsg::RemoveAlert{symbol, direction}) => {
                                println!("Remove Alert : {}{:?}", symbol, direction);
                            },
                            None => println!("[server] Haven't reeceived a suitable command")
                        }
                    }
                    None => println!("[server] Failed to receive the message")
                }
            }   

            _ = tokio::time::sleep(Duration::from_mins(1)) => {
                println!("Checking if sending alert is possible!")
            }

        }

    }

}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {

    let stock_symbols = read_all_stocks();

    let stock_map : MapLock = Arc::new(RwLock::new(HashMap::new()));

    let stock_map_clone = stock_map.clone();
    tokio::spawn(async move {
        let _ = scrap_stocks(stock_map_clone, stock_symbols).await;
    });

    println!("Program uruchomiony. Naciśnij Ctrl+C aby zakończyć.");
    
    let listener = TcpListener::bind("127.0.0.1:1234").await.unwrap();


    loop {
        let (socket, _) = listener.accept().await.unwrap();

        let stock_map_client_clone = stock_map.clone();

        tokio::spawn(async move {
            handle_client(socket, stock_map_client_clone).await;
        });

    }
    let _ = tokio::signal::ctrl_c().await; 

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