use std::collections::HashMap;

use dioxus::prelude::*;
use quest_scraper::ScrapeProgress;

use super::{PlaythroughModal, QuestCard};
use crate::state::{AppState, SyncStatus};
use crate::sync_service::SyncPhase;

/// Center panel: search, sync, and the filtered list of quest cards.
#[component]
pub fn QuestList() -> Element {
    let mut state = use_context::<AppState>();
    let mut show_modal = use_signal(|| false);
    let view = state.view.read().clone();
    let sync = state.sync.read().clone();
    let syncing = matches!(sync, SyncStatus::Running(_));
    let current = *state.current.read();
    let linked = state.playthroughs.read().iter().any(|p| Some(p.id) == current && p.link.is_some());
    let rows = state.rows.read();

    let names: HashMap<i64, &str> = rows.iter().map(|(q, _)| (q.id, q.title.as_str())).collect();
    let visible = view.apply(&rows);
    let visible_done =
        visible.iter().filter(|(_, p)| p.status() == quest_db::QuestStatus::Completed).count();
    let visible_pct = if visible.is_empty() {
        0.0
    } else {
        visible_done as f64 * 100.0 / visible.len() as f64
    };

    rsx! {
        main { class: "pane quest-pane",
            div { class: "toolbar",
                input {
                    class: "input search",
                    r#type: "search",
                    placeholder: "Search quests…",
                    value: "{view.search}",
                    oninput: move |e| state.view.write().search = e.value(),
                }
                button {
                    class: "btn btn-primary",
                    disabled: syncing,
                    title: "Read the quest list from the game files, then add wiki data",
                    onclick: move |_| state.start_sync(),
                    if syncing {
                        span { class: "spinner" }
                        "Syncing…"
                    } else {
                        "Sync Quests ⟳"
                    }
                }
            }
            match &sync {
                SyncStatus::Running(phase) => rsx! {
                    div { class: "sync-status", {phase_text(*phase)} }
                },
                SyncStatus::Done(msg) => rsx! {
                    div { class: "sync-status sync-ok", "{msg}" }
                },
                SyncStatus::Failed(msg) => rsx! {
                    div { class: "sync-status sync-failed", "Sync failed — nothing was changed. {msg}" }
                },
                SyncStatus::Idle => rsx! {},
            }

            div { class: "quest-scroll",
                if current.is_none() {
                    div { class: "empty",
                        h2 { "No playthrough yet" }
                        p {
                            "Playthroughs are created automatically when a save of a new game is found, "
                            "or you can create one yourself."
                        }
                        button { class: "btn btn-primary", onclick: move |_| show_modal.set(true), "+ New Playthrough" }
                    }
                } else if rows.is_empty() {
                    div { class: "empty",
                        h2 { "No quests yet" }
                        p { "Click " strong { "Sync Quests" } " to read the quest list from your game files." }
                    }
                } else if visible.is_empty() {
                    div { class: "empty", p { "No quests match the current filters." } }
                } else {
                    for (quest, progress) in visible.iter() {
                        QuestCard {
                            key: "{current:?}-{quest.id}",
                            quest: quest.clone(),
                            progress: progress.clone(),
                            cutoff_name: quest.cutoff_quest_id.and_then(|id| names.get(&id)).map(|n| n.to_string()),
                            save_synced: linked && quest.is_trackable(),
                        }
                    }
                }
            }
            if !visible.is_empty() {
                div { class: "list-footer",
                    "Showing {visible.len()} quests · {visible_done} completed ({visible_pct:.1}%)"
                }
            }
        }
        if show_modal() {
            PlaythroughModal { on_close: move |_| show_modal.set(false) }
        }
    }
}

pub fn phase_text(phase: SyncPhase) -> String {
    match phase {
        SyncPhase::ReadingGame => "Reading quests from the game files…".into(),
        SyncPhase::Wiki(ScrapeProgress::ListingCategories) => "Listing wiki categories…".into(),
        SyncPhase::Wiki(ScrapeProgress::FetchingPages { done, total }) => {
            format!("Fetching wiki pages: {done}/{total}")
        }
        SyncPhase::Wiki(ScrapeProgress::Translating { done, total }) => {
            format!("Fetching wiki translations: {done}/{total}")
        }
        SyncPhase::Merging => "Saving…".into(),
    }
}
