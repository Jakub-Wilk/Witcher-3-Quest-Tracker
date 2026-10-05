use dioxus::prelude::*;
use quest_db::{Quest, QuestProgress, QuestStatus};

use super::{region_label, source_label, type_label, wiki_url};
use crate::state::AppState;

/// One quest: completion checkbox, metadata, missable warning, "Mark Failed", wiki link and a
/// drawer with the journal description and notes.
#[component]
pub fn QuestCard(quest: Quest, progress: QuestProgress, cutoff_name: Option<String>) -> Element {
    let mut state = use_context::<AppState>();
    let mut notes_open = use_signal(|| false);
    let mut draft = use_signal(|| progress.notes.clone().unwrap_or_default());

    let id = quest.id;
    let status = progress.status;
    let completed = status == QuestStatus::Completed;
    let failed = status == QuestStatus::Failed;
    let has_notes = progress.notes.is_some();
    let saved_notes = progress.notes.clone().unwrap_or_default();
    let url = wiki_url(&quest.wiki_title);

    let card_class = match status {
        QuestStatus::Completed => "card quest-card completed",
        QuestStatus::Failed => "card quest-card failed",
        _ => "card quest-card",
    };

    rsx! {
        div { class: card_class,
            div { class: "quest-row",
                input {
                    class: "checkbox",
                    r#type: "checkbox",
                    checked: completed,
                    disabled: failed,
                    title: "Mark completed",
                    onchange: move |e| {
                        let next = if e.checked() { QuestStatus::Completed } else { QuestStatus::NotStarted };
                        state.set_status(id, next);
                    },
                }
                div { class: "quest-main",
                    div { class: "quest-name",
                        "{quest.display_name()}"
                        if quest.is_unmarked { span { class: "badge badge-muted", "Unmarked" } }
                    }
                    if quest.localized_name.is_some() {
                        div { class: "quest-original-name", "{quest.name}" }
                    }
                    div { class: "quest-meta",
                        span { "{region_label(quest.region)}" }
                        if let Some(level) = quest.recommended_level {
                            span { "Lvl {level}" }
                        }
                        span { "{type_label(quest.quest_type)}" }
                        span { class: "badge badge-source", "{source_label(quest.source)}" }
                    }
                    if !completed && !failed {
                        if let Some(cutoff) = &cutoff_name {
                            div { class: "quest-warning", "⚠ Cutoff: {cutoff}" }
                        }
                        if let Some(notes) = &quest.important_notes {
                            div { class: "quest-important", title: "{notes}", "❗ {notes}" }
                        }
                    }
                }
                div { class: "quest-actions",
                    match status {
                        QuestStatus::Completed => rsx! { span { class: "badge badge-done", "Completed" } },
                        QuestStatus::Failed => rsx! { span { class: "badge badge-failed", "Failed" } },
                        _ => rsx! {},
                    }
                    if !completed {
                        button {
                            class: "btn btn-fail",
                            onclick: move |_| {
                                let next = if failed { QuestStatus::NotStarted } else { QuestStatus::Failed };
                                state.set_status(id, next);
                            },
                            if failed { "Clear Failed" } else { "Mark Failed" }
                        }
                    }
                    button {
                        class: "btn btn-icon",
                        title: "Open on the Witcher wiki",
                        onclick: move |_| {
                            if let Err(e) = open::that(&url) {
                                state.error.set(Some(format!("Could not open the browser: {e}")));
                            }
                        },
                        "Wiki ↗"
                    }
                    button {
                        class: if has_notes { "btn btn-note has-notes" } else { "btn btn-note" },
                        title: "Details and notes",
                        onclick: move |_| notes_open.toggle(),
                        "📝"
                    }
                }
            }
            if notes_open() {
                div { class: "notes-drawer",
                    if let Some(desc) = &quest.description {
                        p { class: "quest-desc", "{desc}" }
                    }
                    textarea {
                        class: "input notes",
                        rows: 3,
                        placeholder: "Your notes for this quest…",
                        value: "{draft}",
                        oninput: move |e| draft.set(e.value()),
                        onblur: move |_| {
                            if draft() != saved_notes {
                                state.set_notes(id, draft());
                            }
                        },
                    }
                }
            }
        }
    }
}
