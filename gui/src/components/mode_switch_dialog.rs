// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;
use dioxus_icons::lucide::TriangleAlert;

use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogDescription,
    AlertDialogTitle,
};

#[component]
pub fn ModeSwitchAlertDialog(open: Signal<bool>, on_confirm: EventHandler<MouseEvent>) -> Element {
    rsx! {
        AlertDialog {
            open: open(),
            on_open_change: move |v| open.set(v),
            AlertDialogTitle {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        TriangleAlert {}
                    }
                    "Enter Flash Mode"
                }
            }
            AlertDialogDescription {
                p {
                    "Switching the DLPC 8445 from application mode to Boot ROM invalidates the image currently on flash."
                }
                p {
                    class: "font-medium",
                    "You must flash a valid firmware image after entering flash mode, or the device will not boot."
                }
                p {
                    "Do you want to continue?"
                }
            }
            AlertDialogActions {
                AlertDialogCancel { "Cancel" }
                AlertDialogAction { on_click: on_confirm, "Yes, enter flash mode" }
            }
        }
    }
}
