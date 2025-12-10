use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

use rust_huge_project::protocol::{
    parse_server_msg, AlertDirection, AlertRequest, ClientMsg, ServerMsg,
};

#[tokio::main]
async fn main() -> io::Result<()> {
    let addr = "127.0.0.1:1234";
    let stream = TcpStream::connect(addr).await?;
    println!("[client] Connected to {addr}");

    // We split the socket so we can listen for incoming alerts 
    // and send user commands at the exact same time without locking issues.
    let (read_half, mut write_half) = stream.into_split();
    let mut server_lines = BufReader::new(read_half).lines();
    let stdin = tokio::io::stdin();
    let mut user_lines = BufReader::new(stdin).lines();

    print_help();

    loop {
        tokio::select! {
            // Handle incoming server messages.
            line = server_lines.next_line() => {
                match line? {
                    Some(line) => {
                        handle_server_line(&line);
                    }
                    None => {
                        println!("[client] Server closed the connection.");
                        break;
                    }
                }
            }

            // Handle user commands from stdin.
            line = user_lines.next_line() => {
                match line? {
                    Some(line) => {
                        let line = line.trim();
                        if line.is_empty() {
                            continue;
                        }
                        if line.eq_ignore_ascii_case("quit") || line.eq_ignore_ascii_case("exit") {
                            println!("[client] Bye.");
                            break;
                        }
                        if line.eq_ignore_ascii_case("help") {
                            print_help();
                            continue;
                        }

                        // Parse user input into a ClientMsg.
                        match parse_user_cmd(line) {
                            Some(msg) => {
                                let wire = msg.to_wire();
                                write_half.write_all(wire.as_bytes()).await?;
                                write_half.flush().await?;
                            }
                            None => {
                                println!("[client] Invalid command. Type 'help'.");
                            }
                        }
                    }
                    None => {
                        println!("[client] stdin closed.");
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

/// Prints a short help for the user.
fn print_help() {
    println!("Commands:");
    println!("  add <SYMBOL> <ABOVE|BELOW> <THRESHOLD>");
    println!("  help");
    println!("  quit");
    println!();
    println!("Examples:");
    println!("  add AAPL ABOVE 200");
    println!("  add TSLA BELOW 150");
    println!("  del AAPL ABOVE 175");
    println!();
}

/// Parses a user command into a ClientMsg.
fn parse_user_cmd(line: &str) -> Option<ClientMsg> {
    let mut parts = line.split_whitespace();
    let cmd = parts.next()?.to_ascii_lowercase();

    match cmd.as_str() {
        "add" => {
            let symbol = parts.next()?.to_string();
            let dir_str = parts.next()?;
            let direction = AlertDirection::from_str(&dir_str.to_ascii_uppercase())?;
            let threshold: f64 = parts.next()?.parse().ok()?;

            Some(ClientMsg::AddAlert(AlertRequest {
                symbol,
                direction,
                threshold,
            }))
        }

        "del" => {
            let symbol = parts.next()?.to_string();
            let dir_str = parts.next()?;
            let direction = AlertDirection::from_str(&dir_str.to_ascii_uppercase())?;

            Some(ClientMsg::RemoveAlert { symbol, direction })
        }

        _ => None,
    }
}

fn handle_server_line(line: &str) {
    match parse_server_msg(line) {
        Some(ServerMsg::AlertTriggered {
            symbol,
            direction,
            threshold,
            current_price,
        }) => {
            println!(
                "[ALERT] {symbol} {:?} threshold={} current={}",
                direction, threshold, current_price.value
            );
        }

        Some(ServerMsg::Error(msg)) => {
            println!("[SERVER ERROR] {msg}");
        }

        None => {
            println!("[client] Unparsed server line: {line}");
        }
    }
}
