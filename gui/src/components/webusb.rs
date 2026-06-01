use dioxus::prelude::*;

use crate::components::dialog::{Dialog, DialogDescription, DialogTitle};

#[component]
pub fn WebUsbSupportModal() -> Element {
    rsx! {
        Dialog {
            open: !webusb_web::Usb::new().is_ok(),
            DialogTitle {
                "❌ WebUSB not supported"
            }
            DialogDescription {
                "This browser does not support WebUSB. See "
                a {
                    href:"https://caniuse.com/webusb",
                    "Can I use: WebUSB"
                 }
                " for a list of browsers supporting WebUSB."
            }
        }
    }
}
