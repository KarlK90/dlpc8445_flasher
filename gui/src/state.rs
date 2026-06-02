use std::collections::VecDeque;

// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::signals::{ReadableExt, Signal};
use tokio::sync::mpsc::UnboundedSender;

use crate::logger::Message;
use dlpc8445_proto::runner::{ActionProgress, DeviceState, RunnerCommand, RunnerState};

#[derive(Clone, Copy)]
pub(crate) struct Dlpc8445GuiState {
    pub(crate) command_tx: Signal<UnboundedSender<RunnerCommand>>,
    pub(crate) device_state: Signal<DeviceState>,
    pub(crate) runner_state: Signal<RunnerState>,
    pub(crate) action_progress: Signal<ActionProgress>,
    pub(crate) logs: Signal<VecDeque<Message>>,
}

impl Dlpc8445GuiState {
    pub(crate) async fn send_command(&self, command: RunnerCommand) {
        self.command_tx
            .read()
            .send(command)
            .expect("failed to send command");
    }
}
