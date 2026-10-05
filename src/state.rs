//! Shared reactive app state, provided once at the root via context.

use chrono::Utc;
use dioxus::prelude::*;
use quest_db::{
    CompletionSummary, Connection, NewPlaythrough, Playthrough, ProgressFilter, Quest,
    QuestProgress, QuestProgressUpdate, QuestSource, QuestStatus, QuestType, playthroughs,
    progress,
};

/// Sidebar / search filters applied to the quest list.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    pub source: Option<QuestSource>,
    pub quest_type: Option<QuestType>,
    pub search: String,
}

impl Filter {
    pub fn matches(&self, quest: &Quest) -> bool {
        self.source.is_none_or(|s| s == quest.source)
            && self.quest_type.is_none_or(|t| t == quest.quest_type)
            && (self.search.is_empty()
                || quest.name.to_lowercase().contains(&self.search.to_lowercase()))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,
    Running { done: usize, total: usize },
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
    pub filter: Signal<Filter>,
    pub sync: Signal<SyncStatus>,
    pub error: Signal<Option<String>>,
}

/// Required for component props. There is only ever one `AppState`, created once at the root.
impl PartialEq for AppState {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl AppState {
    pub fn new(conn: Connection) -> Self {
        let mut state = Self {
            db: CopyValue::new(conn),
            playthroughs: Signal::new(Vec::new()),
            current: Signal::new(None),
            rows: Signal::new(Vec::new()),
            summary: Signal::new(CompletionSummary::default()),
            filter: Signal::new(Filter::default()),
            sync: Signal::new(SyncStatus::Idle),
            error: Signal::new(None),
        };
        state.reload_playthroughs();
        let latest = state.playthroughs.peek().last().map(|p| p.id);
        state.current.set(latest);
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

    pub fn select_playthrough(&mut self, id: i64) {
        self.current.set(Some(id));
        self.reload();
    }

    pub fn create_playthrough(&mut self, new: NewPlaythrough) {
        let inserted = playthroughs::insert(&self.db.read(), &new);
        if let Some(id) = self.report(inserted) {
            self.reload_playthroughs();
            self.select_playthrough(id);
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
