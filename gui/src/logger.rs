// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::sync::atomic::{AtomicU64, Ordering};

use log::{Level, LevelFilter, Log, Metadata, Record};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub(crate) struct Message {
    pub(crate) id: u64,
    pub(crate) msg: String,
}

pub(crate) struct UiLogger {
    sender: UnboundedSender<Message>,
    counter: AtomicU64,
}

impl Log for UiLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Info
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

        let level = match record.level() {
            Level::Error => "ERROR",
            Level::Warn => "WARN",
            Level::Info => "INFO",
            Level::Debug => "DEBUG",
            Level::Trace => "TRACE",
        };

        let msg = format!("[{level}] {}", record.args());

        let _ = self.sender.send(Message {
            id: self.counter.fetch_add(1, Ordering::Relaxed),
            msg: msg.clone(),
        });

        #[cfg(target_family = "wasm")]
        {
            match record.level() {
                Level::Error => web_sys::console::error_1(&msg.into()),
                Level::Warn => web_sys::console::warn_1(&msg.into()),
                Level::Info => web_sys::console::info_1(&msg.into()),
                Level::Debug | Level::Trace => web_sys::console::trace_1(&msg.into()),
            }
        }
        #[cfg(not(target_family = "wasm"))]
        {
            println!("{}", msg);
        }
    }

    fn flush(&self) {}
}

pub(crate) fn init_ui_logger() -> Result<UnboundedReceiver<Message>, log::SetLoggerError> {
    let (log_tx, log_rx) = tokio::sync::mpsc::unbounded_channel();

    let logger = UiLogger {
        sender: log_tx,
        counter: AtomicU64::new(0),
    };
    log::set_boxed_logger(Box::new(logger))?;
    log::set_max_level(LevelFilter::Info);
    Ok(log_rx)
}
