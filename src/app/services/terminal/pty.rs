//! PTY session management.
//!
//! Wraps `portable_pty` to spawn child processes in a pseudo-terminal.

use portable_pty::{CommandBuilder, NativePtySystem, PtySize, PtySystem};
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

/// A PTY session wrapping a child process
pub struct PtySession {
    /// Writer to send input to the child
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    /// The child process handle
    child: Arc<Mutex<Box<dyn portable_pty::Child + Send + Sync>>>,
    /// Master PTY (kept alive for resize; reader/writer already cloned from it)
    master: Box<dyn portable_pty::MasterPty + Send>,
}

impl PtySession {
    /// Spawn a new PTY session with the given command.
    /// Returns the session and a reader for the PTY output.
    pub fn spawn(
        command: Option<&str>,
        args: &[String],
        cwd: Option<&str>,
        cols: u16,
        rows: u16,
    ) -> Result<(Self, Box<dyn Read + Send>), String> {
        let pty_system = NativePtySystem::default();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| format!("Failed to open PTY: {}", e))?;

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_string());
        let cmd_str = command.unwrap_or(&shell);

        let mut cmd = CommandBuilder::new(cmd_str);
        for arg in args {
            cmd.arg(arg);
        }
        if let Some(dir) = cwd {
            cmd.cwd(dir);
        }

        // Set TERM for proper terminal emulation
        cmd.env("TERM", "xterm-256color");

        let child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| format!("Failed to spawn '{}': {}", cmd_str, e))?;

        // Clone reader before taking writer — both come from the master fd
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| format!("Failed to clone reader: {}", e))?;

        let writer = pair
            .master
            .take_writer()
            .map_err(|e| format!("Failed to take writer: {}", e))?;

        // pair.master stays valid for resize even after take_writer
        Ok((
            Self {
                writer: Arc::new(Mutex::new(writer)),
                child: Arc::new(Mutex::new(child)),
                master: pair.master,
            },
            reader,
        ))
    }

    /// Write data to the PTY (keyboard input)
    pub fn write(&self, data: &[u8]) {
        if let Ok(mut w) = self.writer.lock() {
            let _ = w.write_all(data);
            let _ = w.flush();
        }
    }

    /// Resize the PTY
    pub fn resize(&self, cols: u16, rows: u16) {
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
    }

    /// Check if the child process has exited
    pub fn try_wait(&self) -> Option<u32> {
        if let Ok(mut child) = self.child.lock() {
            child
                .try_wait()
                .ok()
                .flatten()
                .map(|s| s.exit_code())
        } else {
            None
        }
    }

    /// Kill the child process
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
    }
}

impl Drop for PtySession {
    fn drop(&mut self) {
        self.kill();
    }
}
