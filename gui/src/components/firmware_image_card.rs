use dioxus::prelude::*;
use dioxus_icons::lucide::Folder;

use crate::components::{
    button::{Button, ButtonVariant},
    card::{Card, CardAction, CardContent, CardFooter, CardHeader, CardTitle},
    item::{Item, ItemContent, ItemVariant},
};

#[component]
pub fn FirmwareImageCard() -> Element {
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
                        p {
                            class: "text-center text-sm text-gray-500",
                            "Drag and drop firmware image here"
                        }
                    }
                }
            }
            CardFooter {
                CardAction {
                    Button {
                        variant: ButtonVariant::Outline,
                        Folder { size: 20, stroke: "black" },
                        "Load Image"
                    }
                }
            }
        }
    }
}
