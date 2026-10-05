//! Shared reactive app state, provided once at the root via context.

use std::path::{Path, PathBuf};

use chrono::Utc;
use dioxus::prelude::*;
use quest_db::{
    CompletionSummary, Connection, NewPlaythrough, Playthrough, PlaythroughUpdate, ProgressFilter,
    Quest, QuestProgress, QuestProgressUpdate, QuestStatus, playthroughs, progress, settings,
};
use quest_scraper::{Language, ScrapeProgress};

use crate::sync_service::SYNC_LANGUAGE_KEY;
use crate::view_options::ViewOptions;

/// Settings key holding the id of the last selected playthrough.
const LAST_PLAYTHROUGH_KEY: &str = "last_playthrough_id";

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,
    Running(ScrapeProgress),
    Done(String),
    Failed(String),
}

/// All app state. Every field is `Copy`, so the struct can be moved into event handlers freely.
#[derive(Clone, Copy)]
pub struct AppState {
    /// The SQLite connection. A `CopyValue` (not a `Signal`) since it is not reactive itself.
    pub db: CopyValue<Connection>,
    pub playthroughs: Signal<Vec<Playthrough>>,
    pub current: Signal<Option<i64>>,
    /// Every quest with its progress in the current playthrough (unfiltered).
    pub rows: Signal<Vec<(Quest, QuestProgress)>>,
    pub summary: Signal<CompletionSummary>,
    /// Quest list filters and sort order; persisted to `view_options_path` on change.
    pub view: Signal<ViewOptions>,
    pub view_options_path: CopyValue<PathBuf>,
    pub sync: Signal<SyncStatus>,
    /// Language selected for the next sync (defaults to the last synced one).
    pub language: Signal<Language>,
    pub error: Signal<Option<String>>,
}

/// Required for component props. There is only ever one `AppState`, created once at the root.
impl PartialEq for AppState {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl AppState {
    /// `view_options_path` is the TOML file holding the quest list filters and sort order.
    pub fn new(conn: Connection, view_options_path: &Path) -> Self {
        let language = settings::get(&conn, SYNC_LANGUAGE_KEY)
            .ok()
            .flatten()
            .and_then(|code| code.parse().ok())
            .unwrap_or_default();
        let last_playthrough: Option<i64> = settings::get(&conn, LAST_PLAYTHROUGH_KEY)
            .ok()
            .flatten()
            .and_then(|id| id.parse().ok());

        let mut state = Self {
            db: CopyValue::new(conn),
            playthroughs: Signal::new(Vec::new()),
            current: Signal::new(None),
            rows: Signal::new(Vec::new()),
            summary: Signal::new(CompletionSummary::default()),
            view: Signal::new(ViewOptions::load(view_options_path)),
            view_options_path: CopyValue::new(view_options_path.to_path_buf()),
            sync: Signal::new(SyncStatus::Idle),
            language: Signal::new(language),
            error: Signal::new(None),
        };
        state.reload_playthroughs();
        let initial = {
            let list = state.playthroughs.peek();
            last_playthrough
                .filter(|id| list.iter().any(|p| p.id == *id))
                .or_else(|| list.last().map(|p| p.id))
        };
        state.current.set(initial);
        state.reload();
        state
    }

    /// Records an error for display in the toast, returning `None` on failure.
    pub fn report<T>(&mut self, result: quest_db::Result<T>) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(e) => {
                tracing::error!("{e}");
                self.error.set(Some(e.to_string()));
                None
            }
        }
    }

    /// Writes the current view options to disk.
    pub fn save_view_options(&mut self) {
        let path = self.view_options_path.read().clone();
        if let Err(e) = self.view.peek().save(&path) {
            tracing::error!("Failed to save {}: {e}", path.display());
            self.error.set(Some(format!("Could not save view settings: {e}")));
        }
    }

    pub fn reload_playthroughs(&mut self) {
        let list = playthroughs::list(&self.db.read());
        if let Some(list) = self.report(list) {
            self.playthroughs.set(list);
        }
    }

    /// Re-reads quests, progress and the summary for the current playthrough.
    pub fn reload(&mut self) {
        let Some(pt) = *self.current.peek() else {
            self.rows.set(Vec::new());
            self.summary.set(CompletionSummary::default());
            return;
        };
        let loaded = {
            let conn = self.db.read();
            progress::list_for_playthrough(&conn, pt, &ProgressFilter::default())
                .and_then(|rows| Ok((rows, progress::completion_summary(&conn, pt)?)))
        };
        if let Some((rows, summary)) = self.report(loaded) {
            self.rows.set(rows);
            self.summary.set(summary);
        }
    }

    /// Switches playthrough and remembers it for the next launch.
    pub fn select_playthrough(&mut self, id: Option<i64>) {
        self.current.set(id);
        if let Some(id) = id {
            let saved = settings::set(&self.db.read(), LAST_PLAYTHROUGH_KEY, &id.to_string());
            self.report(saved);
        }
        self.reload();
    }

    pub fn create_playthrough(&mut self, new: NewPlaythrough) {
        let inserted = playthroughs::insert(&self.db.read(), &new);
        if let Some(id) = self.report(inserted) {
            self.reload_playthroughs();
            self.select_playthrough(Some(id));
        }
    }

    pub fn update_playthrough(&mut self, id: i64, update: PlaythroughUpdate) {
        let updated = playthroughs::update(&self.db.read(), id, &update);
        if self.report(updated).is_some() {
            self.reload_playthroughs();
        }
    }

    /// Deletes a playthrough (and its progress) and switches to the newest remaining one.
    pub fn delete_playthrough(&mut self, id: i64) {
        let deleted = playthroughs::delete(&self.db.read(), id);
        if self.report(deleted).is_some() {
            self.reload_playthroughs();
            let next = self.playthroughs.peek().last().map(|p| p.id);
            self.select_playthrough(next);
        }
    }

    /// Sets a quest's status, stamping or clearing `completed_at` accordingly.
    pub fn set_status(&mut self, quest_id: i64, status: QuestStatus) {
        let Some(pt) = *self.current.peek() else { return };
        let completed_at = match status {
            QuestStatus::Completed => Some(Utc::now()),
            _ => None,
        };
        let update = QuestProgressUpdate {
            status: Some(status),
            completed_at: Some(completed_at),
            ..Default::default()
        };
        let result = progress::update_status(&self.db.read(), pt, quest_id, &update);
        if self.report(result).is_some() {
            self.reload();
        }
    }

    pub fn set_notes(&mut self, quest_id: i64, notes: String) {
        let Some(pt) = *self.current.peek() else { return };
        let notes = Some(notes.trim().to_string()).filter(|n| !n.is_empty());
        let update = QuestProgressUpdate { notes: Some(notes), ..Default::default() };
        let result = progress::update_status(&self.db.read(), pt, quest_id, &update);
        if self.report(result).is_some() {
            self.reload();
        }
    }
}
