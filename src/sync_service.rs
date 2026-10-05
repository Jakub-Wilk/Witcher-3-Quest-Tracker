//! Smart sync between the Witcher wiki (`quest_scraper`) and the local DB (`quest_db`).
//!
//! Syncing is split in two so no DB connection is held across `.await`:
//! - [`fetch_remote`] scrapes the wiki (async, no DB access, all-or-nothing).
//! - [`merge`] writes the result in one transaction. Quests are matched by wiki page id, so
//!   renames on the wiki never duplicate quests or orphan progress; per-playthrough progress
//!   is never modified.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use quest_db::{Connection, NewQuest, QuestSource, quests, settings};
use quest_scraper::{Language, ScrapeProgress, ScrapeResult, ScrapedQuest, WikiScraperClient};

/// Settings key holding the language of the last sync.
pub const SYNC_LANGUAGE_KEY: &str = "sync_language";

/// Summary of a [`merge`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub inserted: usize,
    pub updated: usize,
    /// Quests no longer on the wiki that were deleted (nobody had progress on them).
    pub removed: usize,
    /// Quests no longer on the wiki that were kept because some playthrough has progress.
    pub kept_stale: Vec<String>,
    /// Cutoff / previous-quest link targets that matched no synced quest.
    pub unresolved: Vec<String>,
    pub language: Language,
    /// Quests that got a localized title.
    pub translated: usize,
}

impl SyncReport {
    /// One-line summary for the UI.
    pub fn summary(&self) -> String {
        let mut msg = format!(
            "Synced {} quests ({} new, {} updated",
            self.inserted + self.updated,
            self.inserted,
            self.updated
        );
        if self.removed > 0 {
            msg.push_str(&format!(", {} removed", self.removed));
        }
        msg.push_str(").");
        if self.language != Language::English {
            msg.push_str(&format!(
                " {}: {}/{} titles translated.",
                self.language.native_label(),
                self.translated,
                self.inserted + self.updated
            ));
        }
        if !self.kept_stale.is_empty() {
            msg.push_str(&format!(
                " {} quests are no longer on the wiki but were kept because they have progress.",
                self.kept_stale.len()
            ));
        }
        msg
    }
}

/// Scrapes every quest from the wiki, with titles in `language`.
pub async fn fetch_remote(
    language: Language,
    on_progress: impl FnMut(ScrapeProgress),
) -> Result<ScrapeResult, String> {
    let client = WikiScraperClient::new().map_err(|e| e.to_string())?;
    let result = client.fetch_all_quests(language, on_progress).await.map_err(|e| e.to_string())?;
    if result.quests.is_empty() {
        return Err("The wiki returned no quests".into());
    }
    Ok(result)
}

/// Merges a scrape into the DB in a single transaction:
/// 1. insert or update every quest, matched by wiki page id;
/// 2. resolve cutoff / previous-quest links through the scrape's title and redirect aliases;
/// 3. compute the story `sort_order`;
/// 4. delete quests that disappeared from the wiki, unless someone has progress on them;
/// 5. remember the sync language.
pub fn merge(conn: &mut Connection, scrape: &ScrapeResult) -> quest_db::Result<SyncReport> {
    let tx = conn.transaction()?;
    let mut report = SyncReport { language: scrape.language, ..Default::default() };

    // 1. Upsert
    let mut db_ids: HashMap<i64, i64> = HashMap::new(); // page id -> quest id
    for scraped in &scrape.quests {
        let new_quest = to_new_quest(scraped);
        let id = match quests::get_id_by_page_id(&tx, scraped.page_id)? {
            Some(id) => {
                quests::update(&tx, id, &new_quest)?;
                report.updated += 1;
                id
            }
            None => {
                report.inserted += 1;
                quests::insert(&tx, &new_quest)?
            }
        };
        db_ids.insert(scraped.page_id, id);
    }
    report.translated = scrape.translated_count();

    // 2. Links
    let mut resolve = |title: &str, self_id: i64| -> Option<i64> {
        let found = scrape.resolve(title).and_then(|page_id| db_ids.get(&page_id).copied());
        if found.is_none() && !report.unresolved.iter().any(|u| u == title) {
            report.unresolved.push(title.to_string());
        }
        found.filter(|&id| id != self_id)
    };
    let mut prerequisites: HashMap<i64, Vec<i64>> = HashMap::new();
    for scraped in &scrape.quests {
        let id = db_ids[&scraped.page_id];
        let cutoff = scraped.cutoff_titles.iter().find_map(|t| resolve(t, id));
        let mut prereqs: Vec<i64> = Vec::new();
        for title in &scraped.previous_titles {
            if let Some(prereq) = resolve(title, id) {
                if !prereqs.contains(&prereq) {
                    prereqs.push(prereq);
                }
            }
        }
        quests::set_cutoff(&tx, id, cutoff)?;
        quests::set_prerequisites(&tx, id, &prereqs)?;
        prerequisites.insert(id, prereqs);
    }

    // 3. Sort order
    let nodes: Vec<SortNode> = scrape
        .quests
        .iter()
        .map(|q| SortNode {
            id: db_ids[&q.page_id],
            source: q.source,
            level: q.recommended_level,
            name: q.name.clone(),
        })
        .collect();
    for (position, id) in story_order(&nodes, &prerequisites).into_iter().enumerate() {
        quests::set_sort_order(&tx, id, Some(position as i32))?;
    }

    // 4. Stale quests
    let seen: HashSet<i64> = db_ids.keys().copied().collect();
    let missing = quests::delete_missing(&tx, &seen)?;
    report.removed = missing.deleted;
    report.kept_stale = missing.kept;

    // 5. Language
    settings::set(&tx, SYNC_LANGUAGE_KEY, scrape.language.code())?;

    tx.commit()?;
    Ok(report)
}

fn to_new_quest(q: &ScrapedQuest) -> NewQuest {
    NewQuest {
        wiki_page_id: q.page_id,
        wiki_title: q.wiki_title.clone(),
        name: q.name.clone(),
        localized_name: q.localized_name.clone(),
        source: q.source,
        quest_type: q.quest_type,
        region: q.region,
        recommended_level: q.recommended_level,
        sort_order: None,
        description: q.description.clone(),
        important_notes: q.important_notes.clone(),
        is_unmarked: q.is_unmarked,
        cutoff_quest_id: None,
        prerequisite_ids: vec![],
    }
}

struct SortNode {
    id: i64,
    source: QuestSource,
    level: Option<i32>,
    name: String,
}

/// Orders quests roughly as a player meets them: by expansion, then by effective level, then
/// by depth in the previous-quest chain, then by name. The effective level never drops below
/// a prerequisite's; quests with no level anywhere upstream borrow the lowest level of the
/// quests they lead into. Prerequisite cycles (rare wiki inconsistencies) are broken by
/// processing the remaining quests in name order.
fn story_order(nodes: &[SortNode], prerequisites: &HashMap<i64, Vec<i64>>) -> Vec<i64> {
    let index: HashMap<i64, usize> = nodes.iter().enumerate().map(|(i, n)| (n.id, i)).collect();
    let prereqs_of = |i: usize| -> Vec<usize> {
        prerequisites
            .get(&nodes[i].id)
            .map(|ps| ps.iter().filter_map(|p| index.get(p).copied()).collect())
            .unwrap_or_default()
    };

    // Kahn's algorithm, choosing among ready quests by name for determinism.
    let mut remaining: Vec<usize> = (0..nodes.len()).map(|i| prereqs_of(i).len()).collect();
    let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); nodes.len()];
    for i in 0..nodes.len() {
        for p in prereqs_of(i) {
            dependents[p].push(i);
        }
    }
    let mut ready: BinaryHeap<Reverse<(&str, usize)>> = (0..nodes.len())
        .filter(|&i| remaining[i] == 0)
        .map(|i| Reverse((nodes[i].name.as_str(), i)))
        .collect();
    let mut topo = Vec::with_capacity(nodes.len());
    let mut done = vec![false; nodes.len()];
    while topo.len() < nodes.len() {
        let next = match ready.pop() {
            Some(Reverse((_, i))) => i,
            // Cycle: force the alphabetically first unfinished quest.
            None => (0..nodes.len())
                .filter(|&i| !done[i])
                .min_by(|&a, &b| nodes[a].name.cmp(&nodes[b].name))
                .unwrap(),
        };
        if done[next] {
            continue;
        }
        done[next] = true;
        topo.push(next);
        for &d in &dependents[next] {
            remaining[d] = remaining[d].saturating_sub(1);
            if remaining[d] == 0 && !done[d] {
                ready.push(Reverse((nodes[d].name.as_str(), d)));
            }
        }
    }

    // Forward pass: depth in the chain, and a level that never drops below a prerequisite's.
    let mut depth = vec![0usize; nodes.len()];
    let mut level: Vec<Option<i32>> = nodes.iter().map(|n| n.level).collect();
    for &i in &topo {
        let prereqs = prereqs_of(i);
        depth[i] = prereqs.iter().map(|&p| depth[p] + 1).max().unwrap_or(0);
        let inherited = prereqs.iter().filter_map(|&p| level[p]).max();
        level[i] = level[i].max(inherited);
    }
    // Backward pass: quests still without a level (e.g. the prologue) take the lowest level
    // of the quests they lead into.
    for &i in topo.iter().rev() {
        if level[i].is_none() {
            level[i] = dependents[i].iter().filter_map(|&d| level[d]).min();
        }
    }

    let source_rank = |s: QuestSource| match s {
        QuestSource::BaseGame => 0,
        QuestSource::HeartsOfStone => 1,
        QuestSource::BloodAndWine => 2,
    };
    let mut order: Vec<usize> = (0..nodes.len()).collect();
    order.sort_by(|&a, &b| {
        (source_rank(nodes[a].source), level[a].unwrap_or(i32::MAX), depth[a], &nodes[a].name).cmp(&(
            source_rank(nodes[b].source),
            level[b].unwrap_or(i32::MAX),
            depth[b],
            &nodes[b].name,
        ))
    });
    order.into_iter().map(|i| nodes[i].id).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quest_db::{
        Difficulty, NewPlaythrough, QuestProgressUpdate, QuestStatus, QuestType, Region,
        open_in_memory, playthroughs, progress,
    };
    use quest_scraper::mock_scrape_result;

    fn id_of(conn: &Connection, page_id: i64) -> i64 {
        quests::get_id_by_page_id(conn, page_id).unwrap().unwrap()
    }

    fn new_playthrough(conn: &Connection) -> i64 {
        playthroughs::insert(
            conn,
            &NewPlaythrough {
                name: "Run".into(),
                difficulty: Difficulty::DeathMarch,
                is_new_game_plus: false,
                notes: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn merge_inserts_and_resolves_links_through_aliases() {
        let mut conn = open_in_memory().unwrap();
        let report = merge(&mut conn, &mock_scrape_result()).unwrap();
        assert_eq!(report.inserted, 5);
        assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

        let last_wish = quests::get(&conn, id_of(&conn, 1002)).unwrap();
        assert_eq!(last_wish.name, "The Last Wish");
        assert_eq!(last_wish.cutoff_quest_id, Some(id_of(&conn, 1003)));
        assert_eq!(last_wish.prerequisite_ids, vec![id_of(&conn, 1001)]);
        assert!(last_wish.important_notes.is_some());

        // "Isle of Mists" is a redirect alias of page 1004
        let raids = quests::get(&conn, id_of(&conn, 1005)).unwrap();
        assert_eq!(raids.cutoff_quest_id, Some(id_of(&conn, 1004)));
    }

    #[test]
    fn remerge_updates_metadata_and_preserves_progress() {
        let mut conn = open_in_memory().unwrap();
        merge(&mut conn, &mock_scrape_result()).unwrap();
        let last_wish = id_of(&conn, 1002);

        let pt = new_playthrough(&conn);
        progress::update_status(
            &conn,
            pt,
            last_wish,
            &QuestProgressUpdate {
                status: Some(QuestStatus::Completed),
                notes: Some(Some("Kept the bond".into())),
                ..Default::default()
            },
        )
        .unwrap();

        // The wiki renamed the page and changed some data; page id stays the same.
        let mut scrape = mock_scrape_result();
        let q = scrape.quests.iter_mut().find(|q| q.page_id == 1002).unwrap();
        q.wiki_title = "The Last Wish (Witcher 3 quest)".into();
        q.recommended_level = Some(18);
        q.region = Region::Novigrad;
        q.previous_titles.clear();

        let report = merge(&mut conn, &scrape).unwrap();
        assert_eq!((report.inserted, report.updated, report.removed), (0, 5, 0));
        assert_eq!(id_of(&conn, 1002), last_wish);

        let quest = quests::get(&conn, last_wish).unwrap();
        assert_eq!(quest.wiki_title, "The Last Wish (Witcher 3 quest)");
        assert_eq!(quest.recommended_level, Some(18));
        assert_eq!(quest.region, Region::Novigrad);
        assert!(quest.prerequisite_ids.is_empty());

        let prog = progress::get(&conn, pt, last_wish).unwrap();
        assert_eq!(prog.status, QuestStatus::Completed);
        assert_eq!(prog.notes.as_deref(), Some("Kept the bond"));
    }

    #[test]
    fn merge_removes_stale_quests_unless_they_have_progress() {
        let mut conn = open_in_memory().unwrap();
        merge(&mut conn, &mock_scrape_result()).unwrap();
        let pt = new_playthrough(&conn);
        progress::update_status(
            &conn,
            pt,
            id_of(&conn, 1002),
            &QuestProgressUpdate { status: Some(QuestStatus::Failed), ..Default::default() },
        )
        .unwrap();

        let mut scrape = mock_scrape_result();
        scrape.quests.retain(|q| q.page_id != 1002 && q.page_id != 1005);
        let report = merge(&mut conn, &scrape).unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(report.kept_stale, vec!["The Last Wish"]);
        assert!(quests::get_id_by_page_id(&conn, 1005).unwrap().is_none());
        assert!(quests::get_id_by_page_id(&conn, 1002).unwrap().is_some());
    }

    #[test]
    fn merge_stores_localized_names_and_language() {
        let mut conn = open_in_memory().unwrap();
        let mut scrape = mock_scrape_result();
        scrape.language = Language::Polish;
        scrape.quests[1].localized_name = Some("Ostatnie życzenie".into());

        let report = merge(&mut conn, &scrape).unwrap();
        assert_eq!(report.translated, 1);
        assert_eq!(settings::get(&conn, SYNC_LANGUAGE_KEY).unwrap().as_deref(), Some("pl"));
        assert_eq!(quests::get(&conn, id_of(&conn, 1002)).unwrap().display_name(), "Ostatnie życzenie");

        // Re-syncing in English clears the translation.
        merge(&mut conn, &mock_scrape_result()).unwrap();
        assert_eq!(quests::get(&conn, id_of(&conn, 1002)).unwrap().display_name(), "The Last Wish");
        assert_eq!(settings::get(&conn, SYNC_LANGUAGE_KEY).unwrap().as_deref(), Some("en"));
    }

    #[test]
    fn story_order_uses_source_level_and_chain_depth() {
        let node = |id, source, level: Option<i32>, name: &str| SortNode { id, source, level, name: name.into() };
        let nodes = vec![
            node(1, QuestSource::BloodAndWine, Some(35), "Toussaint start"),
            node(2, QuestSource::BaseGame, Some(10), "Later"),
            node(3, QuestSource::BaseGame, Some(1), "Prologue"),
            node(4, QuestSource::BaseGame, None, "Follow-up without level"),
            node(5, QuestSource::BaseGame, Some(1), "A same-level sequel"),
        ];
        let prereqs = HashMap::from([(4, vec![3]), (5, vec![3])]);
        // 4 inherits level 1 from 3; 5 has depth 1 so it sorts after 3 despite its name.
        assert_eq!(story_order(&nodes, &prereqs), vec![3, 5, 4, 2, 1]);
    }

    #[test]
    fn story_order_fills_missing_levels_from_both_directions() {
        let node = |id, level: Option<i32>, name: &str| SortNode { id, source: QuestSource::BaseGame, level, name: name.into() };
        // Mirrors the real prologue: Kaer Morhen -> Lilac and Gooseberries (no levels)
        // -> Imperial Audience (2) -> Pyres of Novigrad (10); a sequel listing a lower level.
        let nodes = vec![
            node(1, None, "Kaer Morhen"),
            node(2, None, "Lilac and Gooseberries"),
            node(3, Some(2), "Imperial Audience"),
            node(4, Some(10), "Pyres of Novigrad"),
            node(5, Some(1), "A sequel listing a lower level"),
            node(6, Some(5), "Unrelated"),
        ];
        let prereqs = HashMap::from([(2, vec![1]), (3, vec![2]), (4, vec![3]), (5, vec![4])]);
        assert_eq!(story_order(&nodes, &prereqs), vec![1, 2, 3, 6, 4, 5]);
    }

    #[test]
    fn story_order_survives_cycles() {
        let node = |id, name: &str| SortNode { id, source: QuestSource::BaseGame, level: Some(1), name: name.into() };
        let nodes = vec![node(1, "A"), node(2, "B")];
        let prereqs = HashMap::from([(1, vec![2]), (2, vec![1])]);
        let order = story_order(&nodes, &prereqs);
        assert_eq!(order.len(), 2);
    }

    /// Full live sync. Run with `cargo test -- --ignored --nocapture live_sync`.
    async fn live(language: Language) -> (Connection, ScrapeResult, SyncReport) {
        let started = std::time::Instant::now();
        let scrape = fetch_remote(language, |_| {}).await.expect("fetch failed");
        let mut conn = open_in_memory().unwrap();
        let report = merge(&mut conn, &scrape).unwrap();
        println!("{language:?} sync took {:.1?}", started.elapsed());
        println!("skipped: {:?}", scrape.skipped);
        println!("{report:#?}");
        (conn, scrape, report)
    }

    #[tokio::test]
    #[ignore = "hits the live Witcher wiki"]
    async fn live_sync() {
        let (conn, _, report) = live(Language::English).await;
        let all = quests::list(&conn, &Default::default()).unwrap();
        let count = |f: &dyn Fn(&quest_db::Quest) -> bool| all.iter().filter(|q| f(q)).count();

        println!("quests: {}", all.len());
        for t in [QuestType::MainQuest, QuestType::SecondaryQuest, QuestType::WitcherContract, QuestType::TreasureHunt, QuestType::ScavengerHunt] {
            println!("  {t:?}: {}", count(&|q| q.quest_type == t));
        }
        for s in [QuestSource::BaseGame, QuestSource::HeartsOfStone, QuestSource::BloodAndWine] {
            println!("  {s:?}: {}", count(&|q| q.source == s));
        }
        for r in [Region::WhiteOrchard, Region::Velen, Region::Novigrad, Region::Oxenfurt, Region::Skellige, Region::KaerMorhen, Region::Vizima, Region::Toussaint, Region::Unknown] {
            println!("  {r:?}: {}", count(&|q| q.region == r));
        }
        let unmarked = count(&|q| q.is_unmarked);
        let cutoffs = count(&|q| q.cutoff_quest_id.is_some());
        let important = count(&|q| q.important_notes.is_some());
        let described = count(&|q| q.description.is_some());
        println!("unmarked: {unmarked}, cutoffs: {cutoffs}, important: {important}, described: {described}");
        let main: Vec<_> = all.iter().filter(|q| q.quest_type == QuestType::MainQuest).take(12).map(|q| q.name.as_str()).collect();
        println!("first main quests: {main:?}");

        assert!(all.len() > 400);
        assert!(count(&|q| q.region == Region::Unknown) * 20 < all.len());
        assert!(cutoffs >= 40);
        assert!(unmarked >= 25);
        assert_eq!(report.inserted, all.len());
    }

    #[tokio::test]
    #[ignore = "hits the live Witcher wiki"]
    async fn live_sync_polish() {
        let (conn, scrape, report) = live(Language::Polish).await;
        assert!(report.translated * 10 >= scrape.quests.len() * 9, "translated {}", report.translated);
        let all = quests::list(&conn, &Default::default()).unwrap();
        let last_wish = all.iter().find(|q| q.name == "The Last Wish").unwrap();
        assert_eq!(last_wish.display_name(), "Ostatnie życzenie");
    }
}
