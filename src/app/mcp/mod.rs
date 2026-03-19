//! MCP (Model Context Protocol) server for editor integration.
//!
//! Provides two modes:
//! 1. **TCP server** (runs in GUI mode): Background thread accepting MCP requests
//!    and dispatching them to the main thread via `Message::McpRequest`.
//! 2. **Bridge mode** (`--mcp-server`): Stdin/stdout ↔ TCP bridge for Claude Code.

pub mod protocol;
pub mod tools;

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use fltk::app::Sender;

use super::domain::messages::Message;

/// Port file location: `~/.config/ferrispad/mcp-port`
pub(crate) fn port_file_path() -> Option<std::path::PathBuf> {
    dirs::config_dir().map(|d| d.join("ferrispad").join("mcp-port"))
}

/// Get the current working directory as an owned String (for hook project_root).
pub(crate) fn cwd_as_string() -> Option<String> {
    std::env::current_dir()
        .ok()
        .map(|p| p.to_string_lossy().into_owned())
}

/// Shared response channels: request_id → SyncSender<String>
pub type McpResponses = Arc<Mutex<HashMap<u64, std::sync::mpsc::SyncSender<String>>>>;

/// Global request ID counter
static NEXT_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

/// Start the MCP TCP server in a background thread.
/// Returns the allocated port and the response channel map, or None if binding fails.
pub fn start_tcp_server(sender: Sender<Message>) -> Option<(u16, McpResponses)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(l) => l,
        Err(e) => {
            eprintln!("[mcp] Failed to start TCP server: {}", e);
            return None;
        }
    };
    let port = listener.local_addr().ok()?.port();

    // Write port to file
    if let Some(path) = port_file_path() {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&path, port.to_string());
    }

    let responses: McpResponses = Arc::new(Mutex::new(HashMap::new()));
    let responses_clone = responses.clone();

    std::thread::spawn(move || {
        // Accept connections one at a time
        for stream in listener.incoming() {
            let stream = match stream {
                Ok(s) => s,
                Err(_) => continue,
            };
            handle_tcp_connection(stream, sender, &responses_clone);
        }
    });

    Some((port, responses))
}

/// Handle a single TCP connection: read newline-delimited JSON, dispatch, respond.
fn handle_tcp_connection(stream: TcpStream, sender: Sender<Message>, responses: &McpResponses) {
    let reader_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return,
    };
    let mut writer = stream;
    let reader = BufReader::new(reader_stream);

    for line in reader.lines() {
        let line = match line {
            Ok(l) => l,
            Err(_) => break,
        };

        if line.trim().is_empty() {
            continue;
        }

        // Parse JSON-RPC envelope
        let parsed: serde_json::Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let method = parsed
            .get("method")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();
        let id = parsed.get("id").cloned();
        let params = parsed
            .get("params")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        // Notifications (no id) don't need a response
        if id.is_none() {
            // Handle notifications like initialized
            continue;
        }

        let json_rpc_id = id.unwrap();
        let request_id = NEXT_REQUEST_ID.fetch_add(1, Ordering::Relaxed);

        // Create response channel
        let (tx, rx) = std::sync::mpsc::sync_channel::<String>(1);
        responses.lock().unwrap().insert(request_id, tx);

        // Dispatch to main thread
        sender.send(Message::McpRequest {
            request_id,
            json_rpc_id: json_rpc_id.clone(),
            method,
            params,
        });

        // Wait for response (timeout 10s)
        let response_body = match rx.recv_timeout(std::time::Duration::from_secs(10)) {
            Ok(r) => r,
            Err(_) => {
                responses.lock().unwrap().remove(&request_id);
                protocol::json_rpc_error(&json_rpc_id, -32000, "Request timed out")
            }
        };

        let mut out = response_body;
        out.push('\n');
        if writer.write_all(out.as_bytes()).is_err() {
            break;
        }
        let _ = writer.flush();
    }
}

/// Bridge mode: read from stdin, forward to TCP; read from TCP, forward to stdout.
/// This is the `--mcp-server` entry point (no GUI).
pub fn run_bridge() -> ! {
    // Read port from file
    let port = port_file_path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .and_then(|s| s.trim().parse::<u16>().ok());

    let port = match port {
        Some(p) => p,
        None => {
            eprintln!("FerrisPad MCP: No port file found. Is FerrisPad running?");
            std::process::exit(1);
        }
    };

    let stream = match TcpStream::connect(format!("127.0.0.1:{}", port)) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrisPad MCP: Failed to connect to port {}: {}", port, e);
            std::process::exit(1);
        }
    };

    let tcp_reader = match stream.try_clone() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrisPad MCP: Failed to clone stream: {}", e);
            std::process::exit(1);
        }
    };
    let mut tcp_writer = stream;

    // Thread: TCP → stdout
    let handle = std::thread::spawn(move || {
        let reader = BufReader::new(tcp_reader);
        let stdout = std::io::stdout();
        let mut stdout = stdout.lock();
        for line in reader.lines() {
            match line {
                Ok(l) => {
                    if writeln!(stdout, "{}", l).is_err() {
                        break;
                    }
                    let _ = stdout.flush();
                }
                Err(_) => break,
            }
        }
    });

    // Main thread: stdin → TCP
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());
    for line in reader.lines() {
        match line {
            Ok(l) => {
                let mut data = l;
                data.push('\n');
                if tcp_writer.write_all(data.as_bytes()).is_err() {
                    break;
                }
                let _ = tcp_writer.flush();
            }
            Err(_) => break,
        }
    }

    let _ = handle.join();
    std::process::exit(0);
}

/// Clean up the MCP port file on exit.
pub fn cleanup_port_file() {
    if let Some(path) = port_file_path() {
        let _ = std::fs::remove_file(path);
    }
}
