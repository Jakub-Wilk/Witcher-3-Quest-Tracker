//! Keeps playthroughs in sync with the game's save files. Saves are only ever read.
//!
//! Every save carries its lineage: the time of the new game it descends from (identifying the
//! in-game run) and the times of all saves before it in that line of play. Each linked
//! playthrough remembers its *head*, the newest save applied so far:
//! - a newer save that has the head in its history is normal progress;
//! - a newer save without it was made after loading an older save, so quests may revert;
//! - a save not newer than the head (an old file touched by cloud sync, say) is ignored.
//!
//! The newest save always wins because it is the game's real current state; the user's manual
//! statuses are stored separately and are never touched.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::NaiveDateTime;
use quest_db::{Connection, HeadSave, QuestStatus, SaveLink, playthroughs, progress, quests};
use w3_formats::{JournalStatus, SaveSnapshot, SaveTime};

/// A parsed save file.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedSave {
    pub path: PathBuf,
    pub snapshot: SaveSnapshot,
}

impl ParsedSave {
    pub fn file_name(&self) -> String {
        self.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
    }

    pub fn saved_at(&self) -> Option<NaiveDateTime> {
        self.snapshot.self_key().and_then(SaveTime::to_datetime)
    }

    /// Save kind from the file name: `ManualSave`, `AutoSave`, `QuickSave` or `CheckPoint`.
    pub fn kind(&self) -> &'static str {
        let name = self.file_name();
        if name.starts_with("QuickSave") {
            "Quicksave"
        } else if name.starts_with("AutoSave") {
            "Autosave"
        } else if name.starts_with("CheckPoint") {
            "Checkpoint"
        } else {
            "Manual save"
        }
    }

    fn link(&self) -> Option<SaveLink> {
        let root = self.snapshot.lineage_root()?;
        Some(SaveLink {
            lineage_root: root.0 as i64,
            game_playthrough_id: self.snapshot.playthrough_id.clone(),
            started_at: root.to_datetime(),
        })
    }
}

/// An in-game run found in the save folder that no playthrough is linked to yet.
#[derive(Debug, Clone, PartialEq)]
pub struct DetectedRun {
    pub lineage_root: i64,
    pub started_at: Option<NaiveDateTime>,
    pub game_playthrough_id: Option<String>,
    pub save_count: usize,
    /// The newest save of the run; applied once the run is linked.
    pub newest: ParsedSave,
}

impl DetectedRun {
    pub fn link(&self) -> SaveLink {
        self.newest.link().expect("detected runs always have a lineage")
    }
}

/// What [`process`] did with a save.
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    /// The save's state was applied to a playthrough.
    Applied {
        playthrough_id: i64,
        /// The save does not descend from the previous head: an older save was loaded.
        rollback: bool,
        changed: usize,
        reverted: usize,
    },
    /// Not newer than what the playthrough already shows.
    UpToDate,
    /// The save belongs to a run no playthrough is linked to.
    Unlinked,
    /// The save belongs to a run the user chose to ignore.
    Ignored,
}

/// Reads one save file.
pub fn read(path: &Path) -> w3_formats::Result<ParsedSave> {
    Ok(ParsedSave { path: path.to_path_buf(), snapshot: w3_formats::read_save(path)? })
}

/// Reads a save the game may still be writing: retries a few times while it is incomplete or
/// locked.
pub async fn read_when_complete(path: PathBuf) -> w3_formats::Result<ParsedSave> {
    let mut attempt = 0;
    loop {
        let p = path.clone();
        let result = tokio::task::spawn_blocking(move || read(&p))
            .await
            .unwrap_or_else(|e| Err(w3_formats::Error::Format(e.to_string())));
        match result {
            Err(w3_formats::Error::Truncated | w3_formats::Error::Io { .. }) if attempt < 5 => {
                attempt += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
            other => return other,
        }
    }
}

/// `.sav` files directly in `dir`.
pub fn list_saves(dir: &Path) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("sav")))
        .collect()
}

/// The result of reading a whole save folder.
#[derive(Debug, Default)]
pub struct Scan {
    /// The newest save of every run, with the number of saves of that run.
    pub newest: Vec<(ParsedSave, usize)>,
    /// Files that could not be read, with the reason.
    pub errors: Vec<(PathBuf, String)>,
}

/// Reads every save in `dir`, keeping only the newest one per run.
pub fn scan(dir: &Path) -> Scan {
    let mut by_run: HashMap<SaveTime, (ParsedSave, usize)> = HashMap::new();
    let mut result = Scan::default();
    for path in list_saves(dir) {
        let save = match read(&path) {
            Ok(save) => save,
            Err(e) => {
                result.errors.push((path, e.to_string()));
                continue;
            }
        };
        let Some(root) = save.snapshot.lineage_root() else { continue };
        match by_run.get_mut(&root) {
            Some((newest, count)) => {
                *count += 1;
                if save.snapshot.self_key() > newest.snapshot.self_key() {
                    *newest = save;
                }
            }
            None => {
                by_run.insert(root, (save, 1));
            }
        }
    }
    result.newest = by_run.into_values().collect();
    result.newest.sort_by_key(|(save, _)| save.snapshot.lineage_root());
    result
}

/// Routes a save to its playthrough and applies it when it is newer than the playthrough's head.
pub fn process(conn: &mut Connection, save: &ParsedSave) -> quest_db::Result<Outcome> {
    let (Some(root), Some(key)) = (save.snapshot.lineage_root(), save.snapshot.self_key()) else {
        return Ok(Outcome::Ignored);
    };
    let root = root.0 as i64;
    if playthroughs::is_lineage_ignored(conn, root)? {
        return Ok(Outcome::Ignored);
    }
    let Some(playthrough) = playthroughs::find_by_lineage(conn, root)? else {
        return Ok(Outcome::Unlinked);
    };
    let rollback = match &playthrough.head {
        Some(head) if key.0 as i64 <= head.key => return Ok(Outcome::UpToDate),
        Some(head) => !save.snapshot.descends_from(SaveTime(head.key as u64)),
        None => false,
    };
    apply(conn, playthrough.id, save, rollback)
}

/// Links a run to an existing playthrough and applies its newest save.
pub fn link(conn: &mut Connection, playthrough_id: i64, run: &DetectedRun) -> quest_db::Result<Outcome> {
    playthroughs::set_link(conn, playthrough_id, Some(&run.link()))?;
    progress::clear_manual_statuses_of_trackable(conn, playthrough_id)?;
    apply(conn, playthrough_id, &run.newest, false)
}

/// Writes a save's quest statuses and makes it the playthrough's head, in one transaction.
fn apply(conn: &mut Connection, playthrough_id: i64, save: &ParsedSave, rollback: bool) -> quest_db::Result<Outcome> {
    let tx = conn.transaction()?;
    let ids = quests::journal_ids(&tx)?;
    let mut per_quest: HashMap<i64, Vec<JournalStatus>> = HashMap::new();
    for (path, &status) in &save.snapshot.journal {
        if let Some(&id) = ids.get(path) {
            per_quest.entry(id).or_default().push(status);
        }
    }
    let statuses: HashMap<i64, QuestStatus> =
        per_quest.into_iter().map(|(id, files)| (id, combine(&files))).collect();
    let report = progress::apply_save_statuses(&tx, playthrough_id, &statuses)?;
    let head = HeadSave {
        key: save.snapshot.self_key().map_or(0, |k| k.0 as i64),
        file: save.file_name(),
        saved_at: save.saved_at(),
    };
    playthroughs::set_head(&tx, playthrough_id, &head)?;
    tx.commit()?;
    Ok(Outcome::Applied {
        playthrough_id,
        rollback,
        changed: report.changed,
        reverted: report.reverted.len(),
    })
}

/// The status of a quest from the states of its journal files (usually one; several for a quest
/// the game splits into parts or alternative endings). Files the player has not reached yet
/// don't count. Anything still active means the quest is in progress — e.g. part 1 done and
/// part 2 started; otherwise any success completes it (only one alternative ending ever does),
/// and a failure shows only when nothing succeeded.
fn combine(files: &[JournalStatus]) -> QuestStatus {
    let has = |s: JournalStatus| files.contains(&s);
    if has(JournalStatus::Active) {
        QuestStatus::InProgress
    } else if has(JournalStatus::Success) {
        QuestStatus::Completed
    } else if has(JournalStatus::Failed) {
        QuestStatus::Failed
    } else {
        QuestStatus::NotStarted
    }
}

/// Watches `dir` (non-recursively) for `.sav` files being created or written. Dropping the
/// watcher closes the receiver; use [`next_batch`] to debounce it.
pub fn watch(dir: &Path) -> notify::Result<(notify::RecommendedWatcher, tokio::sync::mpsc::UnboundedReceiver<PathBuf>)> {
    use notify::{EventKind, RecursiveMode, Watcher};

    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<PathBuf>();
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
        let Ok(event) = event else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        for path in event.paths {
            if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("sav")) {
                let _ = tx.send(path);
            }
        }
    })?;
    watcher.watch(dir, RecursiveMode::NonRecursive)?;
    Ok((watcher, rx))
}

/// Waits for the next written save files and returns them once writing has been quiet for two
/// seconds, so a save the game writes in several steps is reported once. `None` when the
/// watcher is gone.
pub async fn next_batch(rx: &mut tokio::sync::mpsc::UnboundedReceiver<PathBuf>) -> Option<Vec<PathBuf>> {
    let mut batch = vec![rx.recv().await?];
    while let Ok(next) = tokio::time::timeout(Duration::from_secs(2), rx.recv()).await {
        let path = next?;
        if !batch.contains(&path) {
            batch.push(path);
        }
    }
    Some(batch)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use quest_db::{NewPlaythrough, NewQuest, QuestSource, QuestText, QuestType, Region, open_in_memory};
    use w3_formats::HistoryRecord;

    const QUEST_A: &str = "gameplay\\journal\\quests\\a.journal";
    const QUEST_B: &str = "gameplay\\journal\\quests\\b.journal";
    const ROOT: u64 = 100;

    fn add_quest(conn: &Connection, path: &str) -> i64 {
        let quest = NewQuest {
            journal_path: path.into(),
            journal_guid: path.into(),
            base_name: path.into(),
            source: QuestSource::BaseGame,
            quest_type: QuestType::MainQuest,
            region: Region::Velen,
            recommended_level: None,
            wiki_page_id: None,
            wiki_title: None,
            important_notes: None,
            is_unmarked: false,
            texts: vec![QuestText { language: "en".into(), title: path.into(), description: None }],
            extra_journal_paths: vec![],
        };
        quests::upsert(conn, &quest).unwrap().0
    }

    /// A save whose history is `ROOT` followed by `keys`; the last key is the save itself.
    fn save(keys: &[u64], journal: &[(&str, JournalStatus)]) -> ParsedSave {
        let mut history = vec![HistoryRecord { time: SaveTime(ROOT), kind: 1, version: 66 }];
        history.extend(keys.iter().map(|&k| HistoryRecord { time: SaveTime(k), kind: 2, version: 66 }));
        ParsedSave {
            path: PathBuf::from(format!("AutoSave_{}.sav", keys.last().unwrap())),
            snapshot: SaveSnapshot {
                save_version: 66,
                game_version: 29,
                playthrough_id: Some("6abfb2520000423b".into()),
                history,
                journal: journal.iter().map(|(p, s)| (p.to_string(), *s)).collect::<HashMap<_, _>>(),
            },
        }
    }

    fn setup() -> (Connection, i64, i64, i64) {
        let conn = open_in_memory().unwrap();
        let a = add_quest(&conn, QUEST_A);
        let b = add_quest(&conn, QUEST_B);
        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough {
                name: "Run".into(),
                is_new_game_plus: false,
                notes: None,
                link: None,
            },
        )
        .unwrap();
        (conn, pt, a, b)
    }

    fn status(conn: &Connection, pt: i64, quest: i64) -> QuestStatus {
        progress::get(conn, pt, quest).unwrap().map_or(QuestStatus::NotStarted, |p| p.status())
    }

    fn run(newest: ParsedSave) -> DetectedRun {
        DetectedRun {
            lineage_root: ROOT as i64,
            started_at: None,
            game_playthrough_id: None,
            save_count: 1,
            newest,
        }
    }

    #[test]
    fn unknown_run_is_unlinked_until_linked() {
        let (mut conn, pt, a, _) = setup();
        let first = save(&[200], &[(QUEST_A, JournalStatus::Active)]);
        assert_eq!(process(&mut conn, &first).unwrap(), Outcome::Unlinked);

        let outcome = link(&mut conn, pt, &run(first)).unwrap();
        assert!(matches!(outcome, Outcome::Applied { rollback: false, changed: 2, .. }), "{outcome:?}");
        assert_eq!(status(&conn, pt, a), QuestStatus::InProgress);
        let head = playthroughs::get(&conn, pt).unwrap().head.unwrap();
        assert_eq!((head.key, head.file.as_str()), (200, "AutoSave_200.sav"));
    }

    #[test]
    fn forward_progress_then_stale_files_are_ignored() {
        let (mut conn, pt, a, b) = setup();
        link(&mut conn, pt, &run(save(&[200], &[(QUEST_A, JournalStatus::Active)]))).unwrap();

        let later = save(&[200, 300], &[(QUEST_A, JournalStatus::Success), (QUEST_B, JournalStatus::Active)]);
        let outcome = process(&mut conn, &later).unwrap();
        assert!(matches!(outcome, Outcome::Applied { rollback: false, reverted: 0, .. }), "{outcome:?}");
        assert_eq!(status(&conn, pt, a), QuestStatus::Completed);
        assert_eq!(status(&conn, pt, b), QuestStatus::InProgress);

        // The earlier file touched again (e.g. by cloud sync) changes nothing.
        assert_eq!(process(&mut conn, &save(&[200], &[])).unwrap(), Outcome::UpToDate);
        assert_eq!(process(&mut conn, &later).unwrap(), Outcome::UpToDate);
        assert_eq!(status(&conn, pt, a), QuestStatus::Completed);
    }

    #[test]
    fn saving_after_loading_an_older_save_is_a_rollback() {
        let (mut conn, pt, a, b) = setup();
        link(&mut conn, pt, &run(save(&[200], &[(QUEST_A, JournalStatus::Active)]))).unwrap();
        process(&mut conn, &save(&[200, 300], &[(QUEST_A, JournalStatus::Success), (QUEST_B, JournalStatus::Active)]))
            .unwrap();

        // Loaded the save made at 200 and saved again at 400: 300 is not in the history.
        let branch = save(&[200, 400], &[(QUEST_A, JournalStatus::Active)]);
        let outcome = process(&mut conn, &branch).unwrap();
        assert_eq!(
            outcome,
            Outcome::Applied { playthrough_id: pt, rollback: true, changed: 2, reverted: 2 }
        );
        assert_eq!(status(&conn, pt, a), QuestStatus::InProgress);
        assert_eq!(status(&conn, pt, b), QuestStatus::NotStarted);
    }

    #[test]
    fn manual_status_survives_save_updates() {
        let (mut conn, pt, a, _) = setup();
        link(&mut conn, pt, &run(save(&[200], &[(QUEST_A, JournalStatus::Active)]))).unwrap();
        progress::set_manual_status(&conn, pt, a, Some(QuestStatus::Failed)).unwrap();
        process(&mut conn, &save(&[200, 300], &[(QUEST_A, JournalStatus::Success)])).unwrap();
        assert_eq!(status(&conn, pt, a), QuestStatus::Failed);
        let p = progress::get(&conn, pt, a).unwrap().unwrap();
        assert_eq!(p.save_status, Some(QuestStatus::Completed));
    }

    #[test]
    fn linking_replaces_manual_statuses_of_journal_quests() {
        let (mut conn, pt, a, b) = setup();
        progress::set_manual_status(&conn, pt, a, Some(QuestStatus::Completed)).unwrap();
        progress::set_notes(&conn, pt, b, Some("keep me")).unwrap();
        link(&mut conn, pt, &run(save(&[200], &[(QUEST_A, JournalStatus::Active)]))).unwrap();
        let p = progress::get(&conn, pt, a).unwrap().unwrap();
        assert_eq!((p.manual_status, p.status()), (None, QuestStatus::InProgress));
        assert_eq!(progress::get(&conn, pt, b).unwrap().unwrap().notes.as_deref(), Some("keep me"));
    }

    #[test]
    fn combines_the_files_of_a_split_quest() {
        use JournalStatus::*;
        assert_eq!(combine(&[Success, Active]), QuestStatus::InProgress, "part 1 done, part 2 started");
        assert_eq!(combine(&[Success, Inactive, Inactive]), QuestStatus::Completed, "one ending reached");
        assert_eq!(combine(&[Success]), QuestStatus::Completed);
        assert_eq!(combine(&[Failed, Success]), QuestStatus::Completed);
        assert_eq!(combine(&[Failed, Inactive]), QuestStatus::Failed);
        assert_eq!(combine(&[Inactive]), QuestStatus::NotStarted);
        assert_eq!(combine(&[Active]), QuestStatus::InProgress);
    }

    #[test]
    fn saves_update_a_quest_through_any_of_its_files() {
        let (mut conn, pt, a, _) = setup();
        let extra = "extra-part.journal";
        quests::set_extra_journal_paths(&conn, a, &[extra.to_string()]).unwrap();
        link(&mut conn, pt, &run(save(&[200], &[(QUEST_A, JournalStatus::Success), (extra, JournalStatus::Active)])))
            .unwrap();
        assert_eq!(status(&conn, pt, a), QuestStatus::InProgress);
        process(&mut conn, &save(&[200, 300], &[(QUEST_A, JournalStatus::Success), (extra, JournalStatus::Success)]))
            .unwrap();
        assert_eq!(status(&conn, pt, a), QuestStatus::Completed);
    }

    #[test]
    fn ignored_runs_are_skipped() {
        let (mut conn, _, _, _) = setup();
        playthroughs::ignore_lineage(&conn, ROOT as i64).unwrap();
        assert_eq!(process(&mut conn, &save(&[200], &[])).unwrap(), Outcome::Ignored);
    }

    /// Real game catalog (no wiki) + the real Remastered fixture save. Run with
    /// `W3_GAME_DIR=... cargo test real_save_against_real_catalog -- --ignored --nocapture`.
    #[test]
    #[ignore = "needs W3_GAME_DIR and the fixture saves"]
    fn real_save_against_real_catalog() {
        let Some(game_dir) = std::env::var_os("W3_GAME_DIR") else { return };
        let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("crates/w3_formats/tests/fixtures/saves/ManualSave_55607_7ea49400_27df6a.sav");
        let catalog = w3_formats::read_game_catalog(Path::new(&game_dir)).unwrap();
        let mut conn = open_in_memory().unwrap();
        crate::sync_service::merge(&mut conn, &crate::sync_service::Fetched { catalog, wiki: Err("offline".into()) })
            .unwrap();

        let saved = read(&fixture).unwrap();
        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough { name: "2026".into(), is_new_game_plus: false, notes: None, link: None },
        )
        .unwrap();
        let run = DetectedRun { lineage_root: 0, started_at: None, game_playthrough_id: None, save_count: 1, newest: saved };
        link(&mut conn, pt, &run).unwrap();

        let rows = progress::list_for_playthrough(&conn, pt, "en").unwrap();
        let by_title = |t: &str| rows.iter().find(|(q, _)| q.title == t).map(|(_, p)| p.status()).unwrap();
        let summary = progress::completion_summary(&conn, pt).unwrap();
        println!("{summary:?}");
        assert_eq!(by_title("Kaer Morhen"), QuestStatus::Completed);
        assert_eq!(by_title("The Beast of White Orchard"), QuestStatus::Completed);
        assert_eq!(by_title("Missing in Action"), QuestStatus::Completed);
        assert_eq!(by_title("Wild at Heart"), QuestStatus::InProgress); // sq104werewolf
        assert!(summary.completed >= 10 && summary.in_progress >= 5, "{summary:?}");
    }

    #[test]
    fn scans_fixture_copies_keeping_the_newest_save_per_run() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("crates/w3_formats/tests/fixtures/saves");
        if !dir.is_dir() {
            return;
        }
        let scan = scan(&dir);
        assert!(scan.errors.is_empty(), "{:?}", scan.errors);
        let remastered = scan
            .newest
            .iter()
            .find(|(s, _)| s.snapshot.playthrough_id.as_deref() == Some("6abfb2520000423b"))
            .unwrap();
        assert_eq!(remastered.0.file_name(), "ManualSave_55607_7ea49400_27df6a.sav");
        assert_eq!(remastered.1, 3);
        assert_eq!(remastered.0.kind(), "Manual save");
    }
}
