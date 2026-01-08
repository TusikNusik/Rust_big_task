use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use rust_huge_project::protocol::{AlertDirection, ClientMsg, ServerMsg, parse_server_msg};

fn parse_or_fallback(line: &str) -> Option<ServerMsg> {
    if let Some(msg) = parse_server_msg(line) {
        return Some(msg);
    }
    let trimmed = line.trim();
    if trimmed == "USERLOGGED" {
        return Some(ServerMsg::UserLogged);
    }
    if trimmed == "USERREGISTERED" {
        return Some(ServerMsg::UserRegistered);
    }
    None
}

async fn wait_for_msg<F>(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
    label: &str,
    mut pred: F,
) -> ServerMsg
where
    F: FnMut(&ServerMsg) -> bool,
{
    let deadline = Instant::now() + Duration::from_secs(10);
    let mut last_raw: Option<String> = None;
    loop {
        let now = Instant::now();
        if now >= deadline {
            if let Some(raw) = last_raw {
                panic!("timeout waiting for {label}; last raw line: {raw}");
            }
            panic!("timeout waiting for {label}");
        }
        let remaining = deadline - now;
        let line = tokio::time::timeout(remaining, lines.next_line())
            .await
            .expect("timeout waiting for server")
            .expect("failed to read line")
            .expect("server closed connection");
        eprintln!("[test] raw line: {}", line);
        if let Some(msg) = parse_or_fallback(&line) {
            if pred(&msg) {
                return msg;
            }
            if matches!(msg, ServerMsg::Error(_)) {
                panic!("server error while waiting for {label}: {msg:?}");
            }
        } else {
            last_raw = Some(line);
        }
    }
}

#[tokio::test]
async fn e2e_live_server_flow() {
    let addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:1234".into());
    let stream = TcpStream::connect(&addr)
        .await
        .expect("failed to connect to live server");
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    let login = ClientMsg::LoginClient {
        username: "test".into(),
        password: "testtest".into(),
    };
    write_half
        .write_all(login.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    let login_msg = wait_for_msg(&mut lines, "UserLogged", |msg| {
        matches!(msg, ServerMsg::UserLogged | ServerMsg::Error(_))
    })
    .await;
    if let ServerMsg::Error(msg) = login_msg {
        panic!("login failed: {msg}");
    }

    let data = ClientMsg::GetAllClientData;
    write_half
        .write_all(data.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "AllClientData", |msg| {
        matches!(msg, ServerMsg::AllClientData { .. })
    })
    .await;

    let symbol = "AAPL";
    let price = ClientMsg::CheckPrice {
        symbol: symbol.into(),
    };
    write_half
        .write_all(price.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    let current_price = match wait_for_msg(&mut lines, "PriceChecked", |msg| {
        matches!(msg, ServerMsg::PriceChecked { .. })
    })
    .await
    {
        ServerMsg::PriceChecked { price, .. } => price,
        other => panic!("unexpected message: {other:?}"),
    };

    let add_alert = ClientMsg::AddAlert(rust_huge_project::protocol::AlertRequest {
        symbol: symbol.into(),
        direction: AlertDirection::Above,
        threshold: current_price + 1000.0,
    });
    write_half
        .write_all(add_alert.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "AlertAdded", |msg| {
        matches!(msg, ServerMsg::AlertAdded { .. })
    })
    .await;

    let del_alert = ClientMsg::RemoveAlert {
        symbol: symbol.into(),
        direction: AlertDirection::Above,
    };
    write_half
        .write_all(del_alert.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "AlertRemoved", |msg| {
        matches!(msg, ServerMsg::AlertRemoved { .. })
    })
    .await;

    let buy = ClientMsg::BuyStock {
        symbol: symbol.into(),
        quantity: 1,
    };
    write_half
        .write_all(buy.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "StockBought", |msg| {
        matches!(msg, ServerMsg::StockBought { .. })
    })
    .await;

    let sell = ClientMsg::SellStock {
        symbol: symbol.into(),
        quantity: 1,
    };
    write_half
        .write_all(sell.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "StockSold", |msg| {
        matches!(msg, ServerMsg::StockSold { .. })
    })
    .await;

    let data = ClientMsg::GetAllClientData;
    write_half
        .write_all(data.to_wire().as_bytes())
        .await
        .unwrap();
    write_half.flush().await.unwrap();
    wait_for_msg(&mut lines, "AllClientData (after trades)", |msg| {
        matches!(msg, ServerMsg::AllClientData { .. })
    })
    .await;
}
