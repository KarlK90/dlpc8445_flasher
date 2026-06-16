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
    let status = state.device_state.read().cloned();
    let color = if status == DeviceState::Disconnected {
        "grey"
    } else {
        "lightgreen"
    };
    let version = if let DeviceState::ConnectedApplication {
        version,
        extended_version,
    } = status
    {
        let commit_id = String::from_utf8_lossy(&extended_version.commit_id);
        format!("Firmware version: {version} ({commit_id})")
    } else {
        format!("Firmware version: unknown")
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
                    "{status}"
                }
                div {
                    class: "mt-2 font-mono",
                    div {
                        "{version}"
                    }
                }
            }
            CardFooter {
                CardAction {
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
