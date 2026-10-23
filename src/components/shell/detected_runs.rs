use dioxus::prelude::*;

use crate::components::format_local;
use crate::components::ornament::Divider;
use crate::state::{AppState, RunChoice};
use crate::t;

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
    let tr = *state.tr.read();
    let Some(run) = state.detected.read().iter().find(|r| r.lineage_root == lineage_root).cloned() else {
        return rsx! {};
    };
    let started = format_local(tr, run.started_at);
    let default_name = t!(
        tr,
        "run.default_name",
        date = run.started_at.map(|t| t.format("%Y-%m-%d").to_string()).unwrap_or_default()
    );
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

    rsx! {
        div { class: "modal-backdrop",
            div { class: "modal panel run-prompt",
                h2 { class: "modal-title", {t!(tr, "run.title")} }
                Divider {}
                p {
                    {t!(tr, "run.summary", started = started, count = run.save_count,
                        kind = crate::save_tracker::kind_label(tr, newest.kind()),
                        newest = format_local(tr, newest.saved_at()))}
                }
                if let Some(id) = &run.game_playthrough_id {
                    p { class: "muted small", {t!(tr, "run.game_id", id = id)} }
                }
                fieldset { class: "run-option",
                    legend { {t!(tr, "run.create_legend")} }
                    label { class: "field",
                        span { {t!(tr, "playthrough.name")} }
                        input { class: "input", value: "{name}", oninput: move |e| name.set(e.value()) }
                    }
                    label { class: "toggle",
                        input { r#type: "checkbox", checked: ng_plus(), onchange: move |e| ng_plus.set(e.checked()) }
                        span { class: "toggle-track", span { class: "toggle-thumb" } }
                        span { {t!(tr, "common.ng_plus")} }
                    }
                    button {
                        class: "btn btn-gold",
                        disabled: name.read().trim().is_empty(),
                        onclick: move |_| {
                            state.resolve_run(lineage_root, RunChoice::Create {
                                name: name.read().trim().to_string(),
                                is_new_game_plus: ng_plus(),
                            });
                        },
                        {t!(tr, "run.create")}
                    }
                }
                if !unlinked.is_empty() {
                    fieldset { class: "run-option",
                        legend { {t!(tr, "run.link_legend")} }
                        select {
                            class: "select",
                            onchange: move |e| link_target.set(e.value().parse().ok()),
                            for (id, pname) in unlinked.iter() {
                                option { value: "{id}", selected: link_target() == Some(*id), "{pname}" }
                            }
                        }
                        p { class: "muted small", {t!(tr, "run.link_hint")} }
                        button {
                            class: "btn btn-ghost",
                            disabled: link_target().is_none(),
                            onclick: move |_| {
                                if let Some(id) = link_target() {
                                    state.resolve_run(lineage_root, RunChoice::Link(id));
                                }
                            },
                            {t!(tr, "run.link")}
                        }
                    }
                }
                div { class: "modal-actions",
                    if remaining > 0 {
                        span { class: "muted small", {t!(tr, "run.remaining", n = remaining)} }
                    }
                    button {
                        class: "btn btn-ghost",
                        title: t!(tr, "run.ignore_hint"),
                        onclick: move |_| state.resolve_run(lineage_root, RunChoice::Ignore),
                        {t!(tr, "run.ignore")}
                    }
                }
            }
        }
    }
}
