use dioxus::prelude::*;
use dioxus_icons::lucide::TriangleAlert;

use crate::components::alert_dialog::{
    AlertDialog, AlertDialogAction, AlertDialogActions, AlertDialogCancel, AlertDialogDescription,
    AlertDialogTitle,
};

#[component]
pub fn ModeSwitchAlertDialog() -> Element {
    rsx! {
        AlertDialog {
            open: false,
            AlertDialogTitle {
                div {
                    class: "inline-flex items-center",
                    div {
                        class: "mr-2",
                        TriangleAlert {}
                    }
                    "Enter Flash Mode?"
                }
            }
            AlertDialogDescription {
                p {
                    "Switching the DLPC 8445 from application mode to bootrom invalidates the image currently on flash."
                }
                p {
                    "You must flash a valid firmware image after entering flash mode, or the device will not boot."
                }
                p {
                    "Do you want to continue?"
                }
            }
            AlertDialogActions {
                AlertDialogCancel { "Cancel" }
                AlertDialogAction { "Yes, enter flash mode" }
            }
        }
    }
}
