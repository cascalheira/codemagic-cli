//! Global keyboard shortcuts, mirroring the terminal client's bindings.
//!
//! Key events are collected in JavaScript rather than with an `onkeydown`
//! handler on the root element, for two reasons: the handler has to see keys
//! regardless of what currently holds focus, and it has to inspect
//! `document.activeElement` to know whether the user is typing (Dioxus's
//! `KeyboardData` doesn't expose the event target).

use dioxus::prelude::*;

/// An action requested by a keypress. Parsing to this enum keeps the mapping
/// testable and separate from the effects each action has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    /// Select the next build down the list.
    Next,
    /// Select the previous build up the list.
    Prev,
    Refresh,
    NewBuild,
    Settings,
    /// Open the selected build on codemagic.io.
    OpenInBrowser,
    /// Move focus to the workflow filter.
    FocusFilter,
    /// Browse app and workflow IDs.
    AppInfo,
    Help,
    /// Dismiss whatever overlay is open.
    Close,
}

/// Maps a key name produced by [`LISTENER_JS`] to its action.
///
/// `mod+` stands for Cmd on macOS and Ctrl elsewhere; the plain single-letter
/// forms are the terminal client's bindings, kept so muscle memory carries
/// over between the two.
pub fn parse(name: &str) -> Option<Shortcut> {
    Some(match name {
        "ArrowDown" | "j" => Shortcut::Next,
        "ArrowUp" | "k" => Shortcut::Prev,
        "r" | "mod+r" => Shortcut::Refresh,
        "n" | "mod+n" => Shortcut::NewBuild,
        "s" | "mod+," => Shortcut::Settings,
        "o" => Shortcut::OpenInBrowser,
        "f" | "/" | "mod+f" => Shortcut::FocusFilter,
        "i" => Shortcut::AppInfo,
        "?" => Shortcut::Help,
        "Escape" => Shortcut::Close,
        _ => return None,
    })
}

/// Installs the `keydown` listener and streams key names back to Rust.
///
/// Only whitelisted keys are forwarded, and modified ones are also
/// `preventDefault`ed — without that, Cmd/Ctrl+R would reload the webview out
/// from under the app. Plain letters are ignored while a text field has focus
/// so typing a branch name doesn't trigger navigation.
const LISTENER_JS: &str = r#"
    const PLAIN = ['ArrowUp','ArrowDown','j','k','r','n','o','s','f','i','/','?'];
    const MOD = ['mod+r','mod+n','mod+f','mod+,'];
    // Re-running this script (on a hot reload) must not stack listeners.
    if (window.__gantryKeys) window.__gantryKeys();
    const onKey = (e) => {
        if (e.altKey || e.repeat) return;
        const t = document.activeElement;
        const typing = !!t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA'
            || t.tagName === 'SELECT' || t.isContentEditable);
        let name = null;
        if (e.key === 'Escape') {
            name = 'Escape';
        } else if (e.metaKey || e.ctrlKey) {
            const candidate = 'mod+' + e.key.toLowerCase();
            if (MOD.includes(candidate)) { name = candidate; e.preventDefault(); }
        } else if (!typing && PLAIN.includes(e.key)) {
            name = e.key;
        }
        if (name) dioxus.send(name);
    };
    window.addEventListener('keydown', onKey);
    window.__gantryKeys = () => window.removeEventListener('keydown', onKey);
"#;

/// Runs `handler` for every shortcut pressed, for as long as the caller lives.
/// `Copy` because `use_future` may re-run its closure; in practice the handler
/// only captures signals, which are themselves `Copy`.
pub fn use_shortcuts(mut handler: impl FnMut(Shortcut) + Copy + 'static) {
    use_future(move || async move {
        let mut events = document::eval(LISTENER_JS);
        while let Ok(name) = events.recv::<String>().await {
            if let Some(shortcut) = parse(&name) {
                handler(shortcut);
            }
        }
    });
}

/// Moves `delta` places through `order` from `current`.
///
/// With nothing selected, both directions land on the first entry, so a single
/// arrow press from a cold start selects something. Movement clamps at the
/// ends rather than wrapping. Returns `None` when there is nowhere to go.
pub fn step(order: &[String], current: Option<&str>, delta: isize) -> Option<String> {
    if order.is_empty() {
        return None;
    }
    let Some(index) = current.and_then(|id| order.iter().position(|b| b == id)) else {
        return Some(order[0].clone());
    };
    let next = (index as isize + delta).clamp(0, order.len() as isize - 1) as usize;
    (next != index).then(|| order[next].clone())
}

/// The shortcut reference sheet, opened with `?`.
#[component]
pub fn HelpModal(open: Signal<bool>) -> Element {
    // Cmd reads as the native modifier on macOS, Ctrl everywhere else.
    let modifier = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    };
    let rows: Vec<(String, &str)> = vec![
        ("↑ ↓  ·  j k".to_string(), "Move through builds"),
        (format!("r  ·  {modifier}R"), "Refresh"),
        (format!("n  ·  {modifier}N"), "New build"),
        (
            format!("f  ·  /  ·  {modifier}F"),
            "Focus the workflow filter",
        ),
        (format!("s  ·  {modifier},"), "Settings"),
        ("o".to_string(), "Open the build on codemagic.io"),
        ("i".to_string(), "App & workflow IDs"),
        ("?".to_string(), "This list"),
        ("Esc".to_string(), "Close"),
    ];

    rsx! {
        div { class: "modal-overlay", onclick: move |_| open.set(false),
            div { class: "modal form-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { "Keyboard shortcuts" }
                    button { class: "ghost small", onclick: move |_| open.set(false), "Close" }
                }
                div { class: "form-body",
                    dl { class: "shortcut-list",
                        for (keys, what) in rows.iter() {
                            div { key: "{what}", class: "shortcut-row",
                                dt { "{keys}" }
                                dd { "{what}" }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Shortcut, parse, step};

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn plain_and_modified_forms_agree() {
        assert_eq!(parse("r"), parse("mod+r"));
        assert_eq!(parse("n"), parse("mod+n"));
        assert_eq!(parse("f"), parse("/"));
        assert_eq!(parse("s"), Some(Shortcut::Settings));
        assert_eq!(parse("mod+,"), Some(Shortcut::Settings));
    }

    #[test]
    fn unknown_keys_are_ignored() {
        assert_eq!(parse("x"), None);
        assert_eq!(parse("mod+q"), None);
        assert_eq!(parse(""), None);
    }

    #[test]
    fn stepping_from_nothing_selects_the_first_build() {
        let order = ids(&["a", "b", "c"]);
        assert_eq!(step(&order, None, 1).as_deref(), Some("a"));
        assert_eq!(step(&order, None, -1).as_deref(), Some("a"));
    }

    #[test]
    fn stepping_clamps_at_both_ends() {
        let order = ids(&["a", "b", "c"]);
        assert_eq!(step(&order, Some("b"), 1).as_deref(), Some("c"));
        assert_eq!(step(&order, Some("c"), 1), None);
        assert_eq!(step(&order, Some("a"), -1), None);
    }

    #[test]
    fn a_selection_no_longer_in_the_list_restarts_from_the_top() {
        let order = ids(&["a", "b"]);
        assert_eq!(step(&order, Some("gone"), 1).as_deref(), Some("a"));
    }

    #[test]
    fn an_empty_list_has_nowhere_to_go() {
        assert_eq!(step(&[], None, 1), None);
        assert_eq!(step(&[], Some("a"), -1), None);
    }
}
