use dioxus::prelude::*;
use quest_db::{Quest, QuestProgress, QuestStatus};

use super::{region_label, source_label, type_label, wiki_url};
use crate::state::AppState;

/// The manual status to store so the quest shows `desired`: none when the save already says so.
fn manual_for(desired: QuestStatus, save: Option<QuestStatus>) -> Option<QuestStatus> {
    (save.unwrap_or(QuestStatus::NotStarted) != desired).then_some(desired)
}

/// The manual status to store when the user clears `status`: follow the save again, unless the
/// save itself has that status.
fn manual_for_cleared(status: QuestStatus, save: Option<QuestStatus>) -> Option<QuestStatus> {
    (save == Some(status)).then_some(QuestStatus::NotStarted)
}

fn status_label(status: QuestStatus) -> &'static str {
    match status {
        QuestStatus::NotStarted => "not started",
        QuestStatus::InProgress => "in progress",
        QuestStatus::Completed => "completed",
        QuestStatus::Failed => "failed",
    }
}

/// One quest: completion checkbox, metadata, missable warning, "Mark Failed", wiki link and a
/// drawer with the journal description and notes. With `save_synced` the status comes from the
/// player's saves alone: there is no checkbox or "Mark Failed", only the status label.
#[component]
pub fn QuestCard(
    quest: Quest,
    progress: QuestProgress,
    cutoff_name: Option<String>,
    save_synced: bool,
) -> Element {
    let mut state = use_context::<AppState>();
    let mut notes_open = use_signal(|| false);
    let mut draft = use_signal(|| progress.notes.clone().unwrap_or_default());

    let id = quest.id;
    let status = progress.status();
    let save_status = progress.save_status;
    let completed = status == QuestStatus::Completed;
    let failed = status == QuestStatus::Failed;
    let has_notes = progress.notes.is_some();
    let saved_notes = progress.notes.clone().unwrap_or_default();
    let url = quest.wiki_title.as_deref().map(wiki_url);

    let card_class = match status {
        QuestStatus::Completed => "card quest-card completed",
        QuestStatus::Failed => "card quest-card failed",
        _ => "card quest-card",
    };

    rsx! {
        div { class: card_class,
            div { class: "quest-row",
                if !save_synced {
                    input {
                        class: "checkbox",
                        r#type: "checkbox",
                        checked: completed,
                        disabled: failed,
                        title: "Mark completed",
                        onchange: move |e| {
                            let manual = if e.checked() {
                                manual_for(QuestStatus::Completed, save_status)
                            } else {
                                manual_for_cleared(QuestStatus::Completed, save_status)
                            };
                            state.set_status(id, manual);
                        },
                    }
                }
                div { class: "quest-main",
                    div { class: "quest-name",
                        "{quest.title}"
                        if quest.is_unmarked { span { class: "badge badge-muted", "Unmarked" } }
                        if !quest.is_trackable() {
                            span {
                                class: "badge badge-muted",
                                title: "Not in the game journal, so saves cannot report it: tick it off yourself",
                                "Manual"
                            }
                        }
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
                    if progress.overrides_save() {
                        div { class: "quest-override",
                            "Set by you · your save says {status_label(save_status.unwrap_or(QuestStatus::NotStarted))} "
                            button {
                                class: "btn btn-link",
                                title: "Show the status from your save again",
                                onclick: move |_| state.set_status(id, None),
                                "Follow save"
                            }
                        }
                    }
                }
                div { class: "quest-actions",
                    match status {
                        QuestStatus::Completed => rsx! { span { class: "badge badge-done", "Completed" } },
                        QuestStatus::Failed => rsx! { span { class: "badge badge-failed", "Failed" } },
                        QuestStatus::InProgress => rsx! { span { class: "badge badge-active", "In progress" } },
                        _ => rsx! {},
                    }
                    if !completed && !save_synced {
                        button {
                            class: "btn btn-fail",
                            onclick: move |_| {
                                let manual = if failed {
                                    manual_for_cleared(QuestStatus::Failed, save_status)
                                } else {
                                    manual_for(QuestStatus::Failed, save_status)
                                };
                                state.set_status(id, manual);
                            },
                            if failed { "Clear Failed" } else { "Mark Failed" }
                        }
                    }
                    if let Some(url) = url {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_status_only_when_it_differs_from_the_save() {
        use QuestStatus::*;
        assert_eq!(manual_for(Completed, Some(Completed)), None);
        assert_eq!(manual_for(Completed, Some(InProgress)), Some(Completed));
        assert_eq!(manual_for(NotStarted, None), None);
        assert_eq!(manual_for(NotStarted, Some(Completed)), Some(NotStarted));
        assert_eq!(manual_for(Failed, None), Some(Failed));
    }

    #[test]
    fn clearing_a_status_follows_the_save_unless_the_save_has_it() {
        use QuestStatus::*;
        assert_eq!(manual_for_cleared(Completed, Some(InProgress)), None);
        assert_eq!(manual_for_cleared(Completed, None), None);
        assert_eq!(manual_for_cleared(Completed, Some(Completed)), Some(NotStarted));
        assert_eq!(manual_for_cleared(Failed, Some(Failed)), Some(NotStarted));
    }
}
