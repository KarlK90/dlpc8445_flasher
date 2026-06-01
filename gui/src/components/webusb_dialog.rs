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
                    "WebUSB not supported"
                }
            }
            DialogDescription {
                p { "This browser does not support WebUSB. See "
                    a {
                        href:"https://caniuse.com/webusb",
                        "Can I use: WebUSB"
                    }
                    " for a list of browsers supporting WebUSB."
                }
            }
        }
    }
}
