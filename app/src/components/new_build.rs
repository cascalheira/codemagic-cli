//! "New build" wizard: pick app → workflow → branch, then trigger a build.

use dioxus::prelude::*;

use crate::state::AppState;

#[component]
pub fn NewBuildModal(open: Signal<bool>, on_started: EventHandler<String>) -> Element {
    let state = use_context::<AppState>();

    let mut app_id = use_signal(|| Option::<String>::None);
    let mut workflow_id = use_signal(|| Option::<String>::None);
    let mut branch = use_signal(String::new);
    let mut submitting = use_signal(|| false);
    let mut error = use_signal(|| Option::<String>::None);

    // All apps the user can access.
    let apps = use_resource(move || {
        let client = state.client();
        async move { client.get_apps().await }
    });

    // Full detail for the selected app (workflows + branch list).
    let app_detail = use_resource(move || {
        let client = state.client();
        let id = app_id.read().clone();
        async move {
            match id {
                Some(id) => Some(client.get_app(&id).await),
                None => None,
            }
        }
    });

    // Workflows for the selected app, sorted by display name.
    let workflows: Vec<(String, String)> = match &*app_detail.read() {
        Some(Some(Ok(app))) => {
            let mut v: Vec<(String, String)> = app
                .workflows
                .iter()
                .map(|(id, w)| (id.clone(), w.name.clone()))
                .collect();
            v.sort_by_key(|(_, name)| name.to_lowercase());
            v
        }
        _ => Vec::new(),
    };
    let branches: Vec<String> = match &*app_detail.read() {
        Some(Some(Ok(app))) => app.branches.clone(),
        _ => Vec::new(),
    };

    let can_submit = app_id.read().is_some()
        && workflow_id.read().is_some()
        && !branch.read().trim().is_empty()
        && !submitting();

    let start = move |_| {
        let (Some(aid), Some(wid)) = (app_id.read().clone(), workflow_id.read().clone()) else {
            return;
        };
        let br = branch.read().trim().to_string();
        if br.is_empty() {
            return;
        }
        submitting.set(true);
        error.set(None);
        let client = state.client();
        spawn(async move {
            match client.start_build(&aid, &wid, &br).await {
                Ok(build_id) => {
                    submitting.set(false);
                    on_started.call(build_id);
                }
                Err(e) => {
                    submitting.set(false);
                    error.set(Some(e.to_string()));
                }
            }
        });
    };

    rsx! {
        div { class: "modal-overlay", onclick: move |_| open.set(false),
            div { class: "modal form-modal", onclick: move |e| e.stop_propagation(),
                div { class: "modal-head",
                    h3 { "New build" }
                    button { class: "ghost small", onclick: move |_| open.set(false), "Close" }
                }
                div { class: "form-body",
                    // ── App ──
                    label { "App" }
                    match &*apps.read() {
                        None => rsx! { p { class: "muted", "Loading apps…" } },
                        Some(Err(e)) => rsx! { p { class: "error", "Couldn't load apps: {e}" } },
                        Some(Ok(list)) => {
                            let list = list.clone();
                            rsx! {
                                select {
                                    value: "{app_id.read().clone().unwrap_or_default()}",
                                    onchange: move |e| {
                                        let v = e.value();
                                        app_id.set(if v.is_empty() { None } else { Some(v) });
                                        workflow_id.set(None);
                                        branch.set(String::new());
                                    },
                                    option { value: "", "Select an app…" }
                                    for app in list.iter() {
                                        option { value: "{app.id}", "{app.name}" }
                                    }
                                }
                            }
                        }
                    }

                    // ── Workflow ──
                    label { "Workflow" }
                    select {
                        disabled: app_id.read().is_none() || workflows.is_empty(),
                        value: "{workflow_id.read().clone().unwrap_or_default()}",
                        onchange: move |e| {
                            let v = e.value();
                            workflow_id.set(if v.is_empty() { None } else { Some(v) });
                        },
                        option { value: "", "Select a workflow…" }
                        for (id, name) in workflows.iter() {
                            option { value: "{id}", "{name}" }
                        }
                    }

                    // ── Branch ──
                    label { "Branch" }
                    input {
                        list: "branch-options",
                        placeholder: "e.g. main",
                        value: "{branch}",
                        oninput: move |e| branch.set(e.value()),
                    }
                    datalist { id: "branch-options",
                        for b in branches.iter() {
                            option { value: "{b}" }
                        }
                    }

                    if let Some(msg) = &*error.read() {
                        p { class: "error", "{msg}" }
                    }
                }
                div { class: "modal-foot",
                    button { class: "ghost", onclick: move |_| open.set(false), "Cancel" }
                    button {
                        class: "primary",
                        disabled: !can_submit,
                        onclick: start,
                        if submitting() { "Starting…" } else { "Start build" }
                    }
                }
            }
        }
    }
}
