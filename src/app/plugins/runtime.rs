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
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Default maximum instructions per hook call (1 million).
/// This prevents infinite loops while allowing complex operations.
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 1_000_000;

/// Default maximum memory per plugin (16 MB).
/// This prevents memory exhaustion while allowing reasonable data structures.
pub const DEFAULT_MAX_MEMORY: usize = 16 * 1024 * 1024;

/// Default wall-clock deadline for a single hook call (30 s).
///
/// A backstop so a synchronous hook cannot freeze the UI thread indefinitely —
/// a runaway loop that outlives its instruction budget's wall-clock, or a
/// sequence of slow blocking API calls. It is armed only while a hook runs and
/// checked between Lua instructions (see [`LuaRuntime::setup_instruction_limit`]),
/// so idle CPU stays 0% — no background timer. A *single* blocking call is
/// bounded by that call's own timeout (e.g. `DEFAULT_COMMAND_TIMEOUT`); this
/// deadline is chosen comfortably above one command timeout so a legitimate
/// single-command linter is never cut off mid-run.
pub const DEFAULT_HOOK_DEADLINE: Duration = Duration::from_secs(30);

/// Hook interval - check instruction count every N instructions.
/// Lower values = more responsive abort, higher overhead.
/// 1000 is a reasonable balance.
const HOOK_CHECK_INTERVAL: u32 = 1000;

/// The error re-raised by the guarded `pcall`/`xpcall` once the instruction
/// limit has been latched, so the abort propagates instead of being swallowed.
fn instruction_limit_aborted() -> mlua::Error {
    mlua::Error::RuntimeError("Instruction limit exceeded (aborting)".to_string())
}

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
    /// Latched once the instruction limit **or** the wall-clock deadline is
    /// exceeded during an execution. The guarded `pcall`/`xpcall` re-raise while
    /// this is set, so the abort cannot be swallowed by a `pcall` loop. Reset
    /// before each hook call.
    poisoned: Arc<AtomicBool>,
    /// Maximum memory allowed (0 = unlimited)
    max_memory: usize,
    /// Monotonic origin captured at construction. The deadline is stored as
    /// nanoseconds since this instant so it fits in an atomic — no lock in the
    /// per-instruction hook.
    origin: Instant,
    /// Wall-clock deadline for the *current* hook call, as nanoseconds since
    /// `origin`. `0` = disarmed (no hook running). Armed by [`Self::arm_deadline`].
    deadline_ns: Arc<AtomicU64>,
    /// Per-hook wall-clock budget (`Duration::ZERO` = deadline disabled).
    hook_deadline: Duration,
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
        Self::with_limits_and_deadline(max_instructions, max_memory, DEFAULT_HOOK_DEADLINE)
    }

    /// As [`Self::with_limits`], with an explicit per-hook wall-clock deadline.
    ///
    /// Production uses [`DEFAULT_HOOK_DEADLINE`] via [`Self::with_limits`]; this
    /// entry point exists mainly so tests can use a short deadline. A deadline
    /// of `Duration::ZERO` disables the wall-clock bound. The deadline is only
    /// enforced when the instruction hook is installed (`max_instructions != 0`),
    /// which is always the case in production.
    pub fn with_limits_and_deadline(
        max_instructions: u64,
        max_memory: usize,
        hook_deadline: Duration,
    ) -> LuaResult<Self> {
        let lua = Lua::new();
        let instruction_count = Arc::new(AtomicU64::new(0));
        let poisoned = Arc::new(AtomicBool::new(false));
        let deadline_ns = Arc::new(AtomicU64::new(0));

        let runtime = Self {
            lua,
            max_instructions,
            instruction_count,
            poisoned,
            max_memory,
            origin: Instant::now(),
            deadline_ns,
            hook_deadline,
        };
        runtime.setup_sandbox()?;
        runtime.setup_pcall_guard()?;
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
        // Remove coroutines: the instruction-count hook is installed on the main
        // Lua thread only, so code running inside a coroutine executes with no
        // limit at all. `coroutine.wrap(function() while true do end end)()`
        // would freeze the UI forever (T0005). Removing the library closes that
        // hole; plugin hooks are synchronous and do not need coroutines.
        globals.set("coroutine", Value::Nil)?;

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
        let poison = Arc::clone(&self.poisoned);
        let deadline_ns = Arc::clone(&self.deadline_ns);
        let origin = self.origin;

        self.lua.set_hook(
            HookTriggers::new().every_nth_instruction(HOOK_CHECK_INTERVAL),
            move |_lua, _debug| {
                // Wall-clock deadline (armed per hook call). Checked between
                // instructions, so it bounds a runaway loop or a sequence of
                // slow blocking API calls without a background timer (T0011). A
                // single blocking call is bounded by that call's own timeout.
                let deadline = deadline_ns.load(Ordering::Relaxed);
                if deadline != 0 && origin.elapsed().as_nanos() as u64 >= deadline {
                    // Latch like the instruction limit so a `pcall` loop cannot
                    // swallow the abort and keep the UI frozen (T0005).
                    poison.store(true, Ordering::Relaxed);
                    return Err(mlua::Error::RuntimeError(
                        "Hook execution timed out (aborting)".to_string(),
                    ));
                }

                let count = counter.fetch_add(HOOK_CHECK_INTERVAL as u64, Ordering::Relaxed);
                if count >= max {
                    // Latch so the guarded pcall/xpcall re-raise on the way up:
                    // the error below is catchable, but a `pcall` loop can no
                    // longer swallow it and keep spinning (T0005).
                    poison.store(true, Ordering::Relaxed);
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

    /// Wrap `pcall`/`xpcall` so the instruction-limit abort cannot be swallowed.
    ///
    /// Each guarded call delegates to the original, but re-raises (instead of
    /// returning a caught error) if the limit was tripped — either before the
    /// call, or during it. Because every guarded `pcall` on the stack re-raises
    /// on the way up, `while true do pcall(function() while true do end end) end`
    /// can no longer defeat the instruction limit. While not poisoned, error
    /// handling behaves exactly like stock `pcall`/`xpcall`.
    fn setup_pcall_guard(&self) -> LuaResult<()> {
        if self.max_instructions == 0 {
            // No instruction limit → nothing to protect.
            return Ok(());
        }

        let globals = self.lua.globals();
        for name in ["pcall", "xpcall"] {
            let original: Function = globals.get(name)?;
            let poison = Arc::clone(&self.poisoned);
            let guarded = self
                .lua
                .create_function(move |_lua, args: mlua::MultiValue| {
                    if poison.load(Ordering::Relaxed) {
                        return Err(instruction_limit_aborted());
                    }
                    let result = original.call::<mlua::MultiValue>(args);
                    if poison.load(Ordering::Relaxed) {
                        // Limit tripped inside the protected call — don't let the
                        // caught error be discarded.
                        return Err(instruction_limit_aborted());
                    }
                    result
                })?;
            globals.set(name, guarded)?;
        }

        Ok(())
    }

    /// Reset instruction counter (and clear the poison latch) before a new
    /// operation. Called before each hook invocation so a plugin that used its
    /// full budget in one hook starts the next one fresh.
    fn reset_instruction_count(&self) {
        self.instruction_count.store(0, Ordering::Relaxed);
        self.poisoned.store(false, Ordering::Relaxed);
    }

    /// Arm the wall-clock deadline for one hook call. No-op when disabled
    /// (`hook_deadline == 0`). The deadline is stored as an absolute nanosecond
    /// offset from `origin` so the per-instruction hook can compare with a single
    /// atomic load.
    fn arm_deadline(&self) {
        if self.hook_deadline.is_zero() {
            return;
        }
        let now = self.origin.elapsed().as_nanos() as u64;
        let budget = self.hook_deadline.as_nanos() as u64;
        self.deadline_ns
            .store(now.saturating_add(budget), Ordering::Relaxed);
    }

    /// Disarm the deadline once the hook returns, so nothing is counted against a
    /// plugin while no hook is running (idle CPU stays 0%) and a stale deadline
    /// can never abort the next, unrelated execution.
    fn disarm_deadline(&self) {
        self.deadline_ns.store(0, Ordering::Relaxed);
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

    /// Standard-library tables each plugin gets its **own** shallow copy of, so
    /// one plugin monkey-patching e.g. `string.rep` cannot affect another
    /// (T0009). Their function values are shared (functions are immutable), only
    /// the container table is per-plugin. Libraries removed by the sandbox
    /// (`os`, `io`, `debug`, `coroutine`, `package`) are simply absent.
    const PER_PLUGIN_LIBS: [&'static str; 4] = ["string", "table", "math", "utf8"];

    /// Build a fresh per-plugin environment table used as the chunk's `_ENV`.
    ///
    /// Base functions (`print`, `pcall`, `type`, …) are shared references — they
    /// are immutable, and sharing the *guarded* `pcall`/`xpcall` keeps the
    /// instruction-limit protection (T0005) intact. The mutable standard-library
    /// tables ([`Self::PER_PLUGIN_LIBS`]) are shallow-copied so patches stay
    /// plugin-local. `_G` points at the env itself, so a plugin writing a global
    /// mutates only its own environment and cannot see another plugin's globals
    /// (T0009).
    ///
    /// Note: this isolates plugin globals and library-table edits, not the
    /// process-wide string *value* metatable reachable via `getmetatable("")`;
    /// full state separation would be needed for that (see the module docs and
    /// the T0006 advisory lint). Memory is still a single shared Lua-state pool
    /// (see [`Self::with_limits`]).
    fn build_plugin_env(&self) -> LuaResult<Table> {
        let globals = self.lua.globals();
        let env = self.lua.create_table()?;

        for pair in globals.pairs::<Value, Value>() {
            let (key, value) = pair?;

            // Give the plugin its own copy of each mutable stdlib table.
            if let (Value::String(name), Value::Table(lib)) = (&key, &value)
                && name
                    .to_str()
                    .map(|n| Self::PER_PLUGIN_LIBS.contains(&&*n))
                    .unwrap_or(false)
            {
                env.set(key.clone(), shallow_copy_table(&self.lua, lib)?)?;
                continue;
            }

            env.set(key, value)?;
        }

        // `_G` must resolve to the plugin's own environment, not the shared one.
        env.set("_G", &env)?;

        Ok(env)
    }

    /// Load a plugin script from init.lua and return the plugin table.
    ///
    /// Instruction count is reset before loading to give each plugin
    /// a fresh budget for initialization. The chunk runs with a per-plugin
    /// `_ENV` (see [`Self::build_plugin_env`]) so plugins are isolated from each
    /// other's globals and library patches (T0009).
    pub fn load_script(&self, path: &Path) -> LuaResult<Table> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to read {}: {}", path.display(), e))
        })?;

        // Reset instruction counter for this load operation
        self.reset_instruction_count();

        // Execute the script in its own environment and expect it to return a
        // table. The returned functions capture this `_ENV`, so their later hook
        // calls stay isolated too. Time-box the load so a plugin that hangs in
        // its top-level chunk cannot freeze startup (T0011).
        let env = self.build_plugin_env()?;
        self.arm_deadline();
        let load_result = self.lua.load(&content).set_environment(env).eval::<Value>();
        self.disarm_deadline();
        let result = load_result?;

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
            Value::Function(func) => {
                // Arm the wall-clock deadline only around the actual hook body so
                // a slow/hung hook can't freeze the UI thread indefinitely
                // (T0011); disarm on every path, including the error path.
                self.arm_deadline();
                let result = func.call(args);
                self.disarm_deadline();
                result
            }
            Value::Nil => Ok(Value::Nil), // Hook not implemented, that's OK
            _ => Err(mlua::Error::RuntimeError(format!(
                "Plugin hook '{}' must be a function",
                hook_name
            ))),
        }
    }

    /// Get a reference to the underlying Lua instance
    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    /// Create an empty table in the Lua context
    #[allow(dead_code)] // Used in tests; production code calls lua().create_table() directly
    pub fn create_table(&self) -> LuaResult<Table> {
        self.lua.create_table()
    }

    /// Create a function in the Lua context
    #[allow(dead_code)] // Reserved for future plugin API expansion
    pub fn create_function<F, A, R>(&self, func: F) -> LuaResult<Function>
    where
        F: Fn(&Lua, A) -> LuaResult<R> + 'static,
        A: mlua::FromLuaMulti,
        R: mlua::IntoLuaMulti,
    {
        self.lua.create_function(func)
    }
}

/// Shallow-copy a Lua table: a new table with the same key/value pairs (values
/// shared by reference). Used to give each plugin its own copy of a stdlib table
/// so patches don't leak across plugins (T0009). The standard library tables
/// have no metatable, so none is copied.
fn shallow_copy_table(lua: &Lua, src: &Table) -> LuaResult<Table> {
    let dst = lua.create_table()?;
    for pair in src.pairs::<Value, Value>() {
        let (k, v) = pair?;
        dst.set(k, v)?;
    }
    Ok(dst)
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
        let result = runtime
            .lua()
            .load(
                r#"
            local t = {}
            for i = 1, 1000000 do
                t[i] = string.rep("x", 1000)  -- 1KB strings
            end
            return #t
            "#,
            )
            .exec();

        assert!(result.is_err());
        let err = result.unwrap_err();
        // Memory errors in mlua are typically MemoryError variant
        assert!(
            matches!(err, mlua::Error::MemoryError(_)) || err.to_string().contains("memory"),
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

    // Regression (T0005, audit S4): `coroutine` must be gone from the sandbox.
    // The instruction hook is main-thread only, so a busy-loop inside a
    // coroutine would run unlimited; removing the library closes bypass #1.
    #[test]
    fn coroutine_is_removed_from_sandbox() {
        let runtime = LuaRuntime::new().unwrap();
        assert!(
            runtime
                .lua()
                .globals()
                .get::<Value>("coroutine")
                .unwrap()
                .is_nil(),
            "coroutine must be nil in the plugin sandbox"
        );
    }

    // With coroutine removed, a coroutine busy-loop errors immediately instead
    // of freezing (safe to run post-fix — it never enters the loop).
    #[test]
    fn coroutine_busyloop_is_blocked() {
        let runtime = LuaRuntime::with_instruction_limit(100_000).unwrap();
        let result = runtime
            .lua()
            .load("coroutine.wrap(function() while true do end end)()")
            .exec();
        assert!(result.is_err(), "coroutine access must be blocked");
    }

    // Regression (T0005, audit S4): a `pcall`-wrapped busy-loop must not be able
    // to swallow the instruction limit. Bounded outer loop so the test can never
    // hang: before the fix this returns "completed" (limit defeated); after, the
    // poisoned guard re-raises and `exec` returns an error.
    #[test]
    fn pcall_loop_cannot_defeat_instruction_limit() {
        let runtime = LuaRuntime::with_instruction_limit(50_000).unwrap();
        let result: mlua::Result<Value> = runtime
            .lua()
            .load(
                "for i=1,100000 do pcall(function() while true do end end) end return 'completed'",
            )
            .eval();
        assert!(
            result.is_err(),
            "pcall loop defeated the instruction limit: {:?}",
            result.map(|v| format!("{:?}", v))
        );
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Instruction limit"),
            "expected an instruction-limit abort"
        );
    }

    // The guard must not change ordinary pcall behaviour while under budget.
    #[test]
    fn pcall_still_catches_normal_errors() {
        let runtime = LuaRuntime::new().unwrap();
        let caught: bool = runtime
            .lua()
            .load("local ok = pcall(function() error('boom') end); return ok")
            .eval()
            .unwrap();
        assert!(
            !caught,
            "pcall should still catch a normal error (return false)"
        );

        let ok: bool = runtime
            .lua()
            .load("local ok = pcall(function() return 42 end); return ok")
            .eval()
            .unwrap();
        assert!(ok, "pcall should still report success as true");
    }

    // Regression (T0011, audit M4): a hook that would block the UI thread far
    // too long is aborted by the wall-clock deadline, even with instruction
    // budget to spare. The instruction limit is set absurdly high so the
    // *deadline* (not the instruction count) is provably what stops the loop.
    #[test]
    fn hook_execution_is_wall_clock_deadline_bounded() {
        let runtime = LuaRuntime::with_limits_and_deadline(
            1_000_000_000, // effectively unlimited instructions for this test
            DEFAULT_MAX_MEMORY,
            Duration::from_millis(150),
        )
        .unwrap();
        let table = runtime.create_table().unwrap();
        let busy: Function = runtime
            .lua()
            .load("return function() while true do end end")
            .eval()
            .unwrap();
        table.set("on_save", busy).unwrap();

        let start = Instant::now();
        let result = runtime.call_hook(&table, "on_save", ());
        let elapsed = start.elapsed();

        assert!(result.is_err(), "a timed-out hook must return an error");
        assert!(
            result
                .unwrap_err()
                .to_string()
                .to_lowercase()
                .contains("timed out"),
            "expected a wall-clock timeout abort, not an instruction-limit error"
        );
        assert!(
            elapsed < Duration::from_secs(2),
            "deadline did not fire promptly (took {elapsed:?})"
        );
    }

    // The deadline must not disturb a normal, quick hook: it completes and
    // returns its value as usual.
    #[test]
    fn deadline_allows_a_fast_hook_to_complete() {
        let runtime = LuaRuntime::new().unwrap();
        let table = runtime.create_table().unwrap();
        let f: Function = runtime
            .lua()
            .load("return function() local s=0; for i=1,1000 do s=s+i end; return s end")
            .eval()
            .unwrap();
        table.set("on_save", f).unwrap();
        let out: Value = runtime.call_hook(&table, "on_save", ()).unwrap();
        assert_eq!(out.as_i64(), Some(500500));
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
