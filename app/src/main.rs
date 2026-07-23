//! Gantry — a cross-platform Codemagic client (desktop + mobile), built with Dioxus.
//!
//! The data layer is shared with the terminal client via `gantry-core`; this
//! crate only owns the view layer. The scaffold currently ships two screens:
//! token onboarding and the build list.

use dioxus::prelude::*;

mod clipboard;
mod components;
#[cfg(feature = "desktop")]
mod menu;
mod notify;
mod state;

use components::{BuildsScreen, Onboarding, ResizeHandles};
use state::AppState;

const MAIN_CSS: Asset = asset!("/assets/main.css");

fn main() {
    // On desktop, override Dioxus's default window config (which sets
    // always-on-top for dev convenience) and give the window a real title.
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
        #[allow(unused_mut)]
        let mut window = WindowBuilder::new()
            .with_title("Gantry")
            .with_always_on_top(false)
            .with_inner_size(LogicalSize::new(1180.0, 760.0))
            .with_min_inner_size(LogicalSize::new(720.0, 480.0));
        // Let the frosted-glass content run under the traffic lights, and make
        // the window transparent so the desktop shows through the vibrancy
        // material we apply at runtime (macOS).
        #[cfg(target_os = "macos")]
        {
            use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
            window = window
                .with_titlebar_transparent(true)
                .with_fullsize_content_view(true)
                .with_title_hidden(true)
                .with_transparent(true);
        }
        dioxus::LaunchBuilder::desktop()
            .with_cfg(Config::new().with_window(window).with_menu(menu::build()))
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

    // Apply the native macOS vibrancy material behind the (transparent)
    // webview, so the desktop blurs through the frosted UI. When it succeeds we
    // drop the CSS fallback gradient via the `vibrant` class.
    #[allow(unused_mut)]
    let mut vibrant = use_signal(|| false);
    #[cfg(all(feature = "desktop", target_os = "macos"))]
    use_effect(move || {
        use dioxus::desktop::window;
        use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState, apply_vibrancy};
        let ctx = window();
        if apply_vibrancy(
            &*ctx.window,
            NSVisualEffectMaterial::Sidebar,
            Some(NSVisualEffectState::Active),
            None,
        )
        .is_ok()
        {
            vibrant.set(true);
        }
    });

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        main { class: if vibrant() { "app vibrant" } else { "app" },
            ResizeHandles {}
            if state.has_token() {
                BuildsScreen {}
            } else {
                // Onboarding has no toolbar to drag from, so it keeps the
                // full-width strip under the traffic lights.
                div { class: "drag-strip", onmousedown: move |_| start_drag() }
                Onboarding {}
            }
        }
    }
}

/// Starts a native window drag (used by the titlebar drag strip / toolbar).
pub fn start_drag() {
    #[cfg(feature = "desktop")]
    dioxus::desktop::window().drag();
}
