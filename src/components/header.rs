use dioxus::prelude::*;
use quest_db::Playthrough;

use super::{PlaythroughModal, SettingsModal, difficulty_label};
use crate::settings::language_label;
use crate::state::AppState;

/// Which modal is open.
#[derive(Clone, PartialEq)]
enum Modal {
    Closed,
    New,
    Edit(Playthrough),
    Settings,
}

/// Top bar: title, save tracking status, quest language, playthrough switcher, edit, "+ New"
/// and settings.
#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let mut modal = use_signal(|| Modal::Closed);
    let current = *state.current.read();
    let current_playthrough =
        state.playthroughs.read().iter().find(|p| Some(p.id) == current).cloned();
    let language = state.settings.read().ui.language.clone();
    let languages = state.languages.read().clone();
    let tracker = state.tracker.read().clone();
    let watching = state.settings.read().tracking.watch_saves && state.settings.read().valid_save_dir().is_some();

    rsx! {
        header { class: "header",
            h1 { class: "title", span { class: "title-mark", "W" } "Witcher 3 Quest Tracker" }
            div { class: "header-actions",
                div {
                    class: "tracker-status",
                    title: if watching { "Watching the save folder for new saves" } else { "Save watching is off (see Settings)" },
                    if tracker.scanning {
                        span { class: "spinner" }
                        "Reading saves…"
                    } else if let Some(last) = &tracker.last {
                        span { class: if watching { "dot dot-on" } else { "dot" } }
                        "{last}"
                    } else {
                        span { class: if watching { "dot dot-on" } else { "dot" } }
                        if watching { "Watching saves" } else { "Not watching saves" }
                    }
                    button {
                        class: "btn btn-icon",
                        title: "Re-read all saves",
                        disabled: tracker.scanning,
                        onclick: move |_| state.rescan_saves(false),
                        "⟳"
                    }
                }
                if languages.len() > 1 {
                    select {
                        class: "select",
                        title: "Language of quest titles and descriptions",
                        onchange: move |e| state.set_language(e.value()),
                        for code in languages.iter() {
                            option { value: "{code}", selected: *code == language, "{language_label(code)}" }
                        }
                    }
                }
                if !state.playthroughs.read().is_empty() {
                    select {
                        class: "select",
                        onchange: move |e| {
                            if let Ok(id) = e.value().parse() {
                                state.select_playthrough(Some(id));
                            }
                        },
                        for p in state.playthroughs.read().iter() {
                            option {
                                value: "{p.id}",
                                selected: current == Some(p.id),
                                "{p.name} — {difficulty_label(p.difficulty)}"
                                if p.is_new_game_plus { " (NG+)" }
                                if p.link.is_some() { " ⛓" }
                            }
                        }
                    }
                }
                if let Some(p) = current_playthrough {
                    button {
                        class: "btn btn-ghost",
                        title: "Rename, unlink or delete this playthrough",
                        onclick: move |_| modal.set(Modal::Edit(p.clone())),
                        "✎ Edit"
                    }
                }
                button { class: "btn btn-ghost", onclick: move |_| modal.set(Modal::New), "+ New" }
                button {
                    class: "btn btn-icon",
                    title: "Settings",
                    onclick: move |_| modal.set(Modal::Settings),
                    "⚙"
                }
            }
        }
        match modal() {
            Modal::Closed => rsx! {},
            Modal::New => rsx! { PlaythroughModal { on_close: move |_| modal.set(Modal::Closed) } },
            Modal::Edit(p) => rsx! {
                PlaythroughModal { existing: p, on_close: move |_| modal.set(Modal::Closed) }
            },
            Modal::Settings => rsx! { SettingsModal { on_close: move |_| modal.set(Modal::Closed) } },
        }
    }
}
