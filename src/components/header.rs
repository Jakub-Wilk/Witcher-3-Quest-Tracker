use dioxus::prelude::*;

use super::{NewPlaythroughModal, difficulty_label};
use crate::state::AppState;

/// Top bar: title, playthrough switcher and "+ New" button.
#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let mut show_modal = use_signal(|| false);
    let current = *state.current.read();

    rsx! {
        header { class: "header",
            h1 { class: "title", span { class: "title-mark", "W" } "Witcher 3 Quest Tracker" }
            div { class: "header-actions",
                if !state.playthroughs.read().is_empty() {
                    select {
                        class: "select",
                        onchange: move |e| {
                            if let Ok(id) = e.value().parse() {
                                state.select_playthrough(id);
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
                button { class: "btn btn-ghost", onclick: move |_| show_modal.set(true), "+ New" }
            }
        }
        if show_modal() {
            NewPlaythroughModal { on_close: move |_| show_modal.set(false) }
        }
    }
}
