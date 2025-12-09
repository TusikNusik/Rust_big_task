use std::hash::Hash;
use std::time::Duration;
use reqwest::header::USER_AGENT;
use reqwest::header::ACCEPT;
use axum::{Json};
use reqwest;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

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
async fn scrap_stocks(stock_map : MapLock) -> Result<(), reqwest::Error> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()?;

    let url_base = "https://query1.finance.yahoo.com/v8/finance/chart/";

    let some_stocks = ["WBD", "NFLX", "SOFI", "NVDA", "HBI"];

    loop {
        println!("STARTING SCRAPPING");
        let mut temp_map = HashMap::new();

        for i in some_stocks {
            let url = format!("{}{}", url_base, i);

            let request = client.get(url)
                .header(USER_AGENT, "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7)")
                .header(ACCEPT, "application/json")
                .send()
                .await?;

            let request_code = request.status();
            let yahoo_response : YahooResponse = request.json().await?;
            
            let yahoo_chart = yahoo_response.chart;

            if let Some(stock_data) = yahoo_chart.result.first() {
                println!("Stock symbol and currency: {} {}", stock_data.meta.symbol, stock_data.meta.currency);
                println!("Stock price {}", stock_data.meta.regular_market_price);
                temp_map.insert(stock_data.meta.symbol.clone(), stock_data.meta.regular_market_price);
            }

            println!("Response code : {}", request_code);
        }

        if temp_map.len() != 0 {
            let mut writer = stock_map.write().unwrap();

            writer.extend(temp_map);
        }

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn handle_client(socket : TcpStream, map_pointer : MapLock) {
    let (mut read_socket, mut write_socket) = io::split(socket);

        
}

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {

    let stock_map : MapLock = Arc::new(RwLock::new(HashMap::new()));

    let stock_map_clone = stock_map.clone();
    tokio::spawn(async move {
        let _ = scrap_stocks(stock_map_clone).await;
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
