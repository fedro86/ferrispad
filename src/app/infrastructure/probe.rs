//! Run a short external probe (`gdbus`, `gsettings`, …) with a hard deadline
//! (T0088). A probe sits on the UI thread at startup and on Settings → Save,
//! so it must never be able to hang: `Command::output()` waits forever on a
//! child that never exits (e.g. `gdbus` stuck connecting to a wedged bus,
//! where its own `--timeout` does not apply yet).

use std::io::Read;
use std::process::{Child, Command, Output, Stdio};
use std::time::{Duration, Instant};

/// How often the child is polled while waiting. The loop only runs while a
/// probe is in flight (startup / Save), never at idle.
const POLL: Duration = Duration::from_millis(10);

/// At most this much of stdout / stderr is kept. Probes answer a few bytes.
const MAX_OUTPUT: u64 = 4096;

/// Run `cmd` with stdin null and stdout/stderr piped; `None` if it cannot be
/// spawned or is still running at `deadline`, in which case it is killed and
/// reaped (no zombie is left).
///
/// stdout/stderr are read only after the child has exited. That assumes the
/// probe writes far less than a pipe buffer (64 KiB on Linux); a child that
/// filled the pipe would block in `write`, never exit, and be killed at the
/// deadline, so the call stays bounded either way. It also assumes the probe
/// does not leave a grandchild holding the pipe open (`gdbus call` and
/// `gsettings get` do not fork).
pub fn run_bounded(cmd: &mut Command, deadline: Instant) -> Option<Output> {
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(POLL),
            _ => {
                kill_and_reap(&mut child);
                return None;
            }
        }
    };
    Some(Output {
        status,
        stdout: read_capped(child.stdout.take()),
        stderr: read_capped(child.stderr.take()),
    })
}

fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn read_capped(pipe: Option<impl Read>) -> Vec<u8> {
    let mut buf = Vec::new();
    if let Some(pipe) = pipe {
        let _ = pipe.take(MAX_OUTPUT).read_to_end(&mut buf);
    }
    buf
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    #[test]
    fn probe_returns_the_output_of_a_quick_command() {
        let deadline = Instant::now() + Duration::from_secs(5);
        let out = run_bounded(
            Command::new("sh").args(["-c", "echo hi; echo err >&2"]),
            deadline,
        )
        .expect("ran");
        assert!(out.status.success());
        assert_eq!(out.stdout, b"hi\n");
        assert_eq!(out.stderr, b"err\n");
    }

    #[test]
    fn probe_missing_command_is_none() {
        let deadline = Instant::now() + Duration::from_secs(5);
        assert!(run_bounded(&mut Command::new("ferrispad-no-such-probe"), deadline).is_none());
    }

    /// A probe that outlives the deadline is abandoned on time and reaped.
    #[test]
    fn probe_past_the_deadline_is_killed_and_reaped() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let script = format!("echo $$ > '{}'; exec sleep 5", pid_file.display());
        let start = Instant::now();
        let out = run_bounded(
            Command::new("sh").args(["-c", &script]),
            start + Duration::from_millis(1500),
        );
        let elapsed = start.elapsed();
        assert!(out.is_none(), "a hung probe yields no answer");
        assert!(elapsed < Duration::from_secs(3), "took {elapsed:?}");
        assert!(elapsed >= Duration::from_millis(1500), "took {elapsed:?}");
        // Reaped: once waited for, the pid has no /proc entry (a zombie would).
        #[cfg(target_os = "linux")]
        {
            let pid = std::fs::read_to_string(&pid_file).unwrap();
            let proc_dir = std::path::Path::new("/proc").join(pid.trim());
            assert!(!proc_dir.exists(), "child {} left behind", pid.trim());
        }
    }

    /// A probe flooding stdout cannot exceed the deadline either.
    #[test]
    fn probe_that_floods_its_pipe_is_still_bounded() {
        let start = Instant::now();
        let out = run_bounded(
            Command::new("sh").args(["-c", "exec yes"]),
            start + Duration::from_millis(300),
        );
        assert!(out.is_none());
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}
