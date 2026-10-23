use companion_db::{NewPlaythrough, Playthrough, PlaythroughUpdate};
use dioxus::prelude::*;

use crate::components::format_local;
use crate::components::ornament::Divider;
use crate::state::AppState;
use crate::t;

/// Modal form for creating a playthrough, or editing `existing` (with unlink and delete).
#[component]
pub fn PlaythroughModal(existing: Option<Playthrough>, on_close: EventHandler<()>) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let initial = existing.clone();
    let mut name = use_signal(|| initial.as_ref().map(|p| p.name.clone()).unwrap_or_default());
    let mut ng_plus = use_signal(|| initial.as_ref().is_some_and(|p| p.is_new_game_plus));
    let mut notes = use_signal(|| initial.as_ref().and_then(|p| p.notes.clone()).unwrap_or_default());
    let mut confirm_delete = use_signal(|| false);
    let mut confirm_unlink = use_signal(|| false);
    let link = existing.as_ref().and_then(|p| p.link.clone());
    let head = existing.as_ref().and_then(|p| p.head.clone());

    let editing_id = existing.as_ref().map(|p| p.id);
    let can_save = !name.read().trim().is_empty();

    let mut save = move |_: ()| {
        let trimmed_name = name.read().trim().to_string();
        if trimmed_name.is_empty() {
            return;
        }
        let trimmed_notes = Some(notes.read().trim().to_string()).filter(|n| !n.is_empty());
        match editing_id {
            Some(id) => state.update_playthrough(
                id,
                PlaythroughUpdate {
                    name: Some(trimmed_name),
                    is_new_game_plus: Some(ng_plus()),
                    notes: Some(trimmed_notes),
                },
            ),
            None => drop(state.create_playthrough(NewPlaythrough {
                name: trimmed_name,
                is_new_game_plus: ng_plus(),
                notes: trimmed_notes,
                link: None,
            })),
        }
        on_close.call(());
    };

    rsx! {
        div { class: "modal-backdrop", onclick: move |_| on_close.call(()),
            form {
                class: "modal panel",
                onclick: move |e| e.stop_propagation(),
                onsubmit: move |e| {
                    e.prevent_default();
                    save(());
                },
                h2 { class: "modal-title",
                    if editing_id.is_some() { {t!(tr, "playthrough.edit_title")} } else { {t!(tr, "playthrough.new_title")} }
                }
                Divider {}
                label { class: "field",
                    span { {t!(tr, "playthrough.name")} }
                    input {
                        class: "input",
                        autofocus: true,
                        placeholder: t!(tr, "playthrough.name_placeholder"),
                        value: "{name}",
                        oninput: move |e| name.set(e.value()),
                    }
                }
                label { class: "toggle",
                    input { r#type: "checkbox", checked: ng_plus(), onchange: move |e| ng_plus.set(e.checked()) }
                    span { class: "toggle-track", span { class: "toggle-thumb" } }
                    span { {t!(tr, "common.ng_plus")} }
                    span { class: "muted small", {t!(tr, "playthrough.ng_plus_hint")} }
                }
                label { class: "field",
                    span { {t!(tr, "playthrough.notes")} }
                    textarea { class: "input", rows: 3, value: "{notes}", oninput: move |e| notes.set(e.value()) }
                }
                if let (Some(id), Some(link)) = (editing_id, link) {
                    div { class: "field link-info",
                        span { {t!(tr, "playthrough.tracking")} }
                        p { class: "muted small",
                            {t!(tr, "playthrough.linked", started = format_local(tr, link.started_at))}
                            if let Some(game_id) = &link.game_playthrough_id { " " {t!(tr, "run.game_id", id = game_id)} }
                            if let Some(head) = &head {
                                " " {t!(tr, "playthrough.last_save", file = head.file, time = format_local(tr, head.saved_at))}
                            }
                        }
                        button {
                            class: "btn btn-ghost",
                            r#type: "button",
                            onclick: move |_| {
                                if confirm_unlink() {
                                    state.unlink_playthrough(id);
                                    on_close.call(());
                                } else {
                                    confirm_unlink.set(true);
                                }
                            },
                            if confirm_unlink() { {t!(tr, "playthrough.unlink_confirm")} } else { {t!(tr, "playthrough.unlink")} }
                        }
                    }
                }
                div { class: "modal-actions",
                    if let Some(id) = editing_id {
                        button {
                            class: "btn btn-ghost danger modal-delete",
                            r#type: "button",
                            onclick: move |_| {
                                if confirm_delete() {
                                    state.delete_playthrough(id);
                                    on_close.call(());
                                } else {
                                    confirm_delete.set(true);
                                }
                            },
                            if confirm_delete() { {t!(tr, "playthrough.delete_confirm")} } else { {t!(tr, "common.delete")} }
                        }
                    }
                    button { class: "btn btn-ghost", r#type: "button", onclick: move |_| on_close.call(()), {t!(tr, "common.cancel")} }
                    button { class: "btn btn-gold", r#type: "submit", disabled: !can_save,
                        if editing_id.is_some() { {t!(tr, "common.save")} } else { {t!(tr, "common.create")} }
                    }
                }
            }
        }
    }
}
