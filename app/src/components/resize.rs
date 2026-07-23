//! Window resize handles along the window edges.
//!
//! The macOS window uses a full-size content view so the UI runs under the
//! traffic lights. That puts the webview over the window's own resize margin,
//! and WebKit sets the cursor for anything it covers — so the edges show a
//! plain arrow instead of a resize cursor.
//!
//! Handing the edges to the OS isn't an option either: tao's
//! `drag_resize_window` returns `NotSupported` on macOS. So these thin strips
//! both provide the cursor (CSS) and perform the resize (tracking the pointer
//! and moving the window ourselves).
//!
//! One wrinkle: `Window::inner_size()` keeps returning the size the window
//! started with, even after a resize that visibly took effect. Reading it
//! mid-drag would restart every drag from that stale baseline and make the
//! window jump. So the current size is tracked here instead — seeded once at
//! startup and updated from tao's `Resized` events, which do fire.

use dioxus::prelude::*;

use crate::{MIN_H, MIN_W};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    N,
    S,
    E,
    W,
    NE,
    NW,
    SE,
    SW,
}

impl Edge {
    const ALL: [Edge; 8] = [
        Edge::N,
        Edge::S,
        Edge::E,
        Edge::W,
        Edge::NE,
        Edge::NW,
        Edge::SE,
        Edge::SW,
    ];

    /// Also the CSS class, which carries both position and cursor.
    fn name(self) -> &'static str {
        match self {
            Edge::N => "n",
            Edge::S => "s",
            Edge::E => "e",
            Edge::W => "w",
            Edge::NE => "ne",
            Edge::NW => "nw",
            Edge::SE => "se",
            Edge::SW => "sw",
        }
    }

    fn moves_west(self) -> bool {
        matches!(self, Edge::W | Edge::NW | Edge::SW)
    }
    fn moves_east(self) -> bool {
        matches!(self, Edge::E | Edge::NE | Edge::SE)
    }
    fn moves_north(self) -> bool {
        matches!(self, Edge::N | Edge::NE | Edge::NW)
    }
    fn moves_south(self) -> bool {
        matches!(self, Edge::S | Edge::SE | Edge::SW)
    }
}

/// Window geometry in logical points: origin plus size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Geometry {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// The geometry `start` becomes when `edge` is dragged by (`dx`, `dy`).
///
/// Dragging a west or north edge changes the origin as well as the size. Once
/// the size hits the minimum the origin stops too, so the opposite edge stays
/// pinned instead of the whole window sliding away under the pointer.
pub fn resized(start: Geometry, edge: Edge, dx: f64, dy: f64) -> Geometry {
    let mut out = start;

    if edge.moves_east() {
        out.w = (start.w + dx).max(MIN_W);
    } else if edge.moves_west() {
        out.w = (start.w - dx).max(MIN_W);
        out.x = start.x + (start.w - out.w);
    }

    if edge.moves_south() {
        out.h = (start.h + dy).max(MIN_H);
    } else if edge.moves_north() {
        out.h = (start.h - dy).max(MIN_H);
        out.y = start.y + (start.h - out.h);
    }

    out
}

/// An in-progress drag: which edge, where the pointer started, and the
/// geometry at that moment.
#[derive(Clone, Copy)]
struct Drag {
    edge: Edge,
    from_x: f64,
    from_y: f64,
    start: Geometry,
}

/// The window's logical size as last reported by tao, or `None` before the
/// first reading.
#[cfg(feature = "desktop")]
fn live_size() -> (f64, f64) {
    let ctx = dioxus::desktop::window();
    let scale = ctx.window.scale_factor();
    let size = ctx.window.inner_size();
    (size.width as f64 / scale, size.height as f64 / scale)
}

/// Current origin in logical points. Unlike the size, this getter stays
/// accurate after a programmatic move.
#[cfg(feature = "desktop")]
fn origin() -> Option<(f64, f64)> {
    let ctx = dioxus::desktop::window();
    let scale = ctx.window.scale_factor();
    let pos = ctx.window.outer_position().ok()?;
    Some((pos.x as f64 / scale, pos.y as f64 / scale))
}

#[cfg(feature = "desktop")]
fn apply(geo: Geometry) {
    use dioxus::desktop::{LogicalSize, tao::dpi::LogicalPosition};
    let ctx = dioxus::desktop::window();
    // Size first: moving the origin first would make a clamped resize visibly
    // shift the window before snapping back.
    ctx.window.set_inner_size(LogicalSize::new(geo.w, geo.h));
    ctx.window
        .set_outer_position(LogicalPosition::new(geo.x, geo.y));
}

#[component]
pub fn ResizeHandles() -> Element {
    let mut drag = use_signal(|| Option::<Drag>::None);
    // Authoritative window size, because `inner_size()` goes stale. Seeded
    // from tao (correct at startup) and kept current by `Resized` events.
    #[cfg(feature = "desktop")]
    let mut size = use_signal(live_size);
    #[cfg(feature = "desktop")]
    dioxus::desktop::use_wry_event_handler(move |event, _| {
        use dioxus::desktop::{WindowEvent, tao::event::Event};
        if let Event::WindowEvent {
            event: WindowEvent::Resized(new),
            ..
        } = event
        {
            let scale = dioxus::desktop::window().window.scale_factor();
            size.set((new.width as f64 / scale, new.height as f64 / scale));
        }
    });
    // Owned snapshot: a live read guard inside `rsx!` stays borrowed across
    // the reactive flush, and the move handler writes this same signal.
    let active: Option<Edge> = drag.read().as_ref().map(|d| d.edge);

    let mut begin = move |edge: Edge, e: Event<MouseData>| {
        #[cfg(feature = "desktop")]
        {
            let Some((x, y)) = origin() else { return };
            let (w, h) = *size.peek();
            let start = Geometry { x, y, w, h };
            let p = e.data().screen_coordinates();
            drag.set(Some(Drag {
                edge,
                from_x: p.x,
                from_y: p.y,
                start,
            }));
        }
        #[cfg(not(feature = "desktop"))]
        {
            let _ = (edge, e);
        }
    };

    rsx! {
        for edge in Edge::ALL {
            div {
                key: "{edge.name()}",
                class: "resize-handle resize-{edge.name()}",
                onmousedown: move |e: Event<MouseData>| begin(edge, e),
            }
        }
        // While dragging, this overlay owns the pointer, so the move events
        // keep arriving even when the cursor outruns the thin edge strip.
        if let Some(edge) = active {
            div {
                class: "resize-capture {edge.name()}",
                onmousemove: move |e: Event<MouseData>| {
                    let Some(d) = *drag.peek() else { return };
                    let p = e.data().screen_coordinates();
                    #[cfg(feature = "desktop")]
                    {
                        let next = resized(d.start, d.edge, p.x - d.from_x, p.y - d.from_y);
                        apply(next);
                        // Don't wait for the Resized event: the next move may
                        // arrive first, and it must not see the old size.
                        size.set((next.w, next.h));
                    }
                    #[cfg(not(feature = "desktop"))]
                    let _ = (d, p);
                },
                onmouseup: move |_| drag.set(None),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Edge, Geometry, MIN_H, MIN_W, resized};

    const START: Geometry = Geometry {
        x: 100.0,
        y: 100.0,
        w: 1000.0,
        h: 800.0,
    };

    #[test]
    fn dragging_east_grows_width_and_leaves_the_origin() {
        let g = resized(START, Edge::E, 50.0, 0.0);
        assert_eq!((g.x, g.y, g.w, g.h), (100.0, 100.0, 1050.0, 800.0));
    }

    #[test]
    fn dragging_west_moves_the_origin_with_the_size() {
        let g = resized(START, Edge::W, 50.0, 0.0);
        assert_eq!((g.x, g.w), (150.0, 950.0));
        // The east edge stays where it was.
        assert_eq!(g.x + g.w, START.x + START.w);
    }

    #[test]
    fn dragging_north_moves_the_origin_with_the_size() {
        let g = resized(START, Edge::N, 0.0, 60.0);
        assert_eq!((g.y, g.h), (160.0, 740.0));
        assert_eq!(g.y + g.h, START.y + START.h);
    }

    #[test]
    fn corners_move_both_axes() {
        let g = resized(START, Edge::NW, 40.0, 60.0);
        assert_eq!((g.x, g.y, g.w, g.h), (140.0, 160.0, 960.0, 740.0));
    }

    #[test]
    fn a_south_east_drag_never_touches_the_origin() {
        let g = resized(START, Edge::SE, -10.0, -20.0);
        assert_eq!((g.x, g.y), (START.x, START.y));
        assert_eq!((g.w, g.h), (990.0, 780.0));
    }

    /// The bug this guards: without clamping the origin too, dragging past the
    /// minimum keeps sliding the window while its size stays put.
    #[test]
    fn past_the_minimum_the_opposite_edge_stays_pinned() {
        let g = resized(START, Edge::W, 5000.0, 0.0);
        assert_eq!(g.w, MIN_W);
        assert_eq!(g.x + g.w, START.x + START.w);

        let g = resized(START, Edge::N, 0.0, 5000.0);
        assert_eq!(g.h, MIN_H);
        assert_eq!(g.y + g.h, START.y + START.h);
    }

    #[test]
    fn shrinking_from_the_east_still_stops_at_the_minimum() {
        let g = resized(START, Edge::E, -5000.0, 0.0);
        assert_eq!((g.x, g.w), (100.0, MIN_W));
    }
}
