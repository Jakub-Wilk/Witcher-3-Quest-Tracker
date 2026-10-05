//! Smart sync between the Witcher wiki (`quest_scraper`) and the local DB (`quest_db`).
//!
//! Syncing is split in two so no DB connection is held across `.await`:
//! - [`fetch_remote`] scrapes the wiki (async, no DB access).
//! - [`merge`] writes the scraped data in one transaction, updating static quest metadata
//!   and links while never touching per-playthrough progress.

use std::collections::BTreeMap;

use quest_db::{Connection, QuestSource, quests};
use quest_scraper::{ScrapedQuestSource, ScrapedQuestStore, ScrapedQuestType, WikiScraperClient};

/// Wiki categories to sync, with the quest type / source each one implies.
const CATEGORIES: &[(&str, Option<ScrapedQuestType>, Option<ScrapedQuestSource>)] = &[
    ("Category:The_Witcher_3_main_quests", Some(ScrapedQuestType::MainQuest), None),
    ("Category:The_Witcher_3_secondary_quests", Some(ScrapedQuestType::SecondaryQuest), None),
    ("Category:The_Witcher_3_contracts", Some(ScrapedQuestType::WitcherContract), None),
    ("Category:The_Witcher_3_treasure_hunts", Some(ScrapedQuestType::TreasureHunt), None),
    ("Category:Hearts_of_Stone_quests", None, Some(ScrapedQuestSource::HeartsOfStone)),
    ("Category:Blood_and_Wine_quests", None, Some(ScrapedQuestSource::BloodAndWine)),
];

/// Result of scraping the wiki.
#[derive(Debug, Default)]
pub struct RemoteQuests {
    pub store: ScrapedQuestStore,
    /// `(page title, error)` for every page that could not be scraped.
    pub failed: Vec<(String, String)>,
}

/// Summary of a [`merge`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub inserted: usize,
    pub updated: usize,
    pub links_resolved: usize,
    /// Cutoff / prerequisite names that matched no quest in the DB.
    pub unresolved: Vec<String>,
}

/// Scrapes every quest page in [`CATEGORIES`], reporting `(done, total)` page progress.
///
/// Fails only if the category listings cannot be fetched; individual page failures are
/// collected in [`RemoteQuests::failed`].
pub async fn fetch_remote(on_progress: impl FnMut(usize, usize)) -> Result<RemoteQuests, String> {
    let client = WikiScraperClient::new().map_err(|e| e.to_string())?;

    // title -> (type hint, source hint), merged across categories
    let mut hints: BTreeMap<String, (Option<ScrapedQuestType>, Option<ScrapedQuestSource>)> =
        BTreeMap::new();
    for &(category, quest_type, source) in CATEGORIES {
        let titles = client
            .fetch_category_member_titles(category)
            .await
            .map_err(|e| format!("Failed to list {category}: {e}"))?;
        for title in titles.into_iter().filter(|t| !is_overview_page(t)) {
            let entry = hints.entry(title).or_default();
            entry.0 = entry.0.or(quest_type);
            entry.1 = entry.1.or(source);
        }
    }

    if hints.is_empty() {
        return Err("The wiki returned no quest pages".into());
    }

    let titles: Vec<&str> = hints.keys().map(String::as_str).collect();
    let batch = client.fetch_quests_batch_with_progress(&titles, on_progress).await;

    // Category membership is a stronger signal than the parser's text heuristics.
    let mut store = ScrapedQuestStore::default();
    for mut quest in batch.store.quests().iter().cloned() {
        let page_title = quest.name.replace(' ', "_");
        let (quest_type, source) = hints
            .get(&quest.name)
            .or_else(|| hints.get(&page_title))
            .copied()
            .unwrap_or_default();
        if let Some(t) = quest_type {
            quest.quest_type = t;
        }
        if let Some(s) = source {
            quest.source = s;
        }
        quest.name = canonical_name(&quest.name);
        store.push(quest);
    }

    Ok(RemoteQuests { store, failed: batch.failed })
}

/// Merges scraped quests into the DB in a single transaction.
///
/// Pass 1 inserts new quests and updates metadata of existing ones (matched by name + source).
/// Pass 2 resolves cutoff and prerequisite names to quest IDs, across all sources.
/// User progress (`quest_progress`) is never modified.
pub fn merge(conn: &mut Connection, store: &ScrapedQuestStore) -> quest_db::Result<SyncReport> {
    let tx = conn.transaction()?;
    let mut report = SyncReport::default();

    let mut ids = Vec::with_capacity(store.len());
    for scraped in store.quests() {
        let new_quest = scraped.clone().into_new_quest(None, vec![]);
        match quests::find_by_name(&tx, &new_quest.name, new_quest.source)? {
            Some(existing) if existing.source == new_quest.source => {
                quests::update_metadata(&tx, existing.id, &new_quest)?;
                report.updated += 1;
                ids.push(existing.id);
            }
            _ => {
                ids.push(quests::insert(&tx, &new_quest)?);
                report.inserted += 1;
            }
        }
    }

    for (scraped, &id) in store.quests().iter().zip(&ids) {
        let source: QuestSource = scraped.source.into();
        let mut resolve = |name: &str| -> quest_db::Result<Option<i64>> {
            let found = quests::find_by_name(&tx, &canonical_name(name), source)?
                .map(|q| q.id)
                .filter(|&found_id| found_id != id);
            match found {
                Some(_) => report.links_resolved += 1,
                None => {
                    if !report.unresolved.iter().any(|u| u == name) {
                        report.unresolved.push(name.to_string());
                    }
                }
            }
            Ok(found)
        };

        let cutoff = match &scraped.cutoff_quest_name {
            Some(name) => resolve(name)?,
            None => None,
        };
        let mut prereqs = Vec::new();
        for name in &scraped.prerequisite_quest_names {
            if let Some(prereq_id) = resolve(name)? {
                if !prereqs.contains(&prereq_id) {
                    prereqs.push(prereq_id);
                }
            }
        }

        quests::set_cutoff(&tx, id, cutoff)?;
        quests::set_prerequisites(&tx, id, &prereqs)?;
    }

    tx.commit()?;
    Ok(report)
}

/// Strips wiki disambiguation suffixes, e.g. "Bloody Baron (quest)" -> "Bloody Baron".
fn canonical_name(name: &str) -> String {
    let name = name.replace('_', " ");
    let trimmed = name.trim();
    match trimmed.rfind(" (") {
        Some(idx) if trimmed.ends_with(')') => trimmed[..idx].trim().to_string(),
        _ => trimmed.to_string(),
    }
}

/// Category listings include overview pages such as "Hearts of Stone quests".
fn is_overview_page(title: &str) -> bool {
    let lower = title.to_lowercase();
    ["quests", "contracts", "treasure hunts", "scavenger hunts"]
        .iter()
        .any(|suffix| lower.ends_with(suffix))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_db::{
        NewPlaythrough, Difficulty, QuestProgressUpdate, QuestStatus, QuestType, Region, open_in_memory,
        playthroughs, progress,
    };
    use quest_scraper::{ScrapedQuest, ScrapedRegion, mock_sample_quests};

    fn scraped(name: &str, cutoff: Option<&str>, prereqs: &[&str]) -> ScrapedQuest {
        ScrapedQuest {
            name: name.into(),
            source: ScrapedQuestSource::BaseGame,
            quest_type: ScrapedQuestType::MainQuest,
            region: ScrapedRegion::Velen,
            recommended_level: Some(10),
            is_unmarked: false,
            sort_order: None,
            description: None,
            cutoff_quest_name: cutoff.map(Into::into),
            prerequisite_quest_names: prereqs.iter().map(|s| s.to_string()).collect(),
            wiki_url: String::new(),
        }
    }

    #[test]
    fn merge_inserts_resolves_links_and_preserves_progress() {
        let mut conn = open_in_memory().unwrap();
        let mut store = mock_sample_quests();
        store.push(scraped("Isle of Mists", None, &[]));
        store.push(scraped("Nameless", None, &[]));

        let report = merge(&mut conn, &store).unwrap();
        assert_eq!(report.inserted, 5);
        assert_eq!(report.updated, 0);
        // "Now or Never" and "Pyres of Novigrad" are not in the store
        assert_eq!(report.unresolved, vec!["Now or Never", "Pyres of Novigrad"]);

        let last_wish = quests::get_by_name(&conn, "The Last Wish", &QuestSource::BaseGame).unwrap();
        let isle = quests::get_by_name(&conn, "Isle of Mists", &QuestSource::BaseGame).unwrap();
        let nameless = quests::get_by_name(&conn, "Nameless", &QuestSource::BaseGame).unwrap();
        assert_eq!(last_wish.cutoff_quest_id, Some(isle.id));
        assert_eq!(last_wish.prerequisite_ids, vec![nameless.id]);

        // Record user progress, then re-sync with changed metadata
        let pt = playthroughs::insert(
            &conn,
            &NewPlaythrough {
                name: "Run".into(),
                difficulty: Difficulty::DeathMarch,
                is_new_game_plus: false,
                notes: None,
            },
        )
        .unwrap();
        progress::update_status(
            &conn,
            pt,
            last_wish.id,
            &QuestProgressUpdate {
                status: Some(QuestStatus::Completed),
                notes: Some(Some("Kept the bond".into())),
                ..Default::default()
            },
        )
        .unwrap();

        let mut store2 = ScrapedQuestStore::default();
        for q in store.quests() {
            let mut q = q.clone();
            if q.name == "The Last Wish" {
                q.recommended_level = Some(18);
                q.region = ScrapedRegion::Skellige;
                q.prerequisite_quest_names.clear();
            }
            store2.push(q);
        }
        let report2 = merge(&mut conn, &store2).unwrap();
        assert_eq!(report2.inserted, 0);
        assert_eq!(report2.updated, 5);

        let last_wish = quests::get(&conn, last_wish.id).unwrap();
        assert_eq!(last_wish.recommended_level, Some(18));
        assert_eq!(last_wish.region, Region::Skellige);
        assert!(last_wish.prerequisite_ids.is_empty());
        assert_eq!(last_wish.cutoff_quest_id, Some(isle.id));

        let prog = progress::get(&conn, pt, last_wish.id).unwrap();
        assert_eq!(prog.status, QuestStatus::Completed);
        assert_eq!(prog.notes.as_deref(), Some("Kept the bond"));
    }

    #[test]
    fn merge_resolves_links_across_sources() {
        let mut conn = open_in_memory().unwrap();
        let mut hos = scraped("Evil's Soft First Touches", None, &["Bloody Baron (quest)"]);
        hos.source = ScrapedQuestSource::HeartsOfStone;
        let store = ScrapedQuestStore::new(vec![scraped("Bloody Baron", None, &[]), hos]);

        let report = merge(&mut conn, &store).unwrap();
        assert!(report.unresolved.is_empty());

        let baron = quests::get_by_name(&conn, "Bloody Baron", &QuestSource::BaseGame).unwrap();
        let evil = quests::get_by_name(&conn, "Evil's Soft First Touches", &QuestSource::HeartsOfStone)
            .unwrap();
        assert_eq!(evil.prerequisite_ids, vec![baron.id]);
    }

    #[test]
    fn canonical_name_strips_disambiguation() {
        assert_eq!(canonical_name("Bloody Baron (quest)"), "Bloody Baron");
        assert_eq!(canonical_name("Open_Sesame!_(Hearts_of_Stone)"), "Open Sesame!");
        assert_eq!(canonical_name("Contract: Dragon"), "Contract: Dragon");
    }

    #[test]
    fn overview_pages_are_skipped() {
        assert!(is_overview_page("Hearts of Stone quests"));
        assert!(!is_overview_page("A Dark Legacy"));
    }

    /// Full live sync against the wiki. Run with `cargo test -- --ignored --nocapture live_sync`.
    #[tokio::test]
    #[ignore = "hits the live Witcher wiki"]
    async fn live_sync() {
        let remote = fetch_remote(|_, _| {}).await.expect("fetch failed");
        let mut conn = open_in_memory().unwrap();
        let report = merge(&mut conn, &remote.store).unwrap();

        println!("scraped: {}, failed: {}", remote.store.len(), remote.failed.len());
        for (title, err) in remote.failed.iter().take(10) {
            println!("  failed {title}: {err}");
        }
        println!("{report:#?}");
        let all = quests::list(&conn, &Default::default()).unwrap();
        let with_cutoff = all.iter().filter(|q| q.cutoff_quest_id.is_some()).count();
        let with_prereqs = all.iter().filter(|q| !q.prerequisite_ids.is_empty()).count();
        println!("quests: {}, with cutoff: {with_cutoff}, with prereqs: {with_prereqs}", all.len());
        for t in [QuestType::MainQuest, QuestType::SecondaryQuest, QuestType::WitcherContract, QuestType::TreasureHunt] {
            println!("  {t:?}: {}", all.iter().filter(|q| q.quest_type == t).count());
        }
        for s in [QuestSource::BaseGame, QuestSource::HeartsOfStone, QuestSource::BloodAndWine] {
            println!("  {s:?}: {}", all.iter().filter(|q| q.source == s).count());
        }
        assert!(report.inserted > 300);
    }
}
