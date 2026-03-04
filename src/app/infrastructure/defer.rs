//! Deferred message sending via FLTK event loop.

use crate::app::domain::messages::Message;

/// Post a message to be dispatched on the next event loop iteration (or after `delay` seconds).
/// Useful for deferring heavy work so the UI can update first.
pub fn defer_send(sender: fltk::app::Sender<Message>, delay: f64, msg: Message) {
    let mut msg = Some(msg);
    fltk::app::add_timeout3(delay, move |_| {
        if let Some(m) = msg.take() {
            sender.send(m);
        }
    });
}
