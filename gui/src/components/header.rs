use dioxus::prelude::*;
use git_version::git_version;

use crate::components::avatar::{Avatar, AvatarImage, AvatarImageSize};

const APP_VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD_COMMIT: &str = git_version!(fallback = "unknown");
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
                    target: "_blank",
                    rel: "noopener noreferrer",
                    "Version {APP_VERSION} ({BUILD_COMMIT})"
                }
                span { " by {TOOL_AUTHOR} 2026" }
            }
        }
    }
}
