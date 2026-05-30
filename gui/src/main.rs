// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    dioxus::launch(App);
}

// ---------------------------------------------------------------------------
// State types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceStatus {
    NoAccess,
    Disconnected,
    Connected,
    InFlashMode,
}

impl DeviceStatus {
    fn label(self) -> &'static str {
        match self {
            Self::NoAccess => "No Device Access",
            Self::Disconnected => "Disconnected",
            Self::Connected => "Connected (Application Mode)",
            Self::InFlashMode => "Connected (Flash Mode)",
        }
    }

    fn css_class(self) -> &'static str {
        match self {
            Self::NoAccess => "status-no-access",
            Self::Disconnected => "status-disconnected",
            Self::Connected => "status-connected",
            Self::InFlashMode => "status-flash-mode",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FlashProgress {
    Idle,
    Flashing,
    Done,
    Error,
    Cancelled,
}

// ---------------------------------------------------------------------------
// Root component
// ---------------------------------------------------------------------------

#[component]
fn App() -> Element {
    let mut device_status = use_signal(|| DeviceStatus::NoAccess);
    let mut flash_progress = use_signal(|| FlashProgress::Idle);
    let mut progress_pct = use_signal(|| 0u8);
    let mut selected_file = use_signal(|| Option::<String>::None);
    let mut log_messages = use_signal(Vec::<String>::new);
    let mut show_flash_mode_modal = use_signal(|| false);
    let mut show_help = use_signal(|| false);

    // --- helpers (mock actions) -------------------------------------------

    let mut add_log = move |msg: String| {
        log_messages.write().push(msg);
    };

    let on_request_device = move |_| {
        device_status.set(DeviceStatus::Connected);
        add_log("WebUSB: device access granted (mock)".into());
    };

    let mut on_file_selected = move |name: String| {
        add_log(format!("Selected image: {name}"));
        selected_file.set(Some(name));
    };

    let start_flash = move |_| {
        if device_status() == DeviceStatus::Connected {
            show_flash_mode_modal.set(true);
            return;
        }
        flash_progress.set(FlashProgress::Flashing);
        progress_pct.set(0);
        add_log("Flash process started (mock)".into());
    };

    let on_cancel = move |_| {
        flash_progress.set(FlashProgress::Cancelled);
        progress_pct.set(0);
        add_log("Flash process cancelled by user".into());
    };

    let on_confirm_flash_mode = move |_| {
        show_flash_mode_modal.set(false);
        device_status.set(DeviceStatus::InFlashMode);
        flash_progress.set(FlashProgress::Flashing);
        progress_pct.set(0);
        add_log("Switched to flash mode (mock)".into());
        add_log("Flash process started (mock)".into());
    };

    let on_cancel_modal = move |_| {
        show_flash_mode_modal.set(false);
        add_log("Flash mode switch cancelled by user".into());
    };

    let on_simulate_progress = move |_| {
        let p = progress_pct();
        if flash_progress() == FlashProgress::Flashing {
            if p < 100 {
                progress_pct.set(p + 10);
                add_log(format!("Flashing... {}%", p + 10));
            }
            if p + 10 >= 100 {
                flash_progress.set(FlashProgress::Done);
                add_log("Flash complete!".into());
            }
        }
    };

    // --- render ------------------------------------------------------------

    rsx! {
        document::Link { rel: "stylesheet", href: MAIN_CSS }

        div { class: "app-container",
            Header {}

            div { class: "main-content",
                // Left column: controls
                div { class: "controls-panel",
                    // WebUSB access
                    section { class: "card",
                        h3 { "Device Access" }
                        p { class: "hint",
                            "WebUSB requires a secure context. Click the button below to request access to the DLPC 8445 device."
                        }
                        button {
                            class: "btn btn-primary",
                            disabled: device_status() != DeviceStatus::NoAccess,
                            onclick: on_request_device,
                            "🔌 Request Device Access"
                        }
                    }

                    // Status indicator
                    section { class: "card",
                        h3 { "Device Status" }
                        div { class: "status-indicator {device_status().css_class()}",
                            span { class: "status-dot" }
                            span { "{device_status().label()}" }
                        }
                    }

                    // File picker
                    section { class: "card",
                        h3 { "Firmware Image" }
                        div { class: "file-picker",
                            button {
                                class: "btn btn-secondary",
                                disabled: device_status() == DeviceStatus::NoAccess,
                                onclick: move |_| on_file_selected("AWOL_DLP_Upgrade.img".into()),
                                "📁 Select Image File"
                            }
                            if let Some(ref name) = selected_file() {
                                span { class: "file-name", "{name}" }
                            }
                        }
                    }

                    // Flash controls
                    section { class: "card",
                        h3 { "Flash Controls" }
                        div { class: "button-group",
                            button {
                                class: "btn btn-success",
                                disabled: selected_file().is_none()
                                    || flash_progress() == FlashProgress::Flashing
                                    || device_status() == DeviceStatus::NoAccess
                                    || device_status() == DeviceStatus::Disconnected,
                                onclick: start_flash,
                                "⚡ Start Flash"
                            }
                            button {
                                class: "btn btn-danger",
                                disabled: flash_progress() != FlashProgress::Flashing,
                                onclick: on_cancel,
                                "✖ Cancel"
                            }
                            button {
                                class: "btn btn-secondary",
                                disabled: flash_progress() != FlashProgress::Flashing,
                                onclick: on_simulate_progress,
                                "⏩ Simulate Progress"
                            }
                        }
                    }

                    // Progress bar
                    section { class: "card",
                        h3 { "Progress" }
                        div { class: "progress-bar-container",
                            div {
                                class: "progress-bar-fill",
                                style: "width: {progress_pct()}%",
                            }
                        }
                        p { class: "progress-text",
                            match flash_progress() {
                                FlashProgress::Idle => "Idle".to_string(),
                                FlashProgress::Flashing => format!("Flashing… {}%", progress_pct()),
                                FlashProgress::Done => "✅ Complete".to_string(),
                                FlashProgress::Error => "❌ Error".to_string(),
                                FlashProgress::Cancelled => "⚠ Cancelled".to_string(),
                            }
                        }
                    }
                }

                // Right column: log viewer + help
                div { class: "info-panel",
                    section { class: "card log-card",
                        h3 { "Log" }
                        div { class: "log-viewer",
                            if log_messages().is_empty() {
                                p { class: "log-empty", "No messages yet." }
                            }
                            for (i, msg) in log_messages().iter().enumerate() {
                                p { key: "{i}", class: "log-entry", "{msg}" }
                            }
                        }
                    }

                    section { class: "card",
                        h3 {
                            class: "help-toggle",
                            onclick: move |_| show_help.set(!show_help()),
                            if show_help() { "▾ Help & Information" } else { "▸ Help & Information" }
                        }
                        if show_help() {
                            HelpSection {}
                        }
                    }
                }
            }

            // Flash-mode confirmation modal
            if show_flash_mode_modal() {
                div { class: "modal-overlay",
                    div { class: "modal",
                        h2 { "⚠️ Enter Flash Mode?" }
                        div { class: "modal-body",
                            p { class: "warning-text",
                                "Switching the DLPC 8445 from application mode to bootrom "
                                strong { "invalidates the image currently on flash." }
                            }
                            p {
                                "You must flash a valid firmware image immediately after entering flash mode, "
                                "or the device may not boot."
                            }
                            p { "Do you want to continue?" }
                        }
                        div { class: "modal-actions",
                            button {
                                class: "btn btn-danger",
                                onclick: on_confirm_flash_mode,
                                "Yes, enter flash mode"
                            }
                            button {
                                class: "btn btn-secondary",
                                onclick: on_cancel_modal,
                                "Cancel"
                            }
                        }
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Sub-components
// ---------------------------------------------------------------------------

#[component]
fn Header() -> Element {
    rsx! {
        header { class: "app-header",
            h1 { "DLPC 8445 Flasher" }
            p { class: "subtitle", "Web-based firmware flash tool for the Texas Instruments DLPC 8445" }
        }
    }
}

#[component]
fn HelpSection() -> Element {
    rsx! {
        div { class: "help-content",
            h4 { "Getting Started" }
            ol {
                li {
                    strong { "Request Device Access" }
                    " – Click \"Request Device Access\" to allow the browser to communicate with the DLPC 8445 via WebUSB. "
                    "A secure context (HTTPS) is required."
                }
                li {
                    strong { "Select Firmware Image" }
                    " – Use \"Select Image File\" to pick the firmware binary (e.g. "
                    code { "AWOL_DLP_Upgrade.img" }
                    ")."
                }
                li {
                    strong { "Start Flash" }
                    " – Press \"Start Flash\". If the device is not already in flash mode you will be prompted to confirm the switch."
                }
                li {
                    strong { "Monitor Progress" }
                    " – Watch the progress bar and log viewer for status updates."
                }
            }

            h4 { "Important Notes" }
            ul {
                li {
                    "Entering flash mode "
                    strong { "invalidates" }
                    " the current firmware on the device. "
                    "Always have a valid image ready before switching."
                }
                li { "Do not disconnect the device while flashing is in progress." }
                li { "The flash process will automatically retry sectors that fail validation (up to 3 attempts)." }
                li { "This tool requires a Chromium-based browser with WebUSB support (e.g. Chrome, Edge)." }
            }

            h4 { "Troubleshooting" }
            ul {
                li {
                    strong { "\"No Device Access\"" }
                    " – Make sure the page is served over HTTPS and you have clicked \"Request Device Access\"."
                }
                li {
                    strong { "Device disconnects during flash" }
                    " – The tool will wait for reconnection and resume from the interrupted sector."
                }
                li {
                    strong { "Repeated validation failures" }
                    " – The flash chip may be defective. Try a different device."
                }
            }
        }
    }
}
