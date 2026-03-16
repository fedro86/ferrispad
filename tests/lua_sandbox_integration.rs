use std::fs;
use tempfile::tempdir;

use ferris_pad::app::plugins::runtime::LuaRuntime;

#[test]
fn test_load_script_and_call_hooks() {
    let dir = tempdir().unwrap();
    let init_lua = dir.path().join("init.lua");
    fs::write(
        &init_lua,
        r#"
        local M = {
            name = "test-plugin",
            version = "1.0.0",
        }

        function M.on_document_open(path)
            return { status = "opened: " .. (path or "nil") }
        end

        function M.on_document_save(path)
            return { status = "saved" }
        end

        return M
        "#,
    )
    .unwrap();

    let runtime = LuaRuntime::new().unwrap();
    let table = runtime.load_script(&init_lua).unwrap();

    // Verify metadata
    let name: String = table.get("name").unwrap();
    assert_eq!(name, "test-plugin");

    // Call on_document_open hook
    let result = runtime
        .call_hook(&table, "on_document_open", "/tmp/test.rs")
        .unwrap();
    assert!(!result.is_nil());

    // Call non-existent hook -> Nil
    let nil_result = runtime
        .call_hook(&table, "on_nonexistent", ())
        .unwrap();
    assert!(nil_result.is_nil());
}

#[test]
fn test_sandbox_blocks_os_library() {
    let dir = tempdir().unwrap();
    let init_lua = dir.path().join("init.lua");
    fs::write(
        &init_lua,
        r#"
        local M = {}
        function M.on_document_open()
            os.execute("echo pwned")
            return {}
        end
        return M
        "#,
    )
    .unwrap();

    let runtime = LuaRuntime::new().unwrap();
    let table = runtime.load_script(&init_lua).unwrap();

    let result = runtime.call_hook(&table, "on_document_open", ());
    assert!(result.is_err(), "os.execute should be blocked by sandbox");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("nil") || err_msg.contains("attempt to index"),
        "Expected nil/index error for os, got: {}",
        err_msg
    );
}

#[test]
fn test_sandbox_blocks_io_library() {
    let dir = tempdir().unwrap();
    let init_lua = dir.path().join("init.lua");
    fs::write(
        &init_lua,
        r#"
        local M = {}
        function M.on_document_open()
            local f = io.open("/etc/passwd", "r")
            return {}
        end
        return M
        "#,
    )
    .unwrap();

    let runtime = LuaRuntime::new().unwrap();
    let table = runtime.load_script(&init_lua).unwrap();

    let result = runtime.call_hook(&table, "on_document_open", ());
    assert!(result.is_err(), "io.open should be blocked by sandbox");
}

#[test]
fn test_instruction_limit_aborts_loop() {
    let dir = tempdir().unwrap();
    let init_lua = dir.path().join("init.lua");
    fs::write(
        &init_lua,
        r#"
        local M = {}
        function M.on_document_open()
            while true do end
        end
        return M
        "#,
    )
    .unwrap();

    // Use a low instruction limit for fast test
    let runtime = LuaRuntime::with_instruction_limit(10_000).unwrap();
    let table = runtime.load_script(&init_lua).unwrap();

    let result = runtime.call_hook(&table, "on_document_open", ());
    assert!(result.is_err(), "Infinite loop should be aborted");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("Instruction limit exceeded"),
        "Expected instruction limit error, got: {}",
        err_msg
    );
}

#[test]
fn test_memory_limit_prevents_allocation() {
    let dir = tempdir().unwrap();
    let init_lua = dir.path().join("init.lua");
    fs::write(
        &init_lua,
        r#"
        local M = {}
        function M.on_document_open()
            local t = {}
            for i = 1, 1000000 do
                t[i] = string.rep("x", 1000)
            end
            return {}
        end
        return M
        "#,
    )
    .unwrap();

    // Small memory limit (100KB)
    let runtime = LuaRuntime::with_limits(1_000_000, 100 * 1024).unwrap();
    let table = runtime.load_script(&init_lua).unwrap();

    let result = runtime.call_hook(&table, "on_document_open", ());
    assert!(result.is_err(), "Excessive allocation should fail");
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("memory"),
        "Expected memory error, got: {}",
        err_msg
    );
}
