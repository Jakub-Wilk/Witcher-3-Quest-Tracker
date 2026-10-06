//! Tests against real saves copied into `tests/fixtures/saves/` (personal files, not committed;
//! each test is skipped when its fixture is missing).

use std::path::PathBuf;

use w3_formats::save::HISTORY_NEW_GAME;
use w3_formats::{JournalStatus, SaveSnapshot, SaveTime, read_save};

fn fixture(name: &str) -> Option<SaveSnapshot> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/saves").join(name);
    if !path.exists() {
        eprintln!("skipping: fixture {name} not present");
        return None;
    }
    Some(read_save(&path).unwrap_or_else(|e| panic!("{name}: {e}")))
}

const REMASTERED_MANUAL: &str = "ManualSave_55607_7ea49400_27df6a.sav";
const REMASTERED_CHECKPOINT: &str = "CheckPoint_62c39_7ea49400_275e0a.sav";
const REMASTERED_QUICK: &str = "QuickSave_8559a_7ea49000_5a78d05.sav";
const NEXTGEN_MANUAL: &str = "ManualSave_8e954_7e948400_4d98227.sav";
const CLASSIC_2021: &str = "ManualSave_1004de_7e540800_318443.sav";
const CLASSIC_2018: &str = "ManualSave_100b2a_7e20d400_2a31861.sav";

const REMASTERED_ROOT: SaveTime = SaveTime(0x7ea4_8400_03e3_550d);

#[test]
fn remastered_save_versions_and_playthrough() {
    let Some(save) = fixture(REMASTERED_MANUAL) else { return };
    assert_eq!((save.save_version, save.game_version), (66, 29));
    assert_eq!(save.playthrough_id.as_deref(), Some("6abfb2520000423b"));
    assert_eq!(save.lineage_root(), Some(REMASTERED_ROOT));
    assert_eq!(save.history[0].kind, HISTORY_NEW_GAME);
    assert_eq!(save.history.len(), 127);
    let saved_at = save.self_key().unwrap().to_datetime().unwrap();
    assert_eq!(saved_at.format("%Y-%m-%d %H:%M:%S").to_string(), "2026-10-06 00:39:55");
}

#[test]
fn remastered_quest_statuses() {
    let Some(save) = fixture(REMASTERED_MANUAL) else { return };
    let status = |path: &str| save.journal.get(path).copied();
    assert_eq!(status("gameplay\\journal\\quests\\q001beggining.journal"), Some(JournalStatus::Success));
    assert_eq!(status("gameplay\\journal\\quests\\mq0001battlefieldmother.journal"), Some(JournalStatus::Success));
    assert_eq!(status("gameplay\\journal\\quests\\sq104werewolf.journal"), Some(JournalStatus::Active));
    assert_eq!(status("dlc\\ep1\\journal\\quests\\q601intro.journal"), Some(JournalStatus::Active));
    assert_eq!(status("gameplay\\journal\\quests\\q309novigradundercontrol2.journal"), Some(JournalStatus::Inactive));
    // Objectives and phases are not reported as separate resources.
    assert!(save.journal.keys().all(|k| k.ends_with(".journal")));
}

#[test]
fn lineage_links_consecutive_saves() {
    let (Some(checkpoint), Some(manual), Some(quick)) =
        (fixture(REMASTERED_CHECKPOINT), fixture(REMASTERED_MANUAL), fixture(REMASTERED_QUICK))
    else {
        return;
    };
    // The manual save was made after the checkpoint, in the same line of play.
    assert!(manual.descends_from(checkpoint.self_key().unwrap()));
    assert!(!checkpoint.descends_from(manual.self_key().unwrap()));
    assert!(manual.descends_from(quick.self_key().unwrap()));
    assert_eq!(checkpoint.lineage_root(), manual.lineage_root());
    assert!(checkpoint.self_key() < manual.self_key());
}

#[test]
fn nextgen_save_has_playthrough_id_without_trailing_nul() {
    let Some(save) = fixture(NEXTGEN_MANUAL) else { return };
    assert_eq!((save.save_version, save.game_version), (64, 27));
    assert_eq!(save.playthrough_id.as_deref(), Some("66b11b0300002849"));
    assert_eq!(save.lineage_root(), Some(SaveTime(0x7e83_9000_0522_3686)));
    assert!(!save.journal.is_empty());
}

#[test]
fn classic_saves_have_lineage_but_no_playthrough_id() {
    for (name, root) in
        [(CLASSIC_2021, 0x7e22_cc00_045b_ae82u64), (CLASSIC_2018, 0x7e15_5800_05a8_ad78u64)]
    {
        let Some(save) = fixture(name) else { continue };
        assert_eq!(save.save_version, 64, "{name}");
        assert_eq!(save.playthrough_id, None, "{name}");
        assert_eq!(save.lineage_root(), Some(SaveTime(root)), "{name}");
        assert_eq!(save.history[0].kind, HISTORY_NEW_GAME, "{name}");
        assert!(save.journal.len() > 100, "{name}: {} journal entries", save.journal.len());
    }
}

#[test]
fn truncated_copy_of_real_save_is_an_error() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/saves").join(REMASTERED_QUICK);
    let Ok(bytes) = std::fs::read(&path) else { return };
    let half = &bytes[..bytes.len() / 2];
    assert!(w3_formats::save::parse_save(half).is_err());
}
