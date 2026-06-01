use dioxus::prelude::*;
use dioxus_icons::lucide::Unplug;

use crate::components::dialog::{Dialog, DialogDescription, DialogTitle};

#[component]
pub fn ReconnectDialog() -> Element {
    rsx! {
        Dialog {
            open: false,
            DialogTitle {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        Unplug {}
                    }
                    "Waiting for Device Reconnect"
                }
            }
            DialogDescription {
                p { "Flashing was interrupted because the device disconnected." }
                p { "Please reconnect the device. Do not close or reload this page while flashing resumes." }
            }
        }
    }
}
