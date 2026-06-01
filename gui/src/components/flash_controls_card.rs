use dioxus::prelude::*;
use dioxus_icons::lucide::Sparkles;

use crate::components::{
    button::{Button, ButtonVariant},
    card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
    progress::Progress,
};

#[component]
pub fn FlashControlsCard() -> Element {
    rsx! {
        Card {
            CardHeader {
                CardTitle {
                    "Flash Controls"
                }
            }
            CardContent {
                Progress {
                    aria_label: "Progressbar Demo",
                    value: 0 as f64
                }
                p {
                    "0 / 0"
                }
            }
            CardFooter {
                CardAction {
                    Button {
                        variant: ButtonVariant::Outline,
                        Sparkles { size: 20, stroke: "black" }
                        "Start Flash"
                    }
                }
            }
        }
    }
}
