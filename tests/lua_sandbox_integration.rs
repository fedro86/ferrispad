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
    let nil_result = runtime.call_hook(&table, "on_nonexistent", ()).unwrap();
    assert!(nil_result.is_nil());
}

// Regression (T0009, audit M2): plugins loaded into the same runtime must not
// see each other's globals or monkey-patch each other's standard library. Two
// plugins share one runtime; A leaks a global and patches `string.rep`, B must
// see neither. Before per-plugin environment isolation, A wrote straight into
// the shared globals, so B observed `LEAKED == "from A"` and `rep == "HACKED"`.
#[test]
fn plugins_are_isolated_from_each_other() {
    let dir = tempdir().unwrap();

    // Plugin A: leak globals (bare + via _G) and monkey-patch string.rep.
    let a = dir.path().join("a.lua");
    fs::write(
        &a,
        r#"
        LEAKED = "from A"
        _G.LEAKED_G = "from A via _G"
        string.rep = function() return "HACKED" end
        local M = {}
        function M.probe()
            return { leaked = tostring(LEAKED), rep = string.rep("x", 3) }
        end
        return M
        "#,
    )
    .unwrap();

    // Plugin B: report what A tried to leak / patch.
    let b = dir.path().join("b.lua");
    fs::write(
        &b,
        r#"
        local M = {}
        function M.probe()
            return {
                leaked = tostring(LEAKED),
                leaked_g = tostring(LEAKED_G),
                rep = string.rep("x", 3),
            }
        end
        return M
        "#,
    )
    .unwrap();

    let runtime = LuaRuntime::new().unwrap();
    let ta = runtime.load_script(&a).unwrap();
    let tb = runtime.load_script(&b).unwrap();

    let as_table = |v: mlua::Value| match v {
        mlua::Value::Table(t) => t,
        other => panic!("expected a table result, got {other:?}"),
    };

    // A sees its own edits within its own environment.
    let a_res = as_table(runtime.call_hook(&ta, "probe", ()).unwrap());
    assert_eq!(a_res.get::<String>("leaked").unwrap(), "from A");
    assert_eq!(a_res.get::<String>("rep").unwrap(), "HACKED");

    // B is isolated: none of A's edits are visible.
    let b_res = as_table(runtime.call_hook(&tb, "probe", ()).unwrap());
    assert_eq!(
        b_res.get::<String>("leaked").unwrap(),
        "nil",
        "B must not see A's global LEAKED"
    );
    assert_eq!(
        b_res.get::<String>("leaked_g").unwrap(),
        "nil",
        "B must not see A's _G.LEAKED_G"
    );
    assert_eq!(
        b_res.get::<String>("rep").unwrap(),
        "xxx",
        "B's string.rep must be intact, not A's patched version"
    );
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
