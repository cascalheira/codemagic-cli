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
pub fn StopIcon() -> Element {
    rsx! {
        svg {
            class: "icon", view_box: "0 0 24 24", fill: "currentColor", stroke: "none",
            rect { x: "6", y: "6", width: "12", height: "12", rx: "2" }
        }
    }
}
