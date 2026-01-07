use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use tokio::time::timeout;

use rust_huge_project::protocol::{parse_server_msg, ClientMsg, ServerMsg};

fn unique_suffix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

async fn next_msg(
    lines: &mut tokio::io::Lines<BufReader<tokio::net::tcp::OwnedReadHalf>>,
) -> ServerMsg {
    let line = timeout(Duration::from_secs(2), lines.next_line())
        .await
        .expect("timeout waiting for server")        
        .expect("failed to read line")
        .expect("server closed connection");
    parse_server_msg(&line).expect("failed to parse server message")
}

#[tokio::test]
async fn e2e_login_and_data() {
    let addr = std::env::var("SERVER_ADDR").unwrap_or_else(|_| "127.0.0.1:1234".into());
    let stream = TcpStream::connect(&addr)
        .await
        .expect("failed to connect to live server");
    let (read_half, mut write_half) = stream.into_split();
    let mut lines = BufReader::new(read_half).lines();

    let suffix = unique_suffix();
    let username = format!("user_{suffix}");
    let password = "pass123";

    let register = ClientMsg::RegisterClient {
        username: username.clone(),
        password: password.to_string(),
    };
    write_half.write_all(register.to_wire().as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();
    match next_msg(&mut lines).await {
        ServerMsg::UserRegistered => {}
        other => panic!("expected UserRegistered, got {other:?}"),
    }

    let login = ClientMsg::LoginClient {
        username: username.clone(),
        password: password.to_string(),
    };
    write_half.write_all(login.to_wire().as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();
    match next_msg(&mut lines).await {
        ServerMsg::UserLogged => {}
        other => panic!("expected UserLogged, got {other:?}"),
    }

    let data = ClientMsg::GetAllClientData;
    write_half.write_all(data.to_wire().as_bytes()).await.unwrap();
    write_half.flush().await.unwrap();
    match next_msg(&mut lines).await {
        ServerMsg::AllClientData { stocks, alerts } => {
            assert!(stocks.is_empty(), "expected empty portfolio");
            assert!(alerts.is_empty(), "expected empty alerts");
        }
        other => panic!("expected AllClientData, got {other:?}"),
    }

}
