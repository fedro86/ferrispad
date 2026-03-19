//! MCP JSON-RPC 2.0 protocol handling.

use serde_json::{Value, json};

/// Handle the `initialize` method.
pub fn handle_initialize(id: &Value) -> String {
    let result = json!({
        "protocolVersion": "2024-11-05",
        "capabilities": {
            "tools": {}
        },
        "serverInfo": {
            "name": "ferrispad",
            "version": env!("CARGO_PKG_VERSION")
        }
    });
    json_rpc_result(id, result)
}

/// Build a JSON-RPC success response.
pub fn json_rpc_result(id: &Value, result: Value) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "result": result
    }))
    .unwrap()
}

/// Build a JSON-RPC error response.
pub fn json_rpc_error(id: &Value, code: i32, message: &str) -> String {
    serde_json::to_string(&json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    }))
    .unwrap()
}
