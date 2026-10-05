use dioxus::prelude::*;
use quest_db::{Difficulty, NewPlaythrough};

use super::{DIFFICULTIES, difficulty_label};
use crate::state::AppState;

/// Modal form for creating a playthrough.
#[component]
pub fn NewPlaythroughModal(on_close: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let mut name = use_signal(String::new);
    let mut difficulty = use_signal(|| Difficulty::BloodAndBrokenBones);
    let mut ng_plus = use_signal(|| false);
    let mut notes = use_signal(String::new);

    let can_create = !name.read().trim().is_empty();

    let mut create = move |_: ()| {
        if name.read().trim().is_empty() {
            return;
        }
        state.create_playthrough(NewPlaythrough {
            name: name.read().trim().to_string(),
            difficulty: difficulty(),
            is_new_game_plus: ng_plus(),
            notes: Some(notes.read().trim().to_string()).filter(|n| !n.is_empty()),
        });
        on_close.call(());
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            form {
                class: "modal card",
                onclick: move |e| e.stop_propagation(),
                onsubmit: move |e| {
                    e.prevent_default();
                    create(());
                },
                h2 { "New Playthrough" }
                label { class: "field",
                    span { "Name" }
                    input {
                        class: "input",
                        autofocus: true,
                        placeholder: "e.g. Death March run",
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                label { class: "field",
                    span { "Difficulty" }
                    select {
                        class: "select",
                        onchange: move |e| {
                            if let Ok(i) = e.value().parse::<usize>() {
                                difficulty.set(DIFFICULTIES[i]);
                            }
                        },
                        for (i, d) in DIFFICULTIES.iter().enumerate() {
                            option { value: "{i}", selected: *d == difficulty(), "{difficulty_label(*d)}" }
                        }
                    }
                }
                label { class: "field field-inline",
                    input {
                        r#type: "checkbox",
                        checked: ng_plus(),
                        onchange: move |e| ng_plus.set(e.checked()),
                    }
                    span { "New Game+" }
                }
                label { class: "field",
                    span { "Notes" }
                    textarea {
                        class: "input",
                        rows: 3,
                        value: "{notes}",
                        oninput: move |e| notes.set(e.value()),
                    }
                }
                div { class: "modal-actions",
                    button { class: "btn btn-ghost", r#type: "button", onclick: move |_| on_close.call(()), "Cancel" }
                    button { class: "btn btn-primary", r#type: "submit", disabled: !can_create, "Create" }
                }
            }
        }
    }
}
