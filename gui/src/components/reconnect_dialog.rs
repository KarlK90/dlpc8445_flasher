// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_icons::lucide::Unplug;

use crate::{
    components::dialog::{Dialog, DialogDescription, DialogTitle},
    state::Dlpc8445GuiState,
};

#[component]
pub fn ReconnectDialog() -> Element {
    let runner_state = use_context::<Dlpc8445GuiState>().runner_state;

    rsx! {
        Dialog {
            open: runner_state.read().clone() == dlpc8445_proto::runner::RunnerState::WaitingForReconnect,
            DialogTitle {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        Unplug {}
                    }
                    "Waiting for Device to Reconnect"
                }
            }
            DialogDescription {
                p { "Flashing was interrupted because the device disconnected." }
                p { "Please reconnect the device. Do not close or reload this page while flashing is resuming." }
            }
        }
    }
}
