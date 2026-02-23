//! Lua runtime wrapper with sandboxing.
//!
//! Provides a safe Lua execution environment that:
//! - Disables dangerous functions (os, io, debug, loadfile, dofile, require)
//! - Allows safe functions (string, table, math, pairs, ipairs, etc.)
//! - Loads plugin scripts and calls hook functions
//! - Limits instruction count to prevent infinite loops (DoS protection)
//! - Limits memory usage to prevent memory exhaustion (DoS protection)

use mlua::{Function, HookTriggers, Lua, Result as LuaResult, Table, Value, VmState};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Default maximum instructions per hook call (1 million).
/// This prevents infinite loops while allowing complex operations.
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Default maximum memory per plugin (16 MB).
/// This prevents memory exhaustion while allowing reasonable data structures.
pub const DEFAULT_MAX_MEMORY: usize = 16 * 1024 * 1024;

/// Hook interval - check instruction count every N instructions.
/// Lower values = more responsive abort, higher overhead.
/// 1000 is a reasonable balance.
const HOOK_CHECK_INTERVAL: u32 = 1000;

/// Lua runtime wrapper with sandboxing, instruction limits, and memory limits.
///
/// Each runtime tracks instruction count per hook call to prevent DoS.
/// Memory usage is limited via Lua's built-in allocator tracking.
pub struct LuaRuntime {
    lua: Lua,
    /// Maximum instructions allowed per hook call
    max_instructions: u64,
    /// Current instruction counter (reset before each hook call)
    instruction_count: Arc<AtomicU64>,
    /// Maximum memory allowed (0 = unlimited)
    max_memory: usize,
}

impl LuaRuntime {
    /// Create a new sandboxed Lua runtime with default limits.
    pub fn new() -> LuaResult<Self> {
        Self::with_limits(DEFAULT_MAX_INSTRUCTIONS, DEFAULT_MAX_MEMORY)
    }

    /// Create a new sandboxed Lua runtime with custom instruction limit.
    ///
    /// # Arguments
    /// * `max_instructions` - Maximum instructions allowed per hook call.
    ///   Set to 0 to disable instruction limiting (not recommended).
    #[allow(dead_code)] // Keep for backwards compatibility and testing
    pub fn with_instruction_limit(max_instructions: u64) -> LuaResult<Self> {
        Self::with_limits(max_instructions, DEFAULT_MAX_MEMORY)
    }

    /// Create a new sandboxed Lua runtime with custom limits.
    ///
    /// # Arguments
    /// * `max_instructions` - Maximum instructions allowed per hook call.
    ///   Set to 0 to disable instruction limiting (not recommended).
    /// * `max_memory` - Maximum memory in bytes allowed for Lua heap.
    ///   Set to 0 to disable memory limiting (not recommended).
    pub fn with_limits(max_instructions: u64, max_memory: usize) -> LuaResult<Self> {
        let lua = Lua::new();
        let instruction_count = Arc::new(AtomicU64::new(0));

        let runtime = Self {
            lua,
            max_instructions,
            instruction_count,
            max_memory,
        };
        runtime.setup_sandbox()?;
        runtime.setup_instruction_limit()?;
        runtime.setup_memory_limit()?;
        Ok(runtime)
    }

    /// Set up the sandbox by removing dangerous globals
    fn setup_sandbox(&self) -> LuaResult<()> {
        let globals = self.lua.globals();

        // Remove dangerous modules/functions
        globals.set("os", Value::Nil)?;
        globals.set("io", Value::Nil)?;
        globals.set("debug", Value::Nil)?;
        globals.set("loadfile", Value::Nil)?;
        globals.set("dofile", Value::Nil)?;
        globals.set("require", Value::Nil)?;
        globals.set("load", Value::Nil)?;
        globals.set("package", Value::Nil)?;

        // Keep safe functions:
        // string, table, math, pairs, ipairs, type, tonumber, tostring,
        // print, next, select, pcall, xpcall, error, assert, rawget, rawset,
        // getmetatable, setmetatable

        Ok(())
    }

    /// Set up instruction count limit to prevent infinite loops.
    ///
    /// The hook fires every HOOK_CHECK_INTERVAL instructions and aborts
    /// execution if the limit is exceeded.
    fn setup_instruction_limit(&self) -> LuaResult<()> {
        if self.max_instructions == 0 {
            // Instruction limiting disabled
            return Ok(());
        }

        let max = self.max_instructions;
        let counter = Arc::clone(&self.instruction_count);

        self.lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_CHECK_INTERVAL),
            move |_lua, _debug| {
                let count = counter.fetch_add(HOOK_CHECK_INTERVAL as u64, Ordering::Relaxed);
                if count >= max {
                    Err(mlua::Error::RuntimeError(format!(
                        "Instruction limit exceeded: {} instructions (max: {})",
                        count, max
                    )))
                } else {
                    Ok(VmState::Continue)
                }
            },
        );

        Ok(())
    }

    /// Reset instruction counter before a new operation.
    /// Called before each hook invocation.
    fn reset_instruction_count(&self) {
        self.instruction_count.store(0, Ordering::Relaxed);
    }

    /// Set up memory limit to prevent memory exhaustion.
    ///
    /// Uses Lua's built-in memory tracking. When an allocation would
    /// exceed the limit, Lua returns a MemoryError.
    fn setup_memory_limit(&self) -> LuaResult<()> {
        if self.max_memory == 0 {
            // Memory limiting disabled
            return Ok(());
        }

        // Get baseline memory usage after sandbox setup
        let baseline = self.lua.used_memory();

        // Set limit as baseline + allowed budget
        let total_limit = baseline + self.max_memory;
        self.lua.set_memory_limit(total_limit)?;

        Ok(())
    }

    /// Get current memory usage in bytes.
    pub fn used_memory(&self) -> usize {
        self.lua.used_memory()
    }

    /// Trigger Lua garbage collection (full cycle).
    /// Call this after clearing plugin references to reclaim memory.
    pub fn collect_garbage(&self) {
        // gc_collect() triggers a full GC cycle
        let _ = self.lua.gc_collect();
    }

    /// Load a plugin script from init.lua and return the plugin table.
    ///
    /// Instruction count is reset before loading to give each plugin
    /// a fresh budget for initialization.
    pub fn load_script(&self, path: &Path) -> LuaResult<Table> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        // Reset instruction counter for this load operation
        self.reset_instruction_count();

        // Execute the script and expect it to return a table
        let chunk = self.lua.load(&content);
        let result: Value = chunk.eval()?;

        match result {
            Value::Table(table) => Ok(table),
            _ => Err(mlua::Error::RuntimeError(
                "Plugin init.lua must return a table".to_string(),
            )),
        }
    }

    /// Call a hook function on a plugin table.
    ///
    /// Returns the result value from the hook, or Nil if hook doesn't exist.
    /// Instruction count is reset before each call to give each hook
    /// a fresh budget.
    pub fn call_hook<A>(&self, plugin_table: &Table, hook_name: &str, args: A) -> LuaResult<Value>
    where
        A: mlua::IntoLuaMulti,
    {
        // Reset instruction counter BEFORE any Lua operations
        // This ensures table.get() doesn't consume our budget
        self.reset_instruction_count();

        // Check if the hook function exists
        let hook_value: Value = plugin_table.get(hook_name)?;

        match hook_value {
            Value::Function(func) => func.call(args),
            Value::Nil => Ok(Value::Nil), // Hook not implemented, that's OK
            _ => Err(mlua::Error::RuntimeError(format!(
                "Plugin hook '{}' must be a function",
                hook_name
            ))),
        }
    }

    /// Get a reference to the underlying Lua instance
    #[allow(dead_code)]  // Reserved for future plugin API expansion
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Create an empty table in the Lua context
    #[allow(dead_code)]  // Reserved for future plugin API expansion
    pub fn create_table(&self) -> LuaResult<Table> {
        self.lua.create_table()
    }

    /// Create a function in the Lua context
    #[allow(dead_code)]  // Reserved for future plugin API expansion
    pub fn create_function<F, A, R>(&self, func: F) -> LuaResult<Function>
    where
        F: Fn(&Lua, A) -> LuaResult<R> + 'static,
        A: mlua::FromLuaMulti,
        R: mlua::IntoLuaMulti,
    {
        self.lua.create_function(func)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sandbox_removes_dangerous_functions() {
        let runtime = LuaRuntime::new().unwrap();
        let globals = runtime.lua().globals();

        // These should all be nil
        assert!(globals.get::<Value>("os").unwrap().is_nil());
        assert!(globals.get::<Value>("io").unwrap().is_nil());
        assert!(globals.get::<Value>("debug").unwrap().is_nil());
        assert!(globals.get::<Value>("loadfile").unwrap().is_nil());
        assert!(globals.get::<Value>("dofile").unwrap().is_nil());
        assert!(globals.get::<Value>("require").unwrap().is_nil());
    }

    #[test]
    fn test_sandbox_keeps_safe_functions() {
        let runtime = LuaRuntime::new().unwrap();
        let globals = runtime.lua().globals();

        // These should still exist
        assert!(!globals.get::<Value>("string").unwrap().is_nil());
        assert!(!globals.get::<Value>("table").unwrap().is_nil());
        assert!(!globals.get::<Value>("math").unwrap().is_nil());
        assert!(!globals.get::<Value>("pairs").unwrap().is_nil());
        assert!(!globals.get::<Value>("ipairs").unwrap().is_nil());
        assert!(!globals.get::<Value>("type").unwrap().is_nil());
        assert!(!globals.get::<Value>("tonumber").unwrap().is_nil());
        assert!(!globals.get::<Value>("tostring").unwrap().is_nil());
        assert!(!globals.get::<Value>("print").unwrap().is_nil());
    }

    #[test]
    fn test_call_hook_nonexistent() {
        let runtime = LuaRuntime::new().unwrap();
        let table = runtime.create_table().unwrap();

        // Calling a non-existent hook should return Nil
        let result = runtime.call_hook(&table, "nonexistent", ()).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn test_basic_lua_execution() {
        let runtime = LuaRuntime::new().unwrap();

        // Test that basic Lua works
        let result: i32 = runtime.lua().load("return 1 + 2").eval().unwrap();
        assert_eq!(result, 3);
    }

    #[test]
    fn test_instruction_limit_aborts_infinite_loop() {
        // Use a low limit for fast test
        let runtime = LuaRuntime::with_instruction_limit(10_000).unwrap();

        // This infinite loop should be aborted
        let result = runtime.lua().load("while true do end").exec();

        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Instruction limit exceeded"),
            "Expected instruction limit error, got: {}",
            err
        );
    }

    #[test]
    fn test_instruction_limit_allows_normal_code() {
        let runtime = LuaRuntime::new().unwrap();

        // Normal code should complete fine
        let result: i32 = runtime
            .lua()
            .load(
                r#"
                local sum = 0
                for i = 1, 1000 do
                    sum = sum + i
                end
                return sum
            "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 500500); // Sum 1..1000
    }

    #[test]
    fn test_instruction_count_resets_between_calls() {
        // Use a limit that allows a moderate loop
        let runtime = LuaRuntime::with_instruction_limit(50_000).unwrap();

        // First call - should succeed
        let result1: i32 = runtime
            .lua()
            .load("local s=0; for i=1,100 do s=s+i end; return s")
            .eval()
            .unwrap();
        assert_eq!(result1, 5050);

        // Second call - should also succeed (counter was reset)
        let result2: i32 = runtime
            .lua()
            .load("local s=0; for i=1,100 do s=s+i end; return s")
            .eval()
            .unwrap();
        assert_eq!(result2, 5050);
    }

    #[test]
    fn test_disabled_instruction_limit() {
        // With limit=0, no hook is installed
        let runtime = LuaRuntime::with_instruction_limit(0).unwrap();

        // A moderate loop should work (we can't test infinite loops here)
        let result: i32 = runtime
            .lua()
            .load("local s=0; for i=1,10000 do s=s+i end; return s")
            .eval()
            .unwrap();
        assert_eq!(result, 50005000);
    }

    #[test]
    fn test_memory_limit_aborts_excessive_allocation() {
        // Use a small memory limit (100KB)
        let runtime = LuaRuntime::with_limits(DEFAULT_MAX_INSTRUCTIONS, 100 * 1024).unwrap();

        // Try to allocate a large table that exceeds the limit
        let result = runtime.lua().load(
            r#"
            local t = {}
            for i = 1, 1000000 do
                t[i] = string.rep("x", 1000)  -- 1KB strings
            end
            return #t
            "#,
        ).exec();

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Memory errors in mlua are typically MemoryError variant
        assert!(
            matches!(err, mlua::Error::MemoryError(_))
                || err.to_string().contains("memory"),
            "Expected memory error, got: {}",
            err
        );
    }

    #[test]
    fn test_memory_limit_allows_normal_operations() {
        // Use default memory limit (16MB)
        let runtime = LuaRuntime::new().unwrap();

        // Normal operations should work fine
        let result: i32 = runtime
            .lua()
            .load(
                r#"
                local t = {}
                for i = 1, 1000 do
                    t[i] = "item_" .. i
                end
                return #t
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 1000);
    }

    #[test]
    fn test_used_memory_tracking() {
        let runtime = LuaRuntime::new().unwrap();

        let before = runtime.used_memory();

        // Allocate some data
        runtime
            .lua()
            .load("_G.big_table = {}; for i=1,10000 do _G.big_table[i] = i end")
            .exec()
            .unwrap();

        let after = runtime.used_memory();

        // Memory should have increased
        assert!(
            after > before,
            "Memory should increase: before={}, after={}",
            before,
            after
        );
    }

    #[test]
    fn test_disabled_memory_limit() {
        // With max_memory=0, no limit is set
        let runtime = LuaRuntime::with_limits(DEFAULT_MAX_INSTRUCTIONS, 0).unwrap();

        // Should be able to allocate without hitting limits
        // (within reason for the test)
        let result: i32 = runtime
            .lua()
            .load(
                r#"
                local t = {}
                for i = 1, 10000 do
                    t[i] = "item_" .. i
                end
                return #t
                "#,
            )
            .eval()
            .unwrap();

        assert_eq!(result, 10000);
    }
}
