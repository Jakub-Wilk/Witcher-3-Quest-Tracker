use std::collections::HashMap;

use dioxus::prelude::*;
use quest_db::QuestStatus;

use super::{NewPlaythroughModal, QuestCard};
use crate::state::{AppState, SyncStatus};
use crate::sync_service::{fetch_remote, merge};

/// Center panel: search, sync, and the filtered list of quest cards.
#[component]
pub fn QuestList() -> Element {
    let mut state = use_context::<AppState>();
    let mut show_modal = use_signal(|| false);
    let filter = state.filter.read().clone();
    let sync = state.sync.read().clone();
    let current = *state.current.read();
    let rows = state.rows.read();

    let names: HashMap<i64, &str> = rows.iter().map(|(q, _)| (q.id, q.name.as_str())).collect();
    let visible: Vec<_> = rows.iter().filter(|(q, _)| filter.matches(q)).collect();
    let visible_done = visible.iter().filter(|(_, p)| p.status == QuestStatus::Completed).count();
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
                    value: "{filter.search}",
                    oninput: move |e| state.filter.write().search = e.value(),
                }
                button {
                    class: "btn btn-primary",
                    disabled: matches!(sync, SyncStatus::Running { .. }),
                    onclick: move |_| {
                        spawn(run_sync(state));
                    },
                    if let SyncStatus::Running { .. } = sync {
                        span { class: "spinner" }
                        "Syncing…"
                    } else {
                        "Sync Quests ⟳"
                    }
                }
            }
            match &sync {
                SyncStatus::Running { done, total } if *total > 0 => rsx! {
                    div { class: "sync-status", "Fetching quest pages: {done}/{total}" }
                },
                SyncStatus::Running { .. } => rsx! {
                    div { class: "sync-status", "Listing wiki categories…" }
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
                        p { "Create a playthrough to start tracking your progress." }
                        button { class: "btn btn-primary", onclick: move |_| show_modal.set(true), "+ New Playthrough" }
                    }
                } else if rows.is_empty() {
                    div { class: "empty",
                        h2 { "No quests yet" }
                        p { "Click " strong { "Sync Quests" } " to download quest data from the Witcher wiki." }
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
                            prereq_names: quest.prerequisite_ids.iter().filter_map(|id| names.get(id)).map(|n| n.to_string()).collect::<Vec<_>>(),
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
            NewPlaythroughModal { on_close: move |_| show_modal.set(false) }
        }
    }
}

/// Scrapes the wiki, then merges into the DB. The DB is untouched if scraping fails.
async fn run_sync(mut state: AppState) {
    let mut sync = state.sync;
    sync.set(SyncStatus::Running { done: 0, total: 0 });

    let remote = match fetch_remote(move |done, total| sync.set(SyncStatus::Running { done, total })).await {
        Ok(remote) => remote,
        Err(e) => {
            tracing::error!("Sync failed: {e}");
            sync.set(SyncStatus::Failed(e));
            return;
        }
    };
    for (title, err) in &remote.failed {
        tracing::warn!("Failed to scrape '{title}': {err}");
    }

    let merged = {
        let mut conn = state.db.write();
        merge(&mut conn, &remote.store)
    };
    match merged {
        Ok(report) => {
            for name in &report.unresolved {
                tracing::debug!("Unresolved quest link: {name}");
            }
            let mut msg = format!(
                "Synced {} quests ({} new, {} updated).",
                report.inserted + report.updated,
                report.inserted,
                report.updated
            );
            if !remote.failed.is_empty() {
                msg.push_str(&format!(" {} pages failed to load.", remote.failed.len()));
            }
            sync.set(SyncStatus::Done(msg));
            state.reload();
        }
        Err(e) => sync.set(SyncStatus::Failed(e.to_string())),
    }
}
