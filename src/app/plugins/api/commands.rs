//! Command execution methods exposed to Lua plugins.

use std::process::Command;

use super::super::security::{validate_command_arg, DEFAULT_COMMAND_TIMEOUT};
use super::EditorApi;

/// Run an external command and return its output.
/// Returns: { stdout = "...", stderr = "...", success = true/false }
///
/// Security:
/// - Command must be in the plugin's approved commands list (from manifest)
/// - Arguments are validated to prevent shell injection
/// - Command runs with a timeout (30 seconds by default)
/// - Working directory is set to project root if available
pub fn run_command(
    lua: &mlua::Lua,
    this: &EditorApi,
    args: mlua::Variadic<String>,
) -> mlua::Result<mlua::Value> {
    use std::io::Read;
    use std::process::Stdio;
    use std::time::Instant;

    let args: Vec<String> = args.into_iter().collect();
    if args.is_empty() {
        return Err(mlua::Error::RuntimeError(
            "run_command requires at least one argument (the command)".to_string(),
        ));
    }

    let cmd = &args[0];
    let cmd_args = &args[1..];

    // Security: Check if command is in approved list
    // Compare against basename so "/path/to/venv/bin/ruff" matches "ruff"
    // If allowed_commands is empty, no commands are permitted (strict mode)
    let plugin_name = this.plugin_name.as_deref().unwrap_or("unknown");
    let cmd_basename = std::path::Path::new(cmd)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(cmd);

    if !this.allowed_commands.iter().any(|c| c == cmd_basename || c == cmd) {
        if this.allowed_commands.is_empty() {
            eprintln!(
                "[plugin:security] {} tried to run '{}' but has no approved commands. \
                Add [permissions] execute = [\"{}\"] to plugin.toml",
                plugin_name, cmd, cmd_basename
            );
            return Err(mlua::Error::RuntimeError(format!(
                "No permissions. Add to plugin.toml: [permissions] execute = [\"{}\"]",
                cmd_basename
            )));
        } else {
            eprintln!(
                "[plugin:security] {} tried to run '{}' which is not in approved list: {:?}",
                plugin_name, cmd, this.allowed_commands
            );
            return Err(mlua::Error::RuntimeError(format!(
                "Command '{}' not approved. Allowed: {:?}",
                cmd_basename, this.allowed_commands
            )));
        }
    }

    // Security: Validate command name (no shell injection in command itself)
    if let Err(reason) = validate_command_arg(cmd) {
        eprintln!("[plugin:security] run_command blocked command '{}': {}", cmd, reason);
        return Err(mlua::Error::RuntimeError(format!(
            "Invalid command: {}",
            reason
        )));
    }

    // Security: Validate all arguments for shell injection
    for (i, arg) in cmd_args.iter().enumerate() {
        if let Err(reason) = validate_command_arg(arg) {
            eprintln!(
                "[plugin:security] run_command blocked argument {}: '{}' - {}",
                i, arg, reason
            );
            return Err(mlua::Error::RuntimeError(format!(
                "Invalid argument {}: {}",
                i, reason
            )));
        }
    }

    // Build command with pipes and optional working directory
    let mut command = Command::new(cmd);
    command
        .args(cmd_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    // Set working directory to project root if available
    if let Some(ref project_root) = this.project_root {
        command.current_dir(project_root);
    }

    // Spawn process
    match command.spawn() {
        Ok(mut child) => {
            let start = Instant::now();
            let timeout = DEFAULT_COMMAND_TIMEOUT;

            // Take stdout/stderr handles BEFORE the poll loop.
            // Drain them in background threads so the child never blocks
            // on a full pipe buffer (classic deadlock: child blocks writing
            // to a full pipe, parent waits for child to exit before reading).
            let stdout_handle = child.stdout.take();
            let stderr_handle = child.stderr.take();

            let stdout_thread = std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(mut out) = stdout_handle {
                    let _ = out.read_to_string(&mut s);
                }
                s
            });

            let stderr_thread = std::thread::spawn(move || {
                let mut s = String::new();
                if let Some(mut err) = stderr_handle {
                    let _ = err.read_to_string(&mut s);
                }
                s
            });

            // Wait for process using a channel instead of polling.
            // The child is moved into a thread that blocks on wait();
            // we extract the PID first so we can kill on timeout.
            let pid = child.id();
            let (tx, rx) = std::sync::mpsc::channel();
            std::thread::spawn(move || {
                let _ = tx.send(child.wait());
            });

            let remaining = timeout.saturating_sub(start.elapsed());
            match rx.recv_timeout(remaining) {
                Ok(Ok(status)) => {
                    let stdout_str = stdout_thread.join().unwrap_or_default();
                    let stderr_str = stderr_thread.join().unwrap_or_default();

                    let result = lua.create_table()?;
                    result.set("stdout", stdout_str)?;
                    result.set("stderr", stderr_str)?;
                    result.set("success", status.success())?;
                    Ok(mlua::Value::Table(result))
                }
                Ok(Err(e)) => {
                    let result = lua.create_table()?;
                    result.set("stdout", "")?;
                    result.set("stderr", format!("Command wait failed: {}", e))?;
                    result.set("success", false)?;
                    Ok(mlua::Value::Table(result))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    // Kill the timed-out process by PID
                    #[cfg(unix)]
                    // SAFETY: pid is a valid process ID from Command::spawn().
                    // SIGKILL (9) is always valid. kill() on a non-existent
                    // pid returns -1 (harmless).
                    unsafe {
                        unsafe extern "C" {
                            fn kill(pid: i32, sig: i32) -> i32;
                        }
                        kill(pid as i32, 9); // SIGKILL
                    }
                    #[cfg(windows)]
                    {
                        // On Windows, use taskkill as a fallback
                        let _ = Command::new("taskkill")
                            .args(["/F", "/PID", &pid.to_string()])
                            .output();
                    }
                    // Wait for the thread to finish (it will see the killed status)
                    let _ = rx.recv();
                    eprintln!(
                        "[plugin:security] run_command killed '{}' (pid {}) after {:?} timeout",
                        cmd, pid, timeout
                    );
                    let result = lua.create_table()?;
                    result.set("stdout", "")?;
                    result.set("stderr", format!(
                        "Command timed out after {} seconds",
                        timeout.as_secs()
                    ))?;
                    result.set("success", false)?;
                    Ok(mlua::Value::Table(result))
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    let result = lua.create_table()?;
                    result.set("stdout", "")?;
                    result.set("stderr", "Command wait thread disconnected".to_string())?;
                    result.set("success", false)?;
                    Ok(mlua::Value::Table(result))
                }
            }
        }
        Err(e) => {
            // Command not found or failed to execute
            let result = lua.create_table()?;
            result.set("stdout", "")?;
            result.set("stderr", format!("Command failed: {}", e))?;
            result.set("success", false)?;
            Ok(mlua::Value::Table(result))
        }
    }
}

/// Check if a command exists in PATH.
/// Returns true if the command is found, false otherwise.
pub fn command_exists(_: &mlua::Lua, _this: &EditorApi, cmd: String) -> mlua::Result<bool> {
    // Security: validate command name for consistency with run_command
    if let Err(reason) = validate_command_arg(&cmd) {
        eprintln!(
            "[plugin:security] command_exists blocked '{}': {}",
            cmd, reason
        );
        return Ok(false);
    }

    // Use `which` on Unix or `where` on Windows
    #[cfg(unix)]
    let check = Command::new("which").arg(&cmd).output();
    #[cfg(windows)]
    let check = Command::new("where").arg(&cmd).output();

    match check {
        Ok(output) => Ok(output.status.success()),
        Err(_) => Ok(false),
    }
}
