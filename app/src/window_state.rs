//! Remembering the window's size and position between sessions.
//!
//! Geometry is tracked from tao's `Resized`/`Moved` events rather than read
//! back from the window, because `Window::inner_size()` keeps reporting the
//! size the window launched with (see `components::resize`).
//!
//! Writes are debounced: a single drag produces a stream of events, and
//! rewriting the config file on each one would mean hundreds of writes to save
//! one final position.

use dioxus::prelude::*;
use gantry_core::config::{self, Monitor, WindowState};

/// Fallback size for a first run, or a config we won't trust.
pub const DEFAULT_W: f64 = 1180.0;
pub const DEFAULT_H: f64 = 760.0;

/// How long the geometry must hold still before it's written.
const SETTLE_MS: u64 = 700;

/// The remembered geometry, if it's usable.
///
/// Position is deliberately not validated here: the monitor list isn't
/// available until the event loop is running, so [`use_persistence`] rechecks
/// it after launch and recovers the window if it landed somewhere unreachable.
pub fn restore() -> Option<WindowState> {
    let saved = config::load_config().ok().flatten()?.window?;
    saved.is_sane().then_some(saved)
}

/// The size to launch at, falling back to the default.
pub fn restore_size() -> (f64, f64) {
    restore()
        .map(|s| s.clamped_size(crate::MIN_W, crate::MIN_H))
        .unwrap_or((DEFAULT_W, DEFAULT_H))
}

/// Tracks the window's geometry, saves it once it settles, and rescues a
/// window restored onto a display that is no longer attached.
pub fn use_persistence() {
    // `None` until the first event tells us where the window actually is.
    let mut current = use_signal(|| Option::<WindowState>::None);

    dioxus::desktop::use_wry_event_handler(move |event, _| {
        use dioxus::desktop::{WindowEvent, tao::event::Event};
        let Event::WindowEvent { event, .. } = event else {
            return;
        };
        let ctx = dioxus::desktop::window();
        let scale = ctx.window.scale_factor();
        // Seed from where the window actually is. Defaulting the position to
        // 0,0 would mean a session that only ever resized saved a top-left
        // corner it was never at — and reopened there next time.
        let mut next = current.peek().unwrap_or_else(|| {
            let pos = ctx.window.outer_position().ok();
            let (width, height) = restore_size();
            WindowState {
                x: pos.map_or(0.0, |p| p.x as f64 / scale),
                y: pos.map_or(0.0, |p| p.y as f64 / scale),
                width,
                height,
            }
        });
        match event {
            WindowEvent::Resized(size) => {
                next.width = size.width as f64 / scale;
                next.height = size.height as f64 / scale;
            }
            WindowEvent::Moved(pos) => {
                next.x = pos.x as f64 / scale;
                next.y = pos.y as f64 / scale;
            }
            _ => return,
        }
        current.set(Some(next));
    });

    // Put the window back on screen if the display it was saved on is gone.
    use_future(move || async move {
        // One frame's grace so the window exists and monitors are enumerable.
        tokio::time::sleep(std::time::Duration::from_millis(250)).await;
        let Some(saved) = restore() else { return };
        let ctx = dioxus::desktop::window();
        if saved.is_reachable(&monitors()) {
            // `with_position` at build time doesn't always take on macOS, so
            // reassert it now that the window is up.
            use dioxus::desktop::tao::dpi::LogicalPosition;
            ctx.window
                .set_outer_position(LogicalPosition::new(saved.x, saved.y));
        } else {
            // Leave the window wherever the OS placed it. The next save will
            // record somewhere reachable.
            eprintln!("gantry: remembered window position is off-screen; using the default");
        }
    });

    // Debounced writer: only saves once the geometry has held still.
    use_future(move || async move {
        let mut last_saved: Option<WindowState> = restore();
        let mut pending: Option<WindowState> = None;
        loop {
            tokio::time::sleep(std::time::Duration::from_millis(SETTLE_MS)).await;
            let now = *current.peek();
            match (now, pending) {
                // Held still since the previous tick, and it's new: write it.
                (Some(now), Some(prev)) if now == prev && Some(now) != last_saved => {
                    config::save_window_state(now);
                    last_saved = Some(now);
                }
                _ => {}
            }
            pending = now;
        }
    });
}

/// The attached monitors' usable areas, in logical points.
fn monitors() -> Vec<Monitor> {
    let ctx = dioxus::desktop::window();
    ctx.window
        .available_monitors()
        .map(|m| {
            let scale = m.scale_factor();
            let pos = m.position();
            let size = m.size();
            Monitor {
                x: pos.x as f64 / scale,
                y: pos.y as f64 / scale,
                width: size.width as f64 / scale,
                height: size.height as f64 / scale,
            }
        })
        .collect()
}
