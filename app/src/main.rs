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
#[cfg(feature = "desktop")]
mod window_state;

use components::{BuildsScreen, Onboarding, ResizeHandles};
use state::AppState;

const MAIN_CSS: Asset = asset!("/assets/main.css");

/// The window's minimum size. Shared so the builder, the resize handles and
/// the restore path can't drift apart — if they did, dragging past the
/// minimum would move the window's origin while its size stayed put.
pub const MIN_W: f64 = 720.0;
pub const MIN_H: f64 = 480.0;

fn main() {
    // On desktop, override Dioxus's default window config (which sets
    // always-on-top for dev convenience) and give the window a real title.
    #[cfg(feature = "desktop")]
    {
        use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
        // Reopen where we left off. The size is safe to apply up front; the
        // position is rechecked against the attached displays after launch,
        // when the monitor list is available.
        let (width, height) = window_state::restore_size();
        #[allow(unused_mut)]
        let mut window = WindowBuilder::new()
            .with_title("Gantry")
            .with_always_on_top(false)
            .with_inner_size(LogicalSize::new(width, height))
            .with_min_inner_size(LogicalSize::new(MIN_W, MIN_H));
        if let Some(saved) = window_state::restore() {
            use dioxus::desktop::tao::dpi::LogicalPosition;
            window = window.with_position(LogicalPosition::new(saved.x, saved.y));
        }
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
    #[cfg(feature = "desktop")]
    crate::window_state::use_persistence();

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

    // macOS hides the titlebar and runs the UI under the traffic lights;
    // Windows and Linux keep their native decorations. The CSS that
    // compensates for that — the traffic-light inset, the drag strip — hangs
    // off this class rather than applying everywhere.
    let chrome = if cfg!(target_os = "macos") {
        " mac"
    } else {
        ""
    };

    rsx! {
        document::Stylesheet { href: MAIN_CSS }
        main { class: if vibrant() { "app vibrant{chrome}" } else { "app{chrome}" },
            ResizeHandles {}
            if state.has_token() {
                BuildsScreen {}
            } else {
                // Onboarding has no toolbar to drag from, so it keeps the
                // full-width strip under the traffic lights. Elsewhere the
                // native titlebar already serves that purpose.
                if cfg!(target_os = "macos") {
                    div { class: "drag-strip", onmousedown: move |_| start_drag() }
                }
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
