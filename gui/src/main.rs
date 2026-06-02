// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use std::{collections::VecDeque, sync::Arc};

use dioxus::prelude::*;
use dlpc8445_proto::{
    runner::{ActionProgress, DeviceState, Runner, RunnerEvent, RunnerState},
    webusb::WebUsbConnection,
};
use log;
use tokio::sync::{
    Mutex,
    mpsc::{self},
};

use crate::{
    components::{
        connection_status_card::ConnectionStatusCard, firmware_image_card::FirmwareImageCard,
        flash_controls_card::FlashControlCard, header::Header,
        help_information_card::HelpInformationCard, log_card::LogCard,
        reconnect_dialog::ReconnectDialog, webusb_dialog::WebUsbSupportDialog,
    },
    logger::Message,
    state::Dlpc8445GuiState,
};

mod components;
mod logger;
mod state;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const COMPONENTS_CSS: Asset = asset!("/assets/dx-components-theme.css");

pub(crate) const LOG_BUFFER_SIZE: usize = 100;

#[derive(Clone)]
struct AppContext {
    log_rx: Arc<Mutex<mpsc::UnboundedReceiver<Message>>>,
}

fn main() {
    let log_rx = Arc::new(Mutex::new(
        logger::init_ui_logger().expect("failed to initialize UI logger"),
    ));

    dioxus::LaunchBuilder::new()
        .with_context(AppContext { log_rx: log_rx })
        .launch(App);
}

#[component]
fn App() -> Element {
    let (event_tx, mut event_rx) = mpsc::unbounded_channel();
    let (command_tx, command_rx) = mpsc::unbounded_channel();

    use_context_provider(|| Dlpc8445GuiState {
        command_tx: Signal::new(command_tx),
        device_state: Signal::new(DeviceState::Disconnected),
        runner_state: Signal::new(RunnerState::Idle),
        action_progress: Signal::new(ActionProgress {
            current: 0,
            total: 0,
        }),
        logs: Signal::new(VecDeque::with_capacity(LOG_BUFFER_SIZE)),
    });

    spawn(async move {
        let mut runner = Runner::<WebUsbConnection>::new(event_tx, command_rx);
        loop {
            match runner.run().await {
                Ok(_) => info!("Runner finished successfully"),
                Err(err) => error!("Runner encountered an error: {err}"),
            }
        }
    });

    spawn(async move {
        let mut state = use_context::<Dlpc8445GuiState>();

        while let Some(event) = event_rx.recv().await {
            log::debug!("Received event: {:?}", event);
            match event {
                RunnerEvent::DeviceStateUpdate(status) => state.device_state.set(status),
                RunnerEvent::ProgressUpdate(flash_progress) => {
                    state.action_progress.set(flash_progress)
                }
                RunnerEvent::RunnerStateUpdate(runner_state) => {
                    state.runner_state.set(runner_state)
                }
            }
        }
    });

    spawn(async move {
        let mut state = use_context::<Dlpc8445GuiState>();
        let log_rx = use_context::<AppContext>().log_rx.clone();

        while let Some(msg) = log_rx.lock().await.recv().await {
            let mut logs = state.logs.write();

            if logs.len() >= LOG_BUFFER_SIZE {
                _ = logs.pop_front();
            }
            logs.push_back(msg);
        }
    });

    rsx! {
    document::Link { rel: "icon", href: FAVICON }
    document::Link { rel: "stylesheet", href: MAIN_CSS }
    document::Link { rel: "stylesheet", href: TAILWIND_CSS }
    document::Link { rel: "stylesheet", href: COMPONENTS_CSS }

    div {
        class: "max-w-4xl",
        Header { }

        div {
            class: "grid grid-cols-2 gap-4 mb-4",
            div {
                class: "grid grid-cols-1 gap-4",
                ConnectionStatusCard {}
                FirmwareImageCard {}
                FlashControlCard {}
            }
            div {
                HelpInformationCard {}
            }
        }

        LogCard {}

        WebUsbSupportDialog {}
        ReconnectDialog {}
    }

    }
}
