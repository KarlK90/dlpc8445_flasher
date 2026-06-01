use dioxus::prelude::*;
use dioxus_icons::lucide::{Circle, Usb};

use crate::components::{
    button::{Button, ButtonVariant},
    card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
};

#[component]
pub fn ConnectionStatusCard() -> Element {
    rsx! {
        Card {
            CardHeader {
                CardTitle {
                    "Connection Status"
                }
            }
            CardContent {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        Circle { size: 20, stroke: "grey", fill: "grey" }
                    }
                    "No Device Access"
                }
            }
            CardFooter {
                CardAction {
                    Button {
                        variant: ButtonVariant::Outline,
                        Usb { size: 20, stroke: "black" }
                        "Request Device Access"
                    }
                }
            }
        }
    }
}
