// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_icons::lucide::{CircleCheck, Sparkles};
use dlpc8445_proto::{
    protocol::SwitchApplicationOption,
    runner::{DeviceState, RunnerAction, RunnerCommand, RunnerState},
};

use crate::{
    components::{
        button::{Button, ButtonVariant},
        card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
        mode_switch_dialog::ModeSwitchAlertDialog,
        progress::Progress,
    },
    state::Dlpc8445GuiState,
};

#[component]
pub fn FlashControlCard() -> Element {
    let state = use_context::<Dlpc8445GuiState>();
    let progress = state.action_progress.read().cloned();
    let mut open = use_signal(|| false);
    let runner_state = state.runner_state.read().clone();

    rsx! {
        Card {
            CardHeader {
                CardTitle {
                    "Flash Control"
                }
            }
            CardContent {
                Progress {
                    aria_label: "Flash Progress",
                    value: progress.current as f64,
                    max: progress.total as f64,
                }
                p {
                    class: "mt-2 font-mono",
                    "{ progress.current } / { progress.total } -  { runner_state.to_string() }",
                }
            }
            CardFooter {
                CardAction {
                    Button {
                        disabled: (*state.device_state.read() == DeviceState::Disconnected) || matches!(*state.runner_state.read(), RunnerState::Running | RunnerState::WaitingForReconnect) || progress.total == 0,
                        onclick: move |_| {
                            let device_state = *state.device_state.read();
                            if matches!(device_state, DeviceState::ConnectedApplication{ version: _, extended_version: _}) {
                                open.set(true);
                            } else {
                                spawn(
                                    async move {
                                        state.send_command(RunnerCommand::StartAction { action: RunnerAction::Flash, enter_flash_mode: true }).await;
                                    }
                                );
                            }
                        },
                        variant: ButtonVariant::Outline,
                        Sparkles { size: 20, stroke: "black" }
                        "Start Flash"
                    }
                    Button {
                        disabled: (*state.device_state.read() != DeviceState::ConnectedFlashMode) || *state.runner_state.read() != RunnerState::Done,
                        class: "ml-4",
                        onclick: move |_| {
                                spawn(
                                    async move {
                                        state.send_command(RunnerCommand::SwitchMode { mode: SwitchApplicationOption::MainApplication }).await;
                                    }
                                );
                        },
                        variant: ButtonVariant::Outline,
                        CircleCheck{ size: 20, stroke: "black" }
                        "Switch to Application Mode"
                    }
                }
            }
        }

        ModeSwitchAlertDialog {
            open: open,
            on_confirm: move |_| {
                open.set(false);
                spawn(async move {
                    state
                        .send_command(RunnerCommand::StartAction {
                            action: RunnerAction::Flash,
                            enter_flash_mode: true,
                        })
                        .await;
                });
            }
        }
    }
}
