use companion_db::{Quest, QuestProgress, QuestStatus};
use dioxus::prelude::*;

use crate::components::ornament::{Glyph, Icon};
use crate::components::{region_label, source_label, status_label, type_label, wiki_url};
use crate::state::AppState;
use crate::t;

/// The manual status to store so the quest shows `desired`: none when the save already says so.
fn manual_for(desired: QuestStatus, save: Option<QuestStatus>) -> Option<QuestStatus> {
    (save.unwrap_or(QuestStatus::NotStarted) != desired).then_some(desired)
}

/// The manual status to store when the user clears `status`: follow the save again, unless the
/// save itself has that status.
fn manual_for_cleared(status: QuestStatus, save: Option<QuestStatus>) -> Option<QuestStatus> {
    (save == Some(status)).then_some(QuestStatus::NotStarted)
}

/// One quest: completion seal, metadata, missable warning, "Mark failed", wiki link and a
/// drawer with the journal description and notes. With `save_synced` the status comes from the
/// player's saves alone: there is no seal or "Mark failed", only the status.
#[component]
pub fn QuestCard(
    /// Position in the list, for the staggered entrance.
    index: usize,
    quest: Quest,
    progress: QuestProgress,
    cutoff_name: Option<String>,
    /// The cutoff quest is in progress: this quest is about to be locked out.
    cutoff_active: bool,
    save_synced: bool,
) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
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

    let state_class = match status {
        QuestStatus::Completed => "is-completed",
        QuestStatus::Failed => "is-failed",
        QuestStatus::InProgress if cutoff_active => "is-active is-cutoff",
        QuestStatus::InProgress => "is-active",
        _ if cutoff_active => "is-cutoff",
        _ => "",
    };

    rsx! {
        article { class: "quest-card {state_class}", style: "--i: {index.min(16)}",
            div { class: "quest-row",
                if save_synced {
                    span { class: "quest-seal is-readonly", title: t!(tr, "quest.from_save"),
                        if completed { Glyph { icon: Icon::Check } }
                        if failed { Glyph { icon: Icon::Close } }
                    }
                } else {
                    button {
                        class: "quest-seal",
                        disabled: failed,
                        title: if completed { t!(tr, "quest.unmark_completed") } else { t!(tr, "quest.mark_completed") },
                        onclick: move |_| {
                            let manual = if completed {
                                manual_for_cleared(QuestStatus::Completed, save_status)
                            } else {
                                manual_for(QuestStatus::Completed, save_status)
                            };
                            state.set_status(id, manual);
                        },
                        if completed { Glyph { icon: Icon::Check } }
                        if failed { Glyph { icon: Icon::Close } }
                    }
                }
                div { class: "quest-main",
                    div { class: "quest-name",
                        span { "{quest.title}" }
                        if quest.is_unmarked { span { class: "badge badge-muted", {t!(tr, "quest.unmarked")} } }
                        if !quest.is_trackable() {
                            span { class: "badge badge-muted", title: t!(tr, "quest.manual_hint"), {t!(tr, "quest.manual")} }
                        }
                    }
                    div { class: "quest-meta",
                        span { {region_label(tr, quest.region)} }
                        if let Some(level) = quest.recommended_level {
                            span { class: "num", {t!(tr, "quest.level", n = level)} }
                        }
                        span { {type_label(tr, quest.quest_type)} }
                        span { class: "badge badge-source", {source_label(tr, quest.source)} }
                    }
                    if !completed && !failed {
                        if let Some(cutoff) = &cutoff_name {
                            div { class: "quest-warning",
                                Glyph { icon: Icon::Warning }
                                span { {t!(tr, "quest.cutoff", name = cutoff)} }
                            }
                        }
                        if let Some(notes) = &quest.important_notes {
                            div { class: "quest-important",
                                Glyph { icon: Icon::Pin }
                                span { "{notes}" }
                            }
                        }
                    }
                    if progress.overrides_save() {
                        div { class: "quest-override",
                            {t!(tr, "quest.override", status = status_label(tr, save_status.unwrap_or(QuestStatus::NotStarted)))}
                            " "
                            button { class: "btn-link", title: t!(tr, "quest.follow_save_hint"), onclick: move |_| state.set_status(id, None),
                                {t!(tr, "quest.follow_save")}
                            }
                        }
                    }
                }
                div { class: "quest-actions",
                    match status {
                        QuestStatus::Completed => rsx! { span { class: "badge badge-done", {status_label(tr, status)} } },
                        QuestStatus::Failed => rsx! { span { class: "badge badge-failed", {status_label(tr, status)} } },
                        QuestStatus::InProgress => rsx! { span { class: "badge badge-active", {status_label(tr, status)} } },
                        _ => rsx! {},
                    }
                    if !completed && !save_synced {
                        button {
                            class: "btn btn-ghost danger small",
                            onclick: move |_| {
                                let manual = if failed {
                                    manual_for_cleared(QuestStatus::Failed, save_status)
                                } else {
                                    manual_for(QuestStatus::Failed, save_status)
                                };
                                state.set_status(id, manual);
                            },
                            if failed { {t!(tr, "quest.clear_failed")} } else { {t!(tr, "quest.mark_failed")} }
                        }
                    }
                    if let Some(url) = url {
                        button {
                            class: "btn-icon",
                            title: t!(tr, "quest.open_wiki"),
                            onclick: move |_| {
                                if let Err(e) = open::that(&url) {
                                    state.error.set(Some(t!(tr, "common.browser_failed", error = e)));
                                }
                            },
                            Glyph { icon: Icon::External }
                        }
                    }
                    button {
                        class: if has_notes { "btn-icon has-notes" } else { "btn-icon" },
                        title: t!(tr, "quest.notes"),
                        onclick: move |_| notes_open.toggle(),
                        Glyph { icon: Icon::Quill }
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
                        placeholder: t!(tr, "quest.notes_placeholder"),
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
