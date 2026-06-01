use dioxus::prelude::*;

use crate::components::{
    connection_status_card::ConnectionStatusCard, firmware_image_card::FirmwareImageCard,
    flash_controls_card::FlashControlsCard, header::Header,
    help_information_card::HelpInformationCard, log_card::LogCard,
    mode_switch_dialog::ModeSwitchAlertDialog, reconnect_dialog::ReconnectDialog,
    webusb_dialog::WebUsbSupportDialog,
};

mod components;

const FAVICON: Asset = asset!("/assets/favicon.ico");
const MAIN_CSS: Asset = asset!("/assets/styling/main.css");
const TAILWIND_CSS: Asset = asset!("/assets/tailwind.css");
const COMPONENTS_CSS: Asset = asset!("/assets/dx-components-theme.css");

fn main() {
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    rsx! {
    document::Link { rel: "icon", href: FAVICON }
    document::Link { rel: "stylesheet", href: MAIN_CSS }
    document::Link { rel: "stylesheet", href: TAILWIND_CSS }
    document::Link { rel: "stylesheet", href: COMPONENTS_CSS }

    div {
        class: "max-w-4xl",
        Header { }

        div {
            class: "grid grid-cols-2 gap-4 mb-4",
            div {
                class: "grid grid-cols-1 gap-4",
                ConnectionStatusCard {}
                FirmwareImageCard {}
                FlashControlsCard {}
            }
            div {
                HelpInformationCard {}
            }
        }

        LogCard {}

        WebUsbSupportDialog {}
        ReconnectDialog {}
        ModeSwitchAlertDialog {}
    }

    }
}
