// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_icons::lucide::{Circle, Usb};
use dlpc8445_proto::runner::{DeviceState, RunnerCommand, RunnerState};

use crate::{
    Dlpc8445GuiState,
    components::{
        button::{Button, ButtonVariant},
        card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
    },
};

#[component]
pub fn ConnectionStatusCard() -> Element {
    let state = use_context::<Dlpc8445GuiState>();
    let status = state.device_state.read().cloned().to_string();
    let color = if *state.device_state.read() == DeviceState::Disconnected {
        "grey"
    } else {
        "lightgreen"
    };

    rsx! {
        Card {
            CardHeader {
                CardTitle {
                    "Connection Status"
                }
            }
            CardContent {
                div {
                    class: "inline-flex items-center font-mono",
                    div {
                        class: "mr-2",
                        Circle { size: 20, stroke: "{ color }", fill: "{color}" }
                    }
                    { status }
                }
            }
            CardFooter {
                CardAction {
                    if cfg!(target_family = "wasm") {
                        Button {
                            disabled: matches!(*state.runner_state.read(), RunnerState::Running | RunnerState::WaitingForReconnect),
                            onclick: move |_| {
                                spawn(
                                    async move {
                                        state.send_command(RunnerCommand::RequestDeviceAccess).await;
                                    }
                                );
                            },
                            variant: ButtonVariant::Outline,
                            Usb { size: 20, stroke: "black" }
                            "Request Device Access"
                        }
                    }
                }
            }
        }
    }
}
