// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::document::eval;
use dioxus::prelude::*;
use dioxus_icons::lucide::Folder;
use dlpc8445_proto::{
    flash::FlashState,
    runner::{RunnerCommand, RunnerState},
};
use tracing_log::log;

use crate::{
    components::{
        button::{Button, ButtonVariant},
        card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
        item::{Item, ItemContent, ItemVariant},
    },
    state::Dlpc8445GuiState,
};

async fn create_flash_state(event: Event<FormData>) -> anyhow::Result<FlashState> {
    if let Some(file) = event.files().pop() {
        // FIXME!
        let bytes = file.read_bytes().await.unwrap();
        return Ok(FlashState::from_buffer(bytes)?);
    }
    return Err(anyhow::anyhow!("No file selected"));
}

#[component]
pub fn FirmwareImageCard() -> Element {
    let state = use_context::<Dlpc8445GuiState>();
    let runner_state = state.runner_state.read().clone();

    rsx! {
            Card {
                CardHeader {
                    CardTitle {
                        "Firmware Image"
                    }
                }
                CardContent {
                    Item {
                        variant: ItemVariant::Outline,
                        ItemContent {
                            input {
                                id: "firmware-upload-input",
                                class: "text-center text-sm text-gray-500",
                                r#type: "file",
                                disabled: runner_state == RunnerState::Running,
                                accept: ".img,application/octet-stream",
                                onchange: move |event| {
                                    spawn(async move {
                                            match create_flash_state(event).await {
                                                Ok(flash_state) => {
                                                    state.send_command(RunnerCommand::LoadImage {image: flash_state}).await;
                                                },
                                                Err(err) => log::error!("Failed to create flash state: {}", err),
                                            };
                                        }
                                    );
                                }
                            }
                        }
                    }
                }
                CardFooter {
                    CardAction {
                    Button {
                        disabled: matches!(*state.runner_state.read(), RunnerState::Running | RunnerState::WaitingForReconnect),
                        variant: ButtonVariant::Outline,
                        onclick: move |_| {
                            let _ = eval(r#"
                                document.getElementById('firmware-upload-input').click();
                            "#);
                        },
                        Folder { size: 20, stroke: "black" },
                        "Load Image"
                    }
                }
            }
        }
    }
}
