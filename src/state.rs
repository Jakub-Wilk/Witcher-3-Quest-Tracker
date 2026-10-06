//! Shared reactive app state, provided once at the root via context.

use std::path::{Path, PathBuf};

use dioxus::prelude::*;
use quest_db::{
    CompletionSummary, Connection, Difficulty, NewPlaythrough, Playthrough, PlaythroughUpdate, Quest,
    QuestProgress, QuestStatus, playthroughs, progress, quests,
};

use crate::save_tracker::{self, DetectedRun, Outcome, ParsedSave, Scan};
use crate::settings::AppSettings;
use crate::sync_service::SyncPhase;
use crate::view_options::ViewOptions;

#[derive(Debug, Clone, PartialEq)]
pub enum SyncStatus {
    Idle,
    Running(SyncPhase),
    Done(String),
    Failed(String),
}

/// What the save tracker last did, for the header.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TrackerStatus {
    pub scanning: bool,
    /// e.g. "Quicksave 22:39 → Death March run".
    pub last: Option<String>,
}

/// How the user resolved a detected in-game run.
#[derive(Debug, Clone, PartialEq)]
pub enum RunChoice {
    Create { name: String, difficulty: Difficulty, is_new_game_plus: bool },
    Link(i64),
    Ignore,
}

/// All app state. Every field is `Copy`, so the struct can be moved into event handlers freely.
#[derive(Clone, Copy)]
pub struct AppState {
    /// The SQLite connection. A `CopyValue` (not a `Signal`) since it is not reactive itself.
    pub db: CopyValue<Connection>,
    /// Persisted settings, except the quest list options, which live in `view`.
    pub settings: Signal<AppSettings>,
    pub settings_path: CopyValue<PathBuf>,
    pub playthroughs: Signal<Vec<Playthrough>>,
    pub current: Signal<Option<i64>>,
    /// Every quest with its progress in the current playthrough (unfiltered).
    pub rows: Signal<Vec<(Quest, QuestProgress)>>,
    pub summary: Signal<CompletionSummary>,
    /// Quest list filters and sort order; persisted with the settings on change.
    pub view: Signal<ViewOptions>,
    /// Languages with quest text.
    pub languages: Signal<Vec<String>>,
    pub quest_count: Signal<i64>,
    pub sync: Signal<SyncStatus>,
    pub tracker: Signal<TrackerStatus>,
    /// In-game runs found in the save folder that are not linked to a playthrough yet.
    pub detected: Signal<Vec<DetectedRun>>,
    pub error: Signal<Option<String>>,
    /// Informational toast (e.g. a reloaded older save).
    pub notice: Signal<Option<String>>,
}

/// Required for component props. There is only ever one `AppState`, created once at the root.
impl PartialEq for AppState {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

impl AppState {
    /// `settings_path` is the TOML file holding the app settings.
    pub fn new(conn: Connection, settings_path: &Path) -> Self {
        let settings = AppSettings::load(settings_path);
        let view = settings.view.clone();
        let last_playthrough = settings.ui.last_playthrough_id;

        let mut state = Self {
            db: CopyValue::new(conn),
            settings: Signal::new(settings),
            settings_path: CopyValue::new(settings_path.to_path_buf()),
            playthroughs: Signal::new(Vec::new()),
            current: Signal::new(None),
            rows: Signal::new(Vec::new()),
            summary: Signal::new(CompletionSummary::default()),
            view: Signal::new(view),
            languages: Signal::new(Vec::new()),
            quest_count: Signal::new(0),
            sync: Signal::new(SyncStatus::Idle),
            tracker: Signal::new(TrackerStatus::default()),
            detected: Signal::new(Vec::new()),
            error: Signal::new(None),
            notice: Signal::new(None),
        };
        state.reload_playthroughs();
        state.reload_catalog_info();
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

    /// Writes the settings (with the current view options) to disk.
    pub fn save_settings(&mut self) {
        let path = self.settings_path.read().clone();
        let mut settings = self.settings.peek().clone();
        settings.view = self.view.peek().clone();
        if let Err(e) = settings.save(&path) {
            tracing::error!("Failed to save {}: {e}", path.display());
            self.error.set(Some(format!("Could not save settings: {e}")));
        }
    }

    /// Changes settings and saves them.
    pub fn update_settings(&mut self, change: impl FnOnce(&mut AppSettings)) {
        change(&mut self.settings.write());
        self.save_settings();
    }

    pub fn language(&self) -> String {
        self.settings.peek().ui.language.clone()
    }

    /// Switches the quest text language (display only).
    pub fn set_language(&mut self, language: String) {
        self.update_settings(|s| s.ui.language = language);
        self.reload();
    }

    pub fn reload_playthroughs(&mut self) {
        let list = playthroughs::list(&self.db.read());
        if let Some(list) = self.report(list) {
            self.playthroughs.set(list);
        }
    }

    /// Re-reads the quest count and available languages.
    pub fn reload_catalog_info(&mut self) {
        let info = {
            let conn = self.db.read();
            quests::count(&conn).and_then(|n| Ok((n, quests::languages(&conn)?)))
        };
        if let Some((count, languages)) = self.report(info) {
            self.quest_count.set(count);
            self.languages.set(languages);
        }
    }

    /// Re-reads quests, progress and the summary for the current playthrough.
    pub fn reload(&mut self) {
        let Some(pt) = *self.current.peek() else {
            self.rows.set(Vec::new());
            self.summary.set(CompletionSummary::default());
            return;
        };
        let language = self.language();
        let loaded = {
            let conn = self.db.read();
            progress::list_for_playthrough(&conn, pt, &language)
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
        if id.is_some() {
            self.update_settings(|s| s.ui.last_playthrough_id = id);
        }
        self.reload();
    }

    pub fn create_playthrough(&mut self, new: NewPlaythrough) -> Option<i64> {
        let inserted = playthroughs::insert(&self.db.read(), &new);
        let id = self.report(inserted)?;
        self.reload_playthroughs();
        self.select_playthrough(Some(id));
        Some(id)
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

    /// Stops tracking saves for a playthrough and forgets the statuses read from them.
    pub fn unlink_playthrough(&mut self, id: i64) {
        let result = {
            let conn = self.db.read();
            playthroughs::set_link(&conn, id, None).and_then(|_| progress::clear_save_statuses(&conn, id))
        };
        if self.report(result).is_some() {
            self.reload_playthroughs();
            self.reload();
        }
    }

    /// Sets the user's status for a quest; `None` follows the save again.
    pub fn set_status(&mut self, quest_id: i64, status: Option<QuestStatus>) {
        let Some(pt) = *self.current.peek() else { return };
        let result = progress::set_manual_status(&self.db.read(), pt, quest_id, status);
        if self.report(result).is_some() {
            self.reload();
        }
    }

    pub fn set_notes(&mut self, quest_id: i64, notes: String) {
        let Some(pt) = *self.current.peek() else { return };
        let notes = Some(notes.trim().to_string()).filter(|n| !n.is_empty());
        let result = progress::set_notes(&self.db.read(), pt, quest_id, notes.as_deref());
        if self.report(result).is_some() {
            self.reload();
        }
    }

    fn playthrough_name(&self, id: i64) -> String {
        self.playthroughs.peek().iter().find(|p| p.id == id).map(|p| p.name.clone()).unwrap_or_default()
    }

    /// Routes one save to its playthrough and refreshes the UI.
    pub fn handle_save(&mut self, save: ParsedSave) {
        let outcome = {
            let mut conn = self.db.write();
            save_tracker::process(&mut conn, &save)
        };
        let Some(outcome) = self.report(outcome) else { return };
        match outcome {
            Outcome::Applied { playthrough_id, rollback, reverted, .. } => {
                self.after_apply(playthrough_id, &save, rollback, reverted);
            }
            Outcome::Unlinked => self.add_detected(save, 1),
            Outcome::UpToDate | Outcome::Ignored => {}
        }
    }

    fn after_apply(&mut self, playthrough_id: i64, save: &ParsedSave, rollback: bool, reverted: usize) {
        self.reload_playthroughs();
        let name = self.playthrough_name(playthrough_id);
        let time = save.saved_at().map(|t| t.format("%H:%M").to_string()).unwrap_or_default();
        self.tracker.write().last = Some(format!("{} {time} → {name}", save.kind()));
        if rollback && reverted > 0 {
            self.notice.set(Some(format!(
                "An earlier save of “{name}” was loaded: {reverted} quest{} went back to how they were in {}.",
                if reverted == 1 { "" } else { "s" },
                save.file_name()
            )));
        }
        if *self.current.peek() == Some(playthrough_id) {
            self.reload();
        }
    }

    /// Remembers a run that needs the user's decision, keeping its newest save.
    fn add_detected(&mut self, save: ParsedSave, save_count: usize) {
        let Some(link) = save.snapshot.lineage_root() else { return };
        let root = link.0 as i64;
        let mut detected = self.detected.write();
        match detected.iter_mut().find(|r| r.lineage_root == root) {
            Some(run) => {
                run.save_count += save_count;
                if save.snapshot.self_key() > run.newest.snapshot.self_key() {
                    run.newest = save;
                }
            }
            None => detected.push(DetectedRun {
                lineage_root: root,
                started_at: link.to_datetime(),
                game_playthrough_id: save.snapshot.playthrough_id.clone(),
                save_count,
                newest: save,
            }),
        }
        detected.sort_by_key(|r| std::cmp::Reverse(r.newest.snapshot.self_key()));
    }

    /// Applies a folder scan: the newest save of every run.
    pub fn apply_scan(&mut self, scan: Scan) {
        for (path, error) in &scan.errors {
            tracing::warn!("Could not read {}: {error}", path.display());
        }
        self.detected.write().clear();
        for (save, count) in scan.newest {
            let outcome = {
                let mut conn = self.db.write();
                save_tracker::process(&mut conn, &save)
            };
            match self.report(outcome) {
                Some(Outcome::Applied { playthrough_id, rollback, reverted, .. }) => {
                    self.after_apply(playthrough_id, &save, rollback, reverted)
                }
                Some(Outcome::Unlinked) => self.add_detected(save, count),
                _ => {}
            }
        }
        self.tracker.write().scanning = false;
    }

    /// Re-reads the whole save folder in the background. With `reapply`, linked playthroughs
    /// forget their head first so their newest save is applied again (after a sync changed the
    /// quest list).
    pub fn rescan_saves(&mut self, reapply: bool) {
        let Some(dir) = self.settings.peek().valid_save_dir().map(Path::to_path_buf) else { return };
        if self.quest_count.peek().eq(&0) || self.tracker.peek().scanning {
            return;
        }
        if reapply {
            let result = {
                let conn = self.db.read();
                playthroughs::list(&conn).and_then(|list| {
                    for p in list.iter().filter(|p| p.link.is_some()) {
                        playthroughs::set_link(&conn, p.id, p.link.as_ref())?;
                    }
                    Ok(())
                })
            };
            self.report(result);
        }
        self.tracker.write().scanning = true;
        let mut state = *self;
        // Not tied to the component that triggered it: onboarding unmounts once quests exist.
        dioxus::core::spawn_forever(async move {
            let scan = tokio::task::spawn_blocking(move || save_tracker::scan(&dir)).await;
            match scan {
                Ok(scan) => state.apply_scan(scan),
                Err(e) => {
                    state.tracker.write().scanning = false;
                    state.error.set(Some(format!("Could not scan saves: {e}")));
                }
            }
        });
    }

    /// Resolves a detected run: links it to a new or existing playthrough, or ignores it.
    pub fn resolve_run(&mut self, lineage_root: i64, choice: RunChoice) {
        let Some(run) = self.detected.peek().iter().find(|r| r.lineage_root == lineage_root).cloned() else {
            return;
        };
        let target = match choice {
            RunChoice::Ignore => {
                let ignored = playthroughs::ignore_lineage(&self.db.read(), lineage_root);
                self.report(ignored);
                None
            }
            RunChoice::Link(id) => Some(id),
            RunChoice::Create { name, difficulty, is_new_game_plus } => self.create_playthrough(NewPlaythrough {
                name,
                difficulty,
                is_new_game_plus,
                notes: None,
                link: None,
            }),
        };
        self.detected.write().retain(|r| r.lineage_root != lineage_root);
        let Some(id) = target else { return };
        let outcome = {
            let mut conn = self.db.write();
            save_tracker::link(&mut conn, id, &run)
        };
        if self.report(outcome).is_some() {
            self.after_apply(id, &run.newest, false, 0);
            self.select_playthrough(Some(id));
        }
    }

    /// Runs a full sync in the background: game data, then the wiki, then one DB transaction.
    /// The DB is untouched if reading the game data fails. Afterwards the newest saves are
    /// applied again, since the quest list may have changed.
    pub fn start_sync(&mut self) {
        if matches!(*self.sync.peek(), SyncStatus::Running(_)) {
            return;
        }
        let Some(game_dir) = self.settings.peek().valid_game_dir().map(Path::to_path_buf) else {
            self.sync.set(SyncStatus::Failed("Set the game folder in Settings first.".into()));
            return;
        };
        let mut state = *self;
        let mut sync = self.sync;
        sync.set(SyncStatus::Running(SyncPhase::ReadingGame));
        // Not tied to the component that triggered it: onboarding unmounts once quests exist.
        dioxus::core::spawn_forever(async move {
            let fetched = match crate::sync_service::fetch(game_dir, move |p| sync.set(SyncStatus::Running(p))).await {
                Ok(fetched) => fetched,
                Err(e) => {
                    tracing::error!("Sync failed: {e}");
                    sync.set(SyncStatus::Failed(e));
                    return;
                }
            };
            sync.set(SyncStatus::Running(SyncPhase::Merging));
            let merged = {
                let mut conn = state.db.write();
                crate::sync_service::merge(&mut conn, &fetched)
            };
            match merged {
                Ok(report) => {
                    for title in &report.unmatched_wiki {
                        tracing::info!("Wiki quest without a game match: {title}");
                    }
                    for title in &report.unmatched_game {
                        tracing::debug!("Game quest without a wiki page: {title}");
                    }
                    for title in &report.unresolved {
                        tracing::debug!("Unresolved quest link: {title}");
                    }
                    sync.set(SyncStatus::Done(report.summary()));
                    state.reload_catalog_info();
                    state.reload();
                    state.rescan_saves(true);
                }
                Err(e) => sync.set(SyncStatus::Failed(e.to_string())),
            }
        });
    }

    /// Offers runs the user ignored earlier again.
    pub fn unignore_runs(&mut self) {
        let cleared = playthroughs::clear_ignored_lineages(&self.db.read());
        if self.report(cleared).is_some() {
            self.rescan_saves(false);
        }
    }
}
