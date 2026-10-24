// SPDX-License-Identifier: Apache-2.0
//! The run's message lines, held in the order the reference logs them until the caller writes
//! them through the suite logger (tool `MPL`).

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    Info(u16, String),
    Warn(u16, String),
    /// `logger_->report`: text with no prefix.
    Report(String),
}

/// Write a message through the process's one logger (as the reference has one per session):
/// each line on stderr as it is logged, and an event under its code.
pub fn emit(m: &Message) {
    use std::sync::{Mutex, OnceLock};
    use vyges_events::logger::Logger;
    static LOG: OnceLock<Mutex<Logger>> = OnceLock::new();
    let mut log = LOG.get_or_init(|| Mutex::new(Logger::new().events("vyges-mpl"))).lock().unwrap_or_else(|e| e.into_inner());
    match m {
        Message::Info(id, t) => log.info("MPL", *id, t),
        Message::Warn(id, t) => log.warn("MPL", *id, t),
        Message::Report(t) => log.report(t),
    }
    for l in log.take_lines() {
        eprintln!("{l}");
    }
}
