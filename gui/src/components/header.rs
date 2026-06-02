// SPDX-License-Identifier: MIT
// SPDX-FileCopyrightText: 2026 Stefan Kerkmann <karlk90@pm.me>

use dioxus::prelude::*;

use crate::components::avatar::{Avatar, AvatarImage, AvatarImageSize};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const REPOSITORY_URL: &str = "https://github.com/KarlK90/dlpc8445_flasher";
const TOOL_AUTHOR: &str = "Stefan Kerkmann";
const LOGO: Asset = asset!("/assets/logo.svg");

#[component]
pub fn Header() -> Element {
    rsx! {
        div {
            class: "text-center my-8",
            h1 {
                class: "text-3xl font-bold inline-flex items-center",
                Avatar {
                    class: "mr-1",
                    size: AvatarImageSize::Small,
                    aria_label: "Loading avatar",
                    AvatarImage {
                        src: LOGO,
                        alt: "",
                    }
                }
                span {
                    "DLPC 8445 Flasher"
                }
            }
            div {
                a {
                    href: REPOSITORY_URL,
                    class: "hover:underline",
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "Version {APP_VERSION}"
                }
                span { " by {TOOL_AUTHOR} 2026" }
            }
        }
    }
}
