//! Small inline SVG icons that inherit `currentColor`.

use dioxus::prelude::*;

#[component]
pub fn PlusIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            line { x1: "12", y1: "5", x2: "12", y2: "19" }
            line { x1: "5", y1: "12", x2: "19", y2: "12" }
        }
    }
}

#[component]
pub fn BranchIcon() -> Element {
    rsx! {
        svg {
            class: "icon glyph", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            line { x1: "6", y1: "3", x2: "6", y2: "15" }
            circle { cx: "18", cy: "6", r: "3" }
            circle { cx: "6", cy: "18", r: "3" }
            path { d: "M18 9a9 9 0 0 1-9 9" }
        }
    }
}

#[component]
pub fn TagIcon() -> Element {
    rsx! {
        svg {
            class: "icon glyph", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M20.59 13.41l-7.17 7.17a2 2 0 0 1-2.83 0L2 12V2h10l8.59 8.59a2 2 0 0 1 0 2.82z" }
            line { x1: "7", y1: "7", x2: "7.01", y2: "7" }
        }
    }
}

#[component]
pub fn RefreshIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            polyline { points: "23 4 23 10 17 10" }
            path { d: "M20.49 15a9 9 0 1 1-2.12-9.36L23 10" }
        }
    }
}

#[component]
pub fn GearIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            circle { cx: "12", cy: "12", r: "3" }
            path { d: "M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 0 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 0 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 0 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 0 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" }
        }
    }
}

#[component]
pub fn ChevronIcon() -> Element {
    rsx! {
        svg {
            class: "icon chevron", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2.4", stroke_linecap: "round", stroke_linejoin: "round",
            polyline { points: "9 18 15 12 9 6" }
        }
    }
}

#[component]
pub fn DownloadIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4" }
            polyline { points: "7 10 12 15 17 10" }
            line { x1: "12", y1: "15", x2: "12", y2: "3" }
        }
    }
}

#[component]
pub fn ExternalLinkIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" }
            polyline { points: "15 3 21 3 21 9" }
            line { x1: "10", y1: "14", x2: "21", y2: "3" }
        }
    }
}

#[component]
pub fn InfoIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            circle { cx: "12", cy: "12", r: "9" }
            line { x1: "12", y1: "11", x2: "12", y2: "16" }
            line { x1: "12", y1: "8", x2: "12", y2: "8" }
        }
    }
}

/// Re-run: a circular arrow with a play head.
#[component]
pub fn RerunIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            path { d: "M20 12a8 8 0 1 1-2.6-5.9" }
            polyline { points: "20 3 20 8 15 8" }
            path { d: "M10 9.5l5 2.5-5 2.5z", fill: "currentColor" }
        }
    }
}

/// Word-wrap toggle: a line that bends back on itself.
#[component]
pub fn WrapIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            line { x1: "3", y1: "6", x2: "21", y2: "6" }
            path { d: "M3 12h15a3 3 0 0 1 0 6h-4" }
            polyline { points: "16 15 13 18 16 21" }
            line { x1: "3", y1: "18", x2: "9", y2: "18" }
        }
    }
}

/// Jump to top / bottom, depending on `up`.
#[component]
pub fn JumpIcon(up: bool) -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            if up {
                line { x1: "5", y1: "4", x2: "19", y2: "4" }
                line { x1: "12", y1: "20", x2: "12", y2: "9" }
                polyline { points: "7 14 12 9 17 14" }
            } else {
                line { x1: "5", y1: "20", x2: "19", y2: "20" }
                line { x1: "12", y1: "4", x2: "12", y2: "15" }
                polyline { points: "7 10 12 15 17 10" }
            }
        }
    }
}

#[component]
pub fn CopyIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "none", stroke: "currentColor",
            stroke_width: "2", stroke_linecap: "round", stroke_linejoin: "round",
            rect { x: "9", y: "9", width: "12", height: "12", rx: "2" }
            path { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }
        }
    }
}

#[component]
pub fn StopIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "currentColor", stroke: "none",
            rect { x: "6", y: "6", width: "12", height: "12", rx: "2" }
        }
    }
}
