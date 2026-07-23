//! First-run screen: collect and validate a Codemagic API token.

use gantry_core::ApiClient;
use dioxus::prelude::*;

use crate::state::AppState;

#[derive(Clone, PartialEq)]
enum Status {
    Idle,
    Checking,
    Error(String),
}

#[component]
pub fn Onboarding() -> Element {
    let mut state = use_context::<AppState>();
    let mut input = use_signal(String::new);
    let mut status = use_signal(|| Status::Idle);

    let submit = use_callback(move |_: ()| {
        let candidate = input.read().trim().to_string();
        if candidate.is_empty() {
            status.set(Status::Error("Enter a token to continue.".into()));
            return;
        }
        status.set(Status::Checking);
        spawn(async move {
            let client = ApiClient::new(candidate.clone());
            match client.validate_token().await {
                Ok(true) => {
                    state.save_token(candidate);
                }
                Ok(false) => {
                    status.set(Status::Error("That token was rejected by Codemagic.".into()));
                }
                Err(e) => {
                    status.set(Status::Error(format!("Network error: {e}")));
                }
            }
        });
    });

    let checking = matches!(*status.read(), Status::Checking);

    // Owned snapshots: a read guard held live inside `rsx!` stays borrowed
    // across the reactive flush, which deadlocks if a spawned task writes the
    // same signal.
    let status_now = status.read().clone();

    rsx! {
        section { class: "onboarding",
            div { class: "card",
                h1 { "Gantry" }
                p { class: "muted",
                    "Paste your API token to get started. Find it under "
                    "Codemagic → Teams → Personal Account → Integrations → API tokens."
                }
                input {
                    class: "token-input",
                    r#type: "password",
                    placeholder: "API token",
                    value: "{input}",
                    disabled: checking,
                    oninput: move |e| input.set(e.value()),
                    onkeydown: move |e| {
                        if e.key() == Key::Enter { submit.call(()); }
                    },
                }
                if let Status::Error(msg) = &status_now {
                    p { class: "error", "{msg}" }
                }
                button {
                    class: "primary",
                    disabled: checking,
                    onclick: move |_| submit.call(()),
                    if checking { "Validating…" } else { "Continue" }
                }
            }
        }
    }
}
