use std::collections::HashMap;

use dioxus::prelude::*;
use quest_db::QuestStatus;
use quest_scraper::{Language, ScrapeProgress};

use super::{PlaythroughModal, QuestCard};
use crate::state::{AppState, SyncStatus};
use crate::sync_service::{fetch_remote, merge};

/// Center panel: search, sync (with title language), and the filtered list of quest cards.
#[component]
pub fn QuestList() -> Element {
    let mut state = use_context::<AppState>();
    let mut show_modal = use_signal(|| false);
    let view = state.view.read().clone();
    let sync = state.sync.read().clone();
    let syncing = matches!(sync, SyncStatus::Running(_));
    let language = *state.language.read();
    let current = *state.current.read();
    let rows = state.rows.read();

    let names: HashMap<i64, &str> = rows.iter().map(|(q, _)| (q.id, q.display_name())).collect();
    let visible = view.apply(&rows);
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
                    value: "{view.search}",
                    oninput: move |e| state.view.write().search = e.value(),
                }
                select {
                    class: "select",
                    title: "Language of quest titles (applied on sync)",
                    disabled: syncing,
                    onchange: move |e| {
                        if let Ok(lang) = e.value().parse::<Language>() {
                            state.language.set(lang);
                        }
                    },
                    for lang in Language::ALL {
                        option { value: "{lang}", selected: lang == language, "{lang.native_label()}" }
                    }
                }
                button {
                    class: "btn btn-primary",
                    disabled: syncing,
                    onclick: move |_| {
                        spawn(run_sync(state, language));
                    },
                    if syncing {
                        span { class: "spinner" }
                        "Syncing…"
                    } else {
                        "Sync Quests ⟳"
                    }
                }
            }
            match &sync {
                SyncStatus::Running(progress) => rsx! {
                    div { class: "sync-status", {progress_text(*progress)} }
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

fn progress_text(progress: ScrapeProgress) -> String {
    match progress {
        ScrapeProgress::ListingCategories => "Listing wiki categories…".into(),
        ScrapeProgress::FetchingPages { done, total } => format!("Fetching quest pages: {done}/{total}"),
        ScrapeProgress::Translating { done, total } => format!("Fetching translated titles: {done}/{total}"),
    }
}

/// Scrapes the wiki, then merges into the DB. The DB is untouched if scraping fails.
async fn run_sync(mut state: AppState, language: Language) {
    let mut sync = state.sync;
    sync.set(SyncStatus::Running(ScrapeProgress::ListingCategories));

    let scrape = match fetch_remote(language, move |p| sync.set(SyncStatus::Running(p))).await {
        Ok(scrape) => scrape,
        Err(e) => {
            tracing::error!("Sync failed: {e}");
            sync.set(SyncStatus::Failed(e));
            return;
        }
    };
    for (title, reason) in &scrape.skipped {
        tracing::info!("Skipped '{title}': {reason}");
    }

    let merged = {
        let mut conn = state.db.write();
        merge(&mut conn, &scrape)
    };
    match merged {
        Ok(report) => {
            for title in &report.unresolved {
                tracing::debug!("Unresolved quest link: {title}");
            }
            sync.set(SyncStatus::Done(report.summary()));
            state.reload();
        }
        Err(e) => sync.set(SyncStatus::Failed(e.to_string())),
    }
}
