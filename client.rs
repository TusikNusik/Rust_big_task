use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;

mod protocol;
use protocol::{parse_server_msg, AlertDirection, AlertRequest, ClientMsg, ServerMsg};

/// A minimal, text-based TCP client for iteration 1.
/// It supports:
/// - sending example commands to the server (ADD / DEL)
/// - receiving ALERT TRIGGER and ERR messages
/// - simple interactive stdin loop
#[tokio::main]
async fn main() -> io::Result<()> {
    // Server
    let addr = "127.0.0.1:1234";

    // Connect to the server over TCP.
    let stream = TcpStream::connect(addr).await?;
    println!("[client] Connected to {addr}");

    // Split the TCP stream into read/write halves,
    // so we can read and write concurrently.
    let (read_half, mut write_half) = stream.into_split();

    // Reader for server lines (newline-delimited protocol).
    let mut server_lines = BufReader::new(read_half).lines();

    // Reader for user input from stdin.
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
                                // Serialize to wire format and send to the server.
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
                        // EOF from stdin
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
/// The syntax mirrors the wire protocol so it's easy to test.
fn print_help() {
    println!("Commands:");
    println!("  add <SYMBOL> <ABOVE|BELOW> <THRESHOLD>");
    println!("  del <SYMBOL> <ABOVE|BELOW>");
    println!("  help");
    println!("  quit");
    println!();
    println!("Examples:");
    println!("  add AAPL ABOVE 200");
    println!("  add TSLA BELOW 150");
    println!("  del AAPL ABOVE");
    println!();
}

/// Parses a user command into a ClientMsg.
/// We keep it minimal for iteration 1.
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

/// Handles one line received from the server.
/// Uses the provided parse_server_msg from protocol.rs.
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
            // Unknown line. Printing for debug
            println!("[client] Unparsed server line: {line}");
        }
    }
}
