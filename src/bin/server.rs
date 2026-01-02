use std::time::Duration;
use reqwest::header::USER_AGENT;
use reqwest::header::ACCEPT;
use rust_huge_project::protocol::Price;
use tokio::io;
use tokio::net::{TcpListener, TcpStream};
use serde::{Deserialize};
use std::collections::HashMap;
use std::sync::{Arc};
use tokio::sync::RwLock;
use std::fs;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use rust_huge_project::protocol::parse_client_msg;
use tokio::net::tcp::OwnedWriteHalf;
use rust_huge_project::protocol::{
    AlertDirection, ClientMsg, ServerMsg,
};


type MapLock = Arc<RwLock<HashMap<String, f64>>>;

#[derive(Debug, Deserialize)]
struct YahooResponse {
    chart : Chart
}

#[derive(Debug, Deserialize)]
struct Chart {
    result : Vec<ChartResult>,
    // error : Option<serde_json::Value>
}

#[derive(Debug, Deserialize)]
struct ChartResult {
    meta : Meta,
    // timestamp: Option<Vec<i64>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")] 
struct Meta {
    currency: String,
    symbol: String,
    regular_market_price: f64,
    // previous_close: f64,
    // regular_market_time: i64,
}

fn read_all_stocks() -> Vec<String> {
    let file = fs::read_to_string("stocks_small.txt").expect("Couldn't open a file");

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
        println!("[server scrapper] STARTING SCRAPPING");
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
                    if request.status().is_success() {
                        let yahoo_response : Result<YahooResponse, _> = request.json().await;
                        match yahoo_response {
                            Ok(yahoo_response) => {
                                let yahoo_chart = yahoo_response.chart;

                                if let Some(stock_data) = yahoo_chart.result.first() {
                                    println!("[server scrapper] Stock symbol and currency: {} {}", stock_data.meta.symbol, stock_data.meta.currency);
                                    println!("[server scrapper] Stock price {}", stock_data.meta.regular_market_price);
                                    temp_map.insert(stock_data.meta.symbol.clone(), stock_data.meta.regular_market_price);
                                }
                            }
                            Err(error) => println!("[server scrapper] Failed Json convertion: {}", error)
                        }
                    }
                    else {
                        println!("[server scrapper] Request not succesfull!");
                    }
                }
                Err(error) => println!("[server scrapper] Scrapping network error: {}", error)

            }
            tokio::time::sleep(Duration::from_millis(10)).await;
            
        }

        if !temp_map.is_empty() {
            let mut writer = stock_map.write().await;

            writer.extend(temp_map);
        }

        println!("[server] Completed scrapping all NASDAQ stocks, clients may join!");

        tokio::time::sleep(Duration::from_secs(60)).await;
    }
}

async fn client_errors(error_message : &str, write_socket :&mut OwnedWriteHalf) -> io::Result<()> {
    let message = ServerMsg::Error(error_message.to_string()).to_wire();
    write_socket.write_all(message.as_bytes()).await?;
    write_socket.flush().await?;

    Ok(())
}

async fn handle_client_requests(user_list : &mut HashMap<String, (AlertDirection , f64)>, map_pointer : &MapLock, write_socket :&mut OwnedWriteHalf) -> io::Result<()> {
    let access = map_pointer.read().await;
    
    let mut invlid_stock : (String, bool) = (String::new(), false);
    for (stock, (direction, price)) in user_list.iter() {
        match access.get(stock) {
            Some(current_value) => {
                let triggered = match direction {
                    AlertDirection::Above => *current_value > *price,
                    AlertDirection::Below => *current_value < *price,
                };
                if triggered {
                    let message = ServerMsg::AlertTriggered { symbol: stock.to_string(), direction: *direction, threshold: *price, current_price: Price { value: *current_value } }.to_wire();
                    write_socket.write_all(message.as_bytes()).await?;
                    write_socket.flush().await?;
                }
            },
            None => {
                client_errors("Stock not available!", write_socket).await?;
                invlid_stock.0 = stock.clone();
                invlid_stock.1 = true;
            }
        }
    }

    user_list.remove(&invlid_stock.0); 
    Ok(())
                
}

async fn handle_client(socket : TcpStream, map_pointer : MapLock) {
    let (read_socket, mut write_socket) = socket.into_split();

    let mut buffered_reads = BufReader::new(read_socket).lines();

    let mut user_list : HashMap<String, (AlertDirection , f64)>  = HashMap::new();

    loop {
        tokio::select! {
            read_input = buffered_reads.next_line() => {
                match read_input {
                    Ok(Some(line)) => {
                        match parse_client_msg(&line) {
                            Some(ClientMsg::AddAlert(alert)) => {
                                println!("AlertRequest :  {:?}{}{}", alert.direction, alert.symbol, alert.threshold);
                                user_list.insert(alert.symbol, (alert.direction, alert.threshold));
                                if let Err(e) = handle_client_requests(&mut user_list, &map_pointer, &mut write_socket).await {
                                    println!("[server] Network error: {}", e);
                                    break;
                                }
                            },  
                            Some(ClientMsg::RemoveAlert{symbol, direction}) => {
                                println!("Remove Alert : {}{:?}", symbol, direction);
                                if user_list.contains_key(&symbol) {
                                    user_list.remove(&symbol);
                                }
                            },
                            Some(ClientMsg::LoginClient { .. }) => {
                                if let Err(e) = client_errors("LOGIN not supported on server.", &mut write_socket).await {
                                    println!("[server] Network error: {}", e);
                                    break;
                                }
                            }
                            Some(ClientMsg::RegisterClient { .. }) => {
                                if let Err(e) = client_errors("REGISTER not supported on server.", &mut write_socket).await {
                                    println!("[server] Network error: {}", e);
                                    break;
                                }
                            }
                            None => {
                                if let Err(e) = client_errors("Wrong command!", &mut write_socket).await {
                                    println!("[server] Network error: {}", e);
                                    break;
                                }
                            }
                        }
                    }
                    Ok(None) => {
                       println!("[server] Client gracefully disconnected, ending current connection!");
                       break;
                    }
                    Err(e) => {
                        println!("[server] Network error: {}", e);
                        break; 
                    }
                }
            }   
            _ = tokio::time::sleep(Duration::from_secs(60)) => {
                println!("[server] Sending alerts to client!");
                if let Err(e) = handle_client_requests(&mut user_list, &map_pointer, &mut write_socket).await {
                    println!("[server] Network error: {}", e);
                    break;
                }
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

    println!("[server] Server runs. Press CTR + C to stop it.");
    
    let listener = TcpListener::bind("127.0.0.1:1234").await.unwrap();


    // Waiting for either new client or closing argument.
    loop {
        tokio::select! {
            listener = listener.accept() => {
                match listener {
                    Ok((socket, addr)) => {
                        println!("[server] New connection from: {}", addr);
                        let stock_map_client_clone = stock_map.clone();

                        tokio::spawn(async move {
                            handle_client(socket, stock_map_client_clone).await;
                        });
                    }
                    Err(_) => println!("[server] Invlid incoming connection!")
                }
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
