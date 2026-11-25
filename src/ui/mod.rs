pub mod color;
pub mod contrast;
pub mod diagnostic_panel;
pub mod dialogs;
pub mod divider;
pub mod editor_container;
pub mod file_dialogs;
pub mod layout_shortcut;
pub mod main_window;
pub mod menu;
pub mod scrollbar;
pub mod split_panel;
pub mod start_page;
pub mod status_bar;
pub mod tab_bar;
pub mod terminal_panel;
pub mod theme;
pub mod toast;
pub mod tree_panel;

/// Serializes the unit tests that construct FLTK widgets: FLTK's "current
/// group" is process-global, so widgets built on parallel test threads could
/// land in each other's groups.
#[cfg(test)]
pub(crate) fn fltk_widget_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
    LOCK.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Run `f` on the one thread that unit tests may build FLTK windows on, and
/// return its result (a panic in `f` is re-raised here). fltk-rs pins its
/// "UI thread" to the first thread that creates a window and asserts it for
/// every later one, while `cargo test` runs each test on its own thread, so
/// every window-building test must go through this.
#[cfg(test)]
pub(crate) fn on_fltk_test_thread<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    use std::sync::mpsc::{Sender, channel};
    use std::sync::{Mutex, OnceLock};
    type Job = Box<dyn FnOnce() + Send>;
    static WORKER: OnceLock<Mutex<Sender<Job>>> = OnceLock::new();

    let worker = WORKER.get_or_init(|| {
        let (jobs, inbox) = channel::<Job>();
        std::thread::Builder::new()
            .name("fltk-test-ui".into())
            .spawn(move || inbox.into_iter().for_each(|job| job()))
            .expect("spawn the FLTK test thread");
        Mutex::new(jobs)
    });
    let (reply, result) = channel();
    let job: Job = Box::new(move || {
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let _ = reply.send(outcome);
    });
    worker
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .send(job)
        .expect("the FLTK test thread is running");
    match result.recv().expect("the FLTK test thread replies") {
        Ok(value) => value,
        Err(panic) => std::panic::resume_unwind(panic),
    }
}
