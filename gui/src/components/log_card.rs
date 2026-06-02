// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};

use crate::{
    LOG_BUFFER_SIZE,
    components::card::{Card, CardContent, CardHeader, CardTitle},
    state::Dlpc8445GuiState,
};

#[component]
pub fn LogCard() -> Element {
    let state = use_context::<Dlpc8445GuiState>();

    rsx! {
        Card {
            CardHeader {
                CardTitle {
                    "Log"
                }
            }
            CardContent {
                ScrollArea {
                    width: "100%",
                    height: "20rem",
                    border: "1px solid var(--primary-color-6)",
                    border_radius: "0.5em",
                    padding: "0 1em 1em 1em",
                    direction: ScrollDirection::Vertical,
                    tabindex: "0",
                    div {
                        class: "font-mono text-xs whitespace-pre-wrap break-all",
                        for msg in state.logs.read().iter().rev() {
                            p { key: "{msg.id}", class: "log-entry", "{msg.msg}" }
                        }
                    }
                }
                p {
                    class: "text-xs mt-2 text-gray-500",
                    "Only the last {LOG_BUFFER_SIZE} messages are shown. The "
                    a {
                        class: "hover:underline",
                        target: "_blank",
                        href: "https://developer.chrome.com/docs/devtools/open",
                        "browser console"
                    }
                    " contains the full log."
                }
            }
        }
    }
}
