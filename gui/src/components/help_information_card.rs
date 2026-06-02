// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;

use crate::components::card::{Card, CardContent, CardHeader, CardTitle};

#[component]
pub fn HelpInformationCard() -> Element {
    rsx! {
        Card {
            style: "height: 100%",
            CardHeader {
                CardTitle {
                    "Getting Started"
                }
            }
            CardContent {
                ol {
                    class: "list-decimal px-4",
                    li {
                        strong { "Request Device Access" }
                        " - Click \"Request Device Access\" to allow the browser to communicate with the DLPC 8445 via WebUSB."
                    }
                    li {
                        strong { "Choose Firmware Image" }
                        " - Drag and drop the image onto the firmware section, or click \"Load Image\"."
                    }
                    li {
                        strong { "Start Flash" }
                        " - Press \"Start Flash\". If the device is not already in flash mode, you will be prompted to confirm the switch."
                    }
                    li {
                        strong { "Monitor Progress" }
                        " - Watch the progress bar and log viewer for status, progress, and errors."
                    }
                }
            }
        }
    }
}
