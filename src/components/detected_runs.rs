use dioxus::prelude::*;

use super::format_local;
use crate::state::{AppState, RunChoice};

/// Asks what to do with the first in-game run found in the save folder that no playthrough
/// tracks yet: create a playthrough for it, link it to an existing untracked one, or ignore it.
#[component]
pub fn DetectedRuns() -> Element {
    let state = use_context::<AppState>();
    let detected = state.detected.read();
    let Some(run) = detected.first().cloned() else { return rsx! {} };
    let remaining = detected.len() - 1;
    rsx! {
        RunPrompt { key: "{run.lineage_root}", lineage_root: run.lineage_root, remaining }
    }
}

#[component]
fn RunPrompt(lineage_root: i64, remaining: usize) -> Element {
    let mut state = use_context::<AppState>();
    let Some(run) = state.detected.read().iter().find(|r| r.lineage_root == lineage_root).cloned() else {
        return rsx! {};
    };
    let started = format_local(run.started_at);
    let default_name = format!("Playthrough started {}", run.started_at.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default());
    let mut name = use_signal(|| default_name);
    let mut ng_plus = use_signal(|| false);
    let unlinked: Vec<(i64, String)> = state
        .playthroughs
        .read()
        .iter()
        .filter(|p| p.link.is_none())
        .map(|p| (p.id, p.name.clone()))
        .collect();
    let mut link_target = use_signal(|| unlinked.first().map(|p| p.0));
    let newest = &run.newest;
    let plural = if run.save_count == 1 { "" } else { "s" };
    let newest_kind = newest.kind().to_lowercase();
    let newest_at = format_local(newest.saved_at());

    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal card run-prompt",
                h2 { "New playthrough found in your saves" }
                p {
                    "A game started "
                    strong { "{started}" }
                    " has {run.save_count} save{plural}; the newest is a {newest_kind} from {newest_at}."
                }
                if let Some(id) = &run.game_playthrough_id {
                    p { class: "muted small", "Game playthrough ID: {id}" }
                }
                fieldset { class: "run-option",
                    legend { "Track it as a new playthrough" }
                    label { class: "field",
                        span { "Name" }
                        input { class: "input", value: "{name}", oninput: move |e| name.set(e.value()) }
                    }
                    label { class: "field field-inline",
                        input { r#type: "checkbox", checked: ng_plus(), onchange: move |e| ng_plus.set(e.checked()) }
                        span { "New Game+" }
                    }
                    button {
                        class: "btn btn-primary",
                        disabled: name.read().trim().is_empty(),
                        onclick: move |_| {
                            state.resolve_run(lineage_root, RunChoice::Create {
                                name: name.read().trim().to_string(),
                                is_new_game_plus: ng_plus(),
                            });
                        },
                        "Create playthrough"
                    }
                }
                if !unlinked.is_empty() {
                    fieldset { class: "run-option",
                        legend { "Or link it to a playthrough you already track by hand" }
                        select {
                            class: "select",
                            onchange: move |e| link_target.set(e.value().parse().ok()),
                            for (id, pname) in unlinked.iter() {
                                option { value: "{id}", selected: link_target() == Some(*id), "{pname}" }
                            }
                        }
                        p { class: "muted small", "Quest statuses will come from your saves from now on; your notes and hand-ticked unmarked quests are kept." }
                        button {
                            class: "btn btn-ghost",
                            disabled: link_target().is_none(),
                            onclick: move |_| {
                                if let Some(id) = link_target() {
                                    state.resolve_run(lineage_root, RunChoice::Link(id));
                                }
                            },
                            "Link"
                        }
                    }
                }
                div { class: "modal-actions",
                    if remaining > 0 {
                        span { class: "muted small", "{remaining} more to review" }
                    }
                    button {
                        class: "btn btn-ghost",
                        title: "Don't track this run (can be undone in Settings)",
                        onclick: move |_| state.resolve_run(lineage_root, RunChoice::Ignore),
                        "Ignore"
                    }
                }
            }
        }
    }
}
