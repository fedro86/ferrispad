//! Lua runtime wrapper with sandboxing.
//!
//! Provides a safe Lua execution environment that:
//! - Disables dangerous functions (os, io, debug, loadfile, dofile, require)
//! - Allows safe functions (string, table, math, pairs, ipairs, etc.)
//! - Loads plugin scripts and calls hook functions

use mlua::{Function, Lua, Result as LuaResult, Table, Value};
use std::path::Path;

/// Lua runtime wrapper with sandboxing
pub struct LuaRuntime {
    lua: Lua,
}

impl LuaRuntime {
    /// Create a new sandboxed Lua runtime
    pub fn new() -> LuaResult<Self> {
        let lua = Lua::new();
        let runtime = Self { lua };
        runtime.setup_sandbox()?;
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

    /// Load a plugin script from init.lua and return the plugin table
    pub fn load_script(&self, path: &Path) -> LuaResult<Table> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            mlua::Error::RuntimeError(format!("Failed to read {}: {}", path.display(), e))
        })?;

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

    /// Call a hook function on a plugin table
    /// Returns the result value from the hook, or None if hook doesn't exist
    pub fn call_hook<A>(&self, plugin_table: &Table, hook_name: &str, args: A) -> LuaResult<Value>
    where
        A: mlua::IntoLuaMulti,
    {
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
}
