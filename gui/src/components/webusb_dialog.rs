// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_icons::lucide::OctagonX;

use crate::components::dialog::{Dialog, DialogDescription, DialogTitle};

#[component]
pub fn WebUsbSupportDialog() -> Element {
    rsx! {
        Dialog {
            open: webusb_web::Usb::new().is_err(),
            DialogTitle {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        OctagonX { }
                    }
                    "Your browser does not support WebUSB"
                }
            }
            DialogDescription {
                p { "See "
                    a {
                        class: "font-medium text-blue-500 hover:underline",
                        href:"https://caniuse.com/webusb",
                        target: "_blank",
                        "Can I use WebUSB?"
                    }
                    " for a list of supported browsers."
                }
            }
        }
    }
}
