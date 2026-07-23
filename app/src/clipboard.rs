//! Copying to the system clipboard from inside the webview.
//!
//! `navigator.clipboard` is the modern API, but the app is served over a
//! custom scheme that some platforms treat as an insecure context, where it is
//! simply absent. Every path here therefore falls back to a hidden textarea
//! plus `document.execCommand('copy')`, which is deprecated but universally
//! available.

use dioxus::prelude::*;

/// The JS `write(text)` helper both entry points below build on.
const WRITER: &str = r#"
    const write = (t) => {
        const fallback = () => {
            const ta = document.createElement('textarea');
            ta.value = t;
            document.body.appendChild(ta);
            ta.select();
            document.execCommand('copy');
            ta.remove();
        };
        if (navigator.clipboard) navigator.clipboard.writeText(t).catch(fallback);
        else fallback();
    };
"#;

/// Copies a literal string.
pub fn copy_text(text: &str) {
    // Serialised as JSON so quotes, newlines and backslashes survive the trip
    // into the script.
    let literal = serde_json::to_string(text).unwrap_or_else(|_| "\"\"".to_string());
    document::eval(&format!("{WRITER} write({literal});"));
}

/// Copies the rendered text of the element with `id`, or nothing if it's gone.
///
/// Reading back from the DOM means the user gets exactly what they can see —
/// a filtered log copies only the lines on screen.
pub fn copy_element(id: &str) {
    let literal = serde_json::to_string(id).unwrap_or_else(|_| "\"\"".to_string());
    document::eval(&format!(
        "{WRITER} const el = document.getElementById({literal}); if (el) write(el.innerText);"
    ));
}
