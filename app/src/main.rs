//! Cross-platform Codemagic client (desktop + mobile) built with Dioxus.
//!
//! The data layer is shared with the terminal client via `codemagic-core`; this
//! crate only owns the view layer. The scaffold currently ships two screens:
//! token onboarding and the build list.

use dioxus::prelude::*;

mod components;
mod state;

use components::{BuildsScreen, Onboarding};
use state::AppState;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // On desktop, override Dioxus's default window config (which sets
    // always-on-top for dev convenience) and give the window a real title.
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, WindowBuilder};
        let window = WindowBuilder::new()
            .with_title("Codemagic")
            .with_always_on_top(false);
        dioxus::LaunchBuilder::desktop()
            .with_cfg(Config::new().with_window(window))
            .launch(App);
    }

    // Mobile / web use the default launcher.
    #[cfg(not(feature = "desktop"))]
    dioxus::launch(App);
}

#[component]
fn App() -> Element {
    // App-wide state, shared with descendants via context.
    use_context_provider(AppState::new);
    let state = use_context::<AppState>();

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        main { class: "app",
            if state.has_token() {
                BuildsScreen {}
            } else {
                Onboarding {}
            }
        }
    }
}
