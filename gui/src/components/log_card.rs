use dioxus::prelude::*;
use dioxus_primitives::scroll_area::{ScrollArea, ScrollDirection};

use crate::components::card::{Card, CardContent, CardHeader, CardTitle};

#[component]
pub fn LogCard() -> Element {
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
                        p { }
                    }
                }
            }
        }
    }
}
