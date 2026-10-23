use dioxus::prelude::*;
use witcher_wiki::ScrapeProgress;

use crate::components::ornament::{Glyph, Icon};
use crate::i18n::Tr;
use crate::item_sync::PictureProgress;
use crate::state::{AppState, SyncStatus};
use crate::sync_service::{SyncPhase, SyncReport};
use crate::t;

/// What a running sync is doing, or how the last one ended. Dismissible once finished.
#[component]
pub fn SyncBanner() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let sync = state.sync.read().clone();
    let (class, text) = match &sync {
        SyncStatus::Idle => return rsx! {},
        SyncStatus::Running(phase) => ("sync-banner is-running", phase_text(tr, *phase)),
        SyncStatus::Done(report) => ("sync-banner is-done", report_text(tr, report)),
        SyncStatus::Failed(msg) => ("sync-banner is-failed", t!(tr, "sync.failed", reason = msg)),
    };
    rsx! {
        div { class,
            if matches!(sync, SyncStatus::Running(_)) {
                span { class: "sync-banner-spinner" }
            }
            span { class: "sync-banner-text", "{text}" }
            if !matches!(sync, SyncStatus::Running(_)) {
                button {
                    class: "btn-x",
                    title: t!(tr, "common.dismiss"),
                    onclick: move |_| state.sync.set(SyncStatus::Idle),
                    Glyph { icon: Icon::Close }
                }
            }
        }
    }
}

/// One-paragraph summary of a finished sync.
fn report_text(tr: Tr, r: &SyncReport) -> String {
    let mut parts = vec![t!(
        tr,
        "sync.done_quests",
        total = r.inserted + r.updated,
        new = r.inserted,
        updated = r.updated,
        languages = r.languages
    )];
    if r.removed > 0 {
        parts.push(t!(tr, "sync.done_removed", n = r.removed));
    }
    parts.push(match &r.wiki_error {
        Some(e) => t!(tr, "sync.done_wiki_failed", error = e),
        None => t!(tr, "sync.done_wiki", matched = r.wiki_matched, manual = r.wiki_only),
    });
    let items = &r.items;
    if items.items > 0 {
        parts.push(t!(
            tr,
            "sync.done_items",
            items = items.items,
            recipes = items.base_recipes,
            plus = items.plus_recipes,
            pictures = items.pictures
        ));
    }
    if let Some(e) = &items.wiki_error {
        parts.push(t!(tr, "sync.done_pictures_failed", error = e));
    }
    if !r.kept_stale.is_empty() {
        parts.push(t!(tr, "sync.done_kept", n = r.kept_stale.len()));
    }
    parts.join(" ")
}

pub fn phase_text(tr: Tr, phase: SyncPhase) -> String {
    match phase {
        SyncPhase::ReadingGame => t!(tr, "sync.reading_game").into(),
        SyncPhase::Wiki(ScrapeProgress::ListingCategories) => t!(tr, "sync.listing").into(),
        SyncPhase::Wiki(ScrapeProgress::FetchingPages { done, total }) => {
            t!(tr, "sync.quest_pages", done = done, total = total)
        }
        SyncPhase::Wiki(ScrapeProgress::Translating { done, total }) => {
            t!(tr, "sync.translations", done = done, total = total)
        }
        SyncPhase::Pictures(PictureProgress::ReadingPages { done, total }) => {
            t!(tr, "sync.item_pages", done = done, total = total)
        }
        SyncPhase::Pictures(PictureProgress::Downloading { done, total }) => {
            t!(tr, "sync.pictures", done = done, total = total)
        }
        SyncPhase::Merging => t!(tr, "sync.saving").into(),
    }
}
