use dioxus::prelude::*;
use quest_db::Playthrough;

use super::{PlaythroughModal, difficulty_label};
use crate::state::AppState;

/// Which playthrough modal is open.
#[derive(Clone, PartialEq)]
enum Modal {
    Closed,
    New,
    Edit(Playthrough),
}

/// Top bar: title, playthrough switcher, edit and "+ New" buttons.
#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let mut modal = use_signal(|| Modal::Closed);
    let current = *state.current.read();
    let current_playthrough =
        state.playthroughs.read().iter().find(|p| Some(p.id) == current).cloned();

    rsx! {
        header { class: "header",
            h1 { class: "title", span { class: "title-mark", "W" } "Witcher 3 Quest Tracker" }
            div { class: "header-actions",
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
                            }
                        }
                    }
                }
                if let Some(p) = current_playthrough {
                    button {
                        class: "btn btn-ghost",
                        title: "Rename or delete this playthrough",
                        onclick: move |_| modal.set(Modal::Edit(p.clone())),
                        "✎ Edit"
                    }
                }
                button { class: "btn btn-ghost", onclick: move |_| modal.set(Modal::New), "+ New" }
            }
        }
        match modal() {
            Modal::Closed => rsx! {},
            Modal::New => rsx! { PlaythroughModal { on_close: move |_| modal.set(Modal::Closed) } },
            Modal::Edit(p) => rsx! {
                PlaythroughModal { existing: p, on_close: move |_| modal.set(Modal::Closed) }
            },
        }
    }
}
