//! Full quest data sync: the game's own quest catalog, enriched with wiki data.
//!
//! Syncing is split so no DB connection is held across `.await`:
//! - [`fetch`] reads the game files (blocking task) and scrapes the wiki (async). Nothing is
//!   written. A wiki failure is not fatal: the game data alone is still a full catalog.
//! - [`merge`] writes the result in one transaction. Quests are keyed by journal path, the same
//!   key saves use; wiki pages are matched to quests by English title. Per-playthrough progress
//!   is never modified.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::PathBuf;

use quest_db::{Connection, NewQuest, QuestSource, QuestText, QuestType, Region, quests};
use quest_scraper::{Language, ScrapeProgress, ScrapeResult, ScrapedQuest, WikiScraperClient};
use w3_formats::{Content, GameCatalog, GameQuest, JournalQuestType};

/// What a running sync is doing, for the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SyncPhase {
    ReadingGame,
    Wiki(ScrapeProgress),
    Merging,
}

/// Everything [`fetch`] gathered.
pub struct Fetched {
    pub catalog: GameCatalog,
    /// `Err` holds why the wiki could not be scraped.
    pub wiki: Result<ScrapeResult, String>,
}

/// Summary of a [`merge`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    pub inserted: usize,
    pub updated: usize,
    /// Quests no longer in the game data that were deleted (nobody had notes or a manual status).
    pub removed: usize,
    /// Quests no longer in the game data that were kept because a user set a status or notes.
    pub kept_stale: Vec<String>,
    /// Languages with quest text.
    pub languages: usize,
    /// Quests that got wiki data.
    pub wiki_matched: usize,
    /// Wiki quest names that matched no game quest (added as wiki-only quests).
    pub unmatched_wiki: Vec<String>,
    /// Quests added from the wiki alone: no journal entry, so not readable from saves.
    pub wiki_only: usize,
    /// English titles of game quests without a wiki page.
    pub unmatched_game: Vec<String>,
    /// Journal files left out via [`EXCLUDED_JOURNALS`].
    pub excluded: usize,
    /// Journal files folded into another file's quest row (same wiki page).
    pub folded: usize,
    /// Cutoff / previous-quest link targets that matched no quest.
    pub unresolved: Vec<String>,
    /// Set when the wiki could not be scraped and its previous data was kept.
    pub wiki_error: Option<String>,
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
        msg.push_str(&format!(") in {} languages.", self.languages));
        match &self.wiki_error {
            Some(e) => msg.push_str(&format!(" The wiki could not be reached, so its previous data was kept ({e}).")),
            None => msg.push_str(&format!(
                " Wiki data for {} quests, plus {} unmarked quests from the wiki that you tick off yourself.",
                self.wiki_matched, self.wiki_only
            )),
        }
        if !self.kept_stale.is_empty() {
            msg.push_str(&format!(
                " {} quests are no longer in the game data but were kept because they have your notes or status.",
                self.kept_stale.len()
            ));
        }
        msg
    }
}

/// Reads the game catalog from `game_dir`, then scrapes the wiki.
pub async fn fetch(game_dir: PathBuf, mut on_phase: impl FnMut(SyncPhase)) -> Result<Fetched, String> {
    on_phase(SyncPhase::ReadingGame);
    let catalog = tokio::task::spawn_blocking(move || w3_formats::read_game_catalog(&game_dir))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| format!("Could not read the game data: {e}"))?;
    for warning in &catalog.warnings {
        tracing::warn!("Game data: {warning}");
    }
    if catalog.quests.is_empty() {
        return Err("The game data contains no quests".into());
    }

    let wiki = async {
        let client = WikiScraperClient::new().map_err(|e| e.to_string())?;
        let result = client
            .fetch_all_quests(Language::English, |p| on_phase(SyncPhase::Wiki(p)))
            .await
            .map_err(|e| e.to_string())?;
        if result.quests.is_empty() {
            return Err("the wiki returned no quests".to_string());
        }
        Ok(result)
    }
    .await;
    if let Err(e) = &wiki {
        tracing::warn!("Wiki scrape failed: {e}");
    }
    Ok(Fetched { catalog, wiki })
}

/// Merges fetched data into the DB in a single transaction:
/// 1. match wiki pages to game quests by English title;
/// 2. insert or update every game quest (texts in all languages), keyed by journal path;
/// 3. resolve cutoff / previous-quest links;
/// 4. compute the story `sort_order`;
/// 5. delete quests that left the game data, unless a user has notes or a status on them.
///
/// Without wiki data, the wiki-derived fields of existing quests are kept as they were.
pub fn merge(conn: &mut Connection, fetched: &Fetched) -> quest_db::Result<SyncReport> {
    let tx = conn.transaction()?;
    let mut report = SyncReport { languages: fetched.catalog.languages.len(), ..Default::default() };
    let catalog = &without_excluded(&fetched.catalog, EXCLUDED_JOURNALS, &mut report);

    // 1. Wiki matching
    let wiki = fetched.wiki.as_ref().ok();
    report.wiki_error = fetched.wiki.as_ref().err().cloned();
    let matches = wiki.map(|w| match_wiki(catalog, w, WIKI_PAGE_OVERRIDES, &mut report)).unwrap_or_default();
    let previous: HashMap<String, quest_db::Quest> = if wiki.is_none() {
        quests::list(&tx, "en")?.into_iter().map(|q| (q.journal_path.clone(), q)).collect()
    } else {
        quests::clear_wiki_page_ids(&tx)?;
        HashMap::new()
    };

    // Files sharing a wiki page become one quest row, keyed by its main file.
    let mut members: HashMap<usize, Vec<usize>> = HashMap::new(); // wiki index -> game indices
    for (&i, &w) in &matches {
        members.entry(w).or_default().push(i);
    }
    let main_file: HashMap<usize, usize> = match wiki {
        Some(wk) => members.iter().map(|(&w, list)| (w, main_member(catalog, list, &wk.quests[w]))).collect(),
        None => HashMap::new(),
    };

    // 2. Upsert game quests
    let mut nodes: Vec<SortNode> = Vec::new();
    let mut linked: Vec<(i64, &ScrapedQuest)> = Vec::new(); // quests with wiki data, for links
    let mut by_page: HashMap<i64, i64> = HashMap::new(); // wiki page id -> quest id
    let mut seen: HashSet<String> = HashSet::new();
    for (i, game) in catalog.quests.iter().enumerate() {
        let page = matches.get(&i).copied();
        if page.is_some_and(|w| main_file[&w] != i) {
            report.folded += 1;
            continue;
        }
        let scraped = wiki.and_then(|wk| page.map(|w| &wk.quests[w]));
        let mut new_quest = to_new_quest(game, scraped);
        if let Some(w) = page {
            let mut extra: Vec<usize> = members[&w].iter().copied().filter(|&j| j != i).collect();
            extra.sort_unstable();
            new_quest.extra_journal_paths = extra.iter().map(|&j| catalog.quests[j].journal_path.clone()).collect();
        }
        if let Some(prev) = previous.get(&game.journal_path) {
            new_quest.wiki_page_id = prev.wiki_page_id;
            new_quest.wiki_title = prev.wiki_title.clone();
            new_quest.important_notes = prev.important_notes.clone();
            new_quest.is_unmarked = prev.is_unmarked;
            if prev.region != Region::Unknown {
                new_quest.region = prev.region;
            }
            if new_quest.recommended_level.is_none_or(|l| l == PLACEHOLDER_LEVEL) {
                new_quest.recommended_level = prev.recommended_level.or(new_quest.recommended_level);
            }
        }
        let id = upsert(&tx, &new_quest, &mut report)?;
        if let Some(page_id) = new_quest.wiki_page_id {
            by_page.insert(page_id, id);
        }
        if let Some(scraped) = scraped {
            linked.push((id, scraped));
        }
        nodes.push(SortNode::new(id, &new_quest, game.title("en").unwrap_or(&game.base_name), &game.group));
        seen.insert(new_quest.journal_path);
    }
    report.wiki_matched = members.len();

    // 2b. Wiki quests without a journal entry (mostly unmarked events): tracked by hand only.
    if let Some(wiki) = wiki {
        let matched: HashSet<usize> = matches.values().copied().collect();
        for (w, scraped) in wiki.quests.iter().enumerate().filter(|(w, _)| !matched.contains(w)) {
            let new_quest = wiki_only_quest(scraped);
            let id = upsert(&tx, &new_quest, &mut report)?;
            by_page.insert(scraped.page_id, id);
            linked.push((id, &wiki.quests[w]));
            nodes.push(SortNode::new(id, &new_quest, &scraped.name, ""));
            seen.insert(new_quest.journal_path);
            report.wiki_only += 1;
        }
    } else {
        // Offline: keep the wiki-only quests from the last sync as they are.
        for prev in previous.values().filter(|q| !q.is_trackable()) {
            nodes.push(SortNode {
                id: prev.id,
                source: prev.source,
                level: prev.recommended_level,
                stage: stage_of(prev.region, ""),
                main: prev.quest_type == QuestType::MainQuest,
                name: prev.title.clone(),
            });
            seen.insert(prev.journal_path.clone());
        }
    }

    // 3. Links
    let prerequisites: HashMap<i64, Vec<i64>> = if let Some(wiki) = wiki {
        let mut resolve = |title: &str, self_id: i64| -> Option<i64> {
            let found = wiki.resolve(title).and_then(|page_id| by_page.get(&page_id).copied());
            if found.is_none() && !report.unresolved.iter().any(|u| u == title) {
                report.unresolved.push(title.to_string());
            }
            found.filter(|&id| id != self_id)
        };
        let mut prerequisites = HashMap::new();
        let with_wiki: HashSet<i64> = linked.iter().map(|(id, _)| *id).collect();
        for &(id, scraped) in &linked {
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
        // Quests that lost their wiki match lose its links too.
        for node in nodes.iter().filter(|n| !with_wiki.contains(&n.id)) {
            quests::set_cutoff(&tx, node.id, None)?;
            quests::set_prerequisites(&tx, node.id, &[])?;
        }
        prerequisites
    } else {
        quests::all_prerequisites(&tx)?
    };

    // 4. Sort order
    for (position, id) in story_order(&nodes, &prerequisites).into_iter().enumerate() {
        quests::set_sort_order(&tx, id, Some(position as i32))?;
    }

    // 5. Stale quests
    let missing = quests::delete_missing(&tx, &seen)?;
    report.removed = missing.deleted;
    report.kept_stale = missing.kept;

    tx.commit()?;
    Ok(report)
}

fn upsert(tx: &Connection, quest: &NewQuest, report: &mut SyncReport) -> quest_db::Result<i64> {
    let (id, inserted) = quests::upsert(tx, quest)?;
    if inserted {
        report.inserted += 1;
    } else {
        report.updated += 1;
    }
    Ok(id)
}

/// Journal files that are not quests a player can finish, left out of the quest list so 100% is
/// reachable. None has a wiki page:
/// - cut content (the wiki's "The Witcher 3 cut content" page): the boat races and the second
///   enchanting level;
/// - entries a full 2018 playthrough never touched although it passed their quests: an umbrella
///   entry over the four "Brothers In Arms" quests, and three leftovers.
const EXCLUDED_JOURNALS: &[&str] = &[
    "gameplay\\journal\\quests\\br201kaertrolde.journal",
    "gameplay\\journal\\quests\\br202faroe.journal",
    "gameplay\\journal\\quests\\br301novigrad.journal",
    "gameplay\\journal\\quests\\br302oxenfurt.journal",
    "dlc\\ep1\\journal\\quests\\mq6005enchanterlevel2.journal",
    "gameplay\\journal\\quests\\q402gatheringallies.journal",
    "gameplay\\journal\\quests\\q401_ugliest_man_alive.journal",
    "gameplay\\journal\\quests\\lwprologuedeserters.journal",
    "gameplay\\journal\\quests\\mq2033deadmanschest.journal",
];

/// The catalog without the `excluded` journal files.
fn without_excluded(catalog: &GameCatalog, excluded: &[&str], report: &mut SyncReport) -> GameCatalog {
    let mut kept = catalog.clone();
    kept.quests.retain(|q| !excluded.contains(&q.journal_path.as_str()));
    report.excluded = catalog.quests.len() - kept.quests.len();
    kept
}

/// Of several journal files sharing a wiki page, the one the quest row is keyed by: a file whose
/// title is the page's (preferring the same expansion), else the first.
fn main_member(catalog: &GameCatalog, members: &[usize], page: &ScrapedQuest) -> usize {
    let keys = [normalize(&page.name), normalize(quest_scraper::wikitext::strip_disambiguator(&page.wiki_title))];
    let titled = |i: &&usize| keys.contains(&normalize(catalog.quests[**i].title("en").unwrap_or_default()));
    members
        .iter()
        .filter(titled)
        .min_by_key(|&&i| (source_of(catalog.quests[i].content) != page.source, i))
        .or_else(|| members.iter().min())
        .copied()
        .expect("a page always has a member")
}

/// Game quests whose wiki page cannot be found by title: (journal path, wiki page id). The file
/// is folded into the row of the page's other file(s).
const WIKI_PAGE_OVERRIDES: &[(&str, i64)] = &[
    // "Novigrad, Closed City II", the second half of "Novigrad, Closed City".
    ("gameplay\\journal\\quests\\q309novigradundercontrol2.journal", 24729),
];

/// Matches wiki quests to game quests by normalized English title, then extends matches to
/// same-titled journal files and applies [`WIKI_PAGE_OVERRIDES`]. Returns game quest index ->
/// wiki quest index; a wiki quest may serve several game quests. Ambiguous titles prefer the
/// same expansion.
fn match_wiki(
    catalog: &GameCatalog,
    wiki: &ScrapeResult,
    overrides: &[(&str, i64)],
    report: &mut SyncReport,
) -> HashMap<usize, usize> {
    let mut by_title: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, q) in catalog.quests.iter().enumerate() {
        if let Some(title) = q.title("en") {
            by_title.entry(normalize(title)).or_default().push(i);
        }
    }
    let mut matches: HashMap<usize, usize> = HashMap::new();
    for (w, scraped) in wiki.quests.iter().enumerate() {
        let candidates = [normalize(&scraped.name), normalize(quest_scraper::wikitext::strip_disambiguator(&scraped.wiki_title))]
            .into_iter()
            .find_map(|key| by_title.get(&key))
            .map(Vec::as_slice)
            .unwrap_or_default();
        let free = |i: &&usize| !matches.contains_key(*i);
        let pick = candidates
            .iter()
            .filter(free)
            .find(|&&i| source_of(catalog.quests[i].content) == scraped.source)
            .or_else(|| candidates.iter().find(free));
        match pick {
            Some(&i) => {
                matches.insert(i, w);
            }
            None => report.unmatched_wiki.push(scraped.name.clone()),
        }
    }

    // One quest the game splits over several journal files with the same title (epilogue
    // variants, a quest's second part, a separate intro): every file gets the page. Only when
    // exactly one wiki quest has that title, so two distinct quests are never conflated.
    let mut wiki_titles: HashMap<String, usize> = HashMap::new();
    for scraped in &wiki.quests {
        *wiki_titles.entry(normalize(&scraped.name)).or_default() += 1;
    }
    let primary: Vec<(usize, usize)> = matches.iter().map(|(&i, &w)| (i, w)).collect();
    for (i, w) in primary {
        let key = normalize(catalog.quests[i].title("en").unwrap_or_default());
        if wiki_titles.get(&key) != Some(&1) {
            continue;
        }
        for &j in by_title.get(&key).into_iter().flatten() {
            matches.entry(j).or_insert(w);
        }
    }

    // Journal files whose title differs from their wiki page's.
    for &(path, page_id) in overrides {
        let game = catalog.quests.iter().position(|q| q.journal_path == path);
        let page = wiki.quests.iter().position(|w| w.page_id == page_id);
        if let (Some(i), Some(w)) = (game, page) {
            matches.entry(i).or_insert(w);
        }
    }

    for (i, q) in catalog.quests.iter().enumerate() {
        if !matches.contains_key(&i) {
            report.unmatched_game.push(q.title("en").unwrap_or(&q.base_name).to_string());
        }
    }
    report.unmatched_wiki.sort();
    report.unmatched_game.sort();
    matches
}

/// Lowercase alphanumerics separated by single spaces, so punctuation and apostrophe styles
/// don't matter.
fn normalize(title: &str) -> String {
    let cleaned: String = title
        .chars()
        .map(|c| if c.is_alphanumeric() { c.to_ascii_lowercase() } else if c == '\'' || c == '\u{2019}' { '\0' } else { ' ' })
        .filter(|&c| c != '\0')
        .collect();
    cleaned.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn source_of(content: Content) -> QuestSource {
    match content {
        Content::Base => QuestSource::BaseGame,
        Content::HeartsOfStone => QuestSource::HeartsOfStone,
        Content::BloodAndWine => QuestSource::BloodAndWine,
    }
}

/// Region from the journal's world id; the engine shares one map for Velen and Novigrad.
fn region_of(world: Option<u32>) -> Region {
    match world {
        Some(1) => Region::Novigrad,
        Some(2) => Region::Skellige,
        Some(3) => Region::KaerMorhen,
        Some(4 | 8) => Region::WhiteOrchard,
        Some(5) => Region::Vizima,
        Some(9) => Region::Velen,
        Some(11) => Region::Toussaint,
        _ => Region::Unknown,
    }
}

/// `quest_levels.csv` gives level 1 to quests it has no real level for.
const PLACEHOLDER_LEVEL: i32 = 1;

/// The game's level, unless it is the placeholder and the wiki knows better.
fn level_of(game: Option<i32>, wiki: Option<i32>) -> Option<i32> {
    match game {
        None | Some(PLACEHOLDER_LEVEL) => wiki.or(game),
        level => level,
    }
}

/// A wiki quest the game journal does not have (mostly unmarked events).
fn wiki_only_quest(w: &ScrapedQuest) -> NewQuest {
    NewQuest {
        journal_path: format!("{}{}", quest_db::WIKI_ONLY_PREFIX, w.page_id),
        journal_guid: String::new(),
        base_name: w.wiki_title.clone(),
        source: w.source,
        quest_type: w.quest_type,
        region: w.region,
        recommended_level: w.recommended_level,
        wiki_page_id: Some(w.page_id),
        wiki_title: Some(w.wiki_title.clone()),
        important_notes: w.important_notes.clone(),
        is_unmarked: w.is_unmarked,
        texts: vec![QuestText { language: "en".into(), title: w.name.clone(), description: w.description.clone() }],
        extra_journal_paths: vec![],
    }
}

/// Coarse story position: the prologue (White Orchard) first, the epilogue last.
fn stage_of(region: Region, group: &str) -> u8 {
    match group {
        "Prologue" => 0,
        "Epilogue" => 2,
        _ if region == Region::WhiteOrchard => 0,
        _ => 1,
    }
}

fn to_new_quest(game: &GameQuest, wiki: Option<&ScrapedQuest>) -> NewQuest {
    let english = game.title("en").unwrap_or_default();
    let quest_type = match game.quest_type {
        JournalQuestType::Story | JournalQuestType::Chapter => QuestType::MainQuest,
        JournalQuestType::Side => QuestType::SecondaryQuest,
        JournalQuestType::MonsterHunt => QuestType::WitcherContract,
        JournalQuestType::TreasureHunt
            if english.starts_with("Scavenger Hunt")
                || wiki.is_some_and(|w| w.quest_type == QuestType::ScavengerHunt) =>
        {
            QuestType::ScavengerHunt
        }
        JournalQuestType::TreasureHunt => QuestType::TreasureHunt,
    };
    let region = match wiki.map(|w| w.region) {
        Some(region) if region != Region::Unknown => region,
        _ => region_of(game.world),
    };
    let texts = game
        .titles
        .iter()
        .map(|(language, title)| QuestText {
            language: language.clone(),
            title: title.clone(),
            description: game.descriptions.get(language).cloned(),
        })
        .collect();
    NewQuest {
        journal_path: game.journal_path.clone(),
        journal_guid: game.guid.clone(),
        base_name: game.base_name.clone(),
        source: source_of(game.content),
        quest_type,
        region,
        recommended_level: level_of(game.recommended_level, wiki.and_then(|w| w.recommended_level)),
        wiki_page_id: wiki.map(|w| w.page_id),
        wiki_title: wiki.map(|w| w.wiki_title.clone()),
        important_notes: wiki.and_then(|w| w.important_notes.clone()),
        is_unmarked: wiki.is_some_and(|w| w.is_unmarked),
        texts,
        extra_journal_paths: vec![],
    }
}

struct SortNode {
    id: i64,
    source: QuestSource,
    level: Option<i32>,
    /// See [`stage_of`].
    stage: u8,
    /// Main quests sort before side content at the same level.
    main: bool,
    name: String,
}

impl SortNode {
    fn new(id: i64, quest: &NewQuest, name: &str, group: &str) -> Self {
        SortNode {
            id,
            source: quest.source,
            level: quest.recommended_level,
            stage: stage_of(quest.region, group),
            main: quest.quest_type == QuestType::MainQuest,
            name: name.to_string(),
        }
    }
}

/// Orders quests roughly as a player meets them: by expansion, then by story stage, then by
/// effective level, main quests first, then by depth in the previous-quest chain, then by name. The effective level never drops below
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
    let key = |i: usize| {
        (
            source_rank(nodes[i].source),
            nodes[i].stage,
            level[i].unwrap_or(i32::MAX),
            !nodes[i].main,
            depth[i],
            &nodes[i].name,
        )
    };
    order.sort_by(|&a, &b| key(a).cmp(&key(b)));
    order.into_iter().map(|i| nodes[i].id).collect()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use quest_db::{Difficulty, NewPlaythrough, QuestStatus, open_in_memory, playthroughs, progress};
    use quest_scraper::mock_scrape_result;

    fn path(n: u32) -> String {
        format!("gameplay\\journal\\quests\\q{n}.journal")
    }

    fn game_quest(n: u32, title: &str, quest_type: JournalQuestType, level: Option<i32>) -> GameQuest {
        GameQuest {
            journal_path: path(n),
            guid: format!("guid-{n}"),
            base_name: format!("Q{n}"),
            quest_type,
            content: Content::Base,
            world: Some(2),
            group: "Chapter 2".into(),
            recommended_level: level,
            titles: BTreeMap::from([
                ("en".to_string(), title.to_string()),
                ("pl".to_string(), format!("{title} (PL)")),
            ]),
            descriptions: BTreeMap::from([("en".to_string(), format!("About {title}"))]),
        }
    }

    /// Game-side counterparts of the mock wiki quests, plus one quest the wiki lacks.
    fn catalog() -> GameCatalog {
        GameCatalog {
            quests: vec![
                game_quest(1, "The Calm Before the Storm", JournalQuestType::Chapter, Some(14)),
                game_quest(2, "The Last Wish", JournalQuestType::Side, None),
                game_quest(3, "Ugly Baby", JournalQuestType::Chapter, Some(23)),
                game_quest(4, "The Isle of Mists", JournalQuestType::Chapter, Some(24)),
                game_quest(5, "Witch Hunter Raids", JournalQuestType::Side, Some(12)),
                game_quest(6, "Scavenger Hunt: Griffin School Gear", JournalQuestType::TreasureHunt, None),
            ],
            languages: vec!["en".into(), "pl".into()],
            warnings: vec![],
        }
    }

    fn fetched() -> Fetched {
        Fetched { catalog: catalog(), wiki: Ok(mock_scrape_result()) }
    }

    fn id_of(conn: &Connection, n: u32) -> i64 {
        quests::get_id_by_journal_path(conn, &path(n)).unwrap().unwrap()
    }

    fn new_playthrough(conn: &Connection) -> i64 {
        playthroughs::insert(
            conn,
            &NewPlaythrough {
                name: "Run".into(),
                difficulty: Difficulty::DeathMarch,
                is_new_game_plus: false,
                notes: None,
                link: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn merge_combines_game_data_with_wiki_data() {
        let mut conn = open_in_memory().unwrap();
        let report = merge(&mut conn, &fetched()).unwrap();
        assert_eq!(report.inserted, 6);
        assert_eq!(report.wiki_matched, 5);
        assert_eq!(report.unmatched_game, vec!["Scavenger Hunt: Griffin School Gear"]);
        assert!(report.unmatched_wiki.is_empty(), "{:?}", report.unmatched_wiki);
        assert!(report.unresolved.is_empty(), "{:?}", report.unresolved);

        let last_wish = quests::get(&conn, id_of(&conn, 2), "pl").unwrap();
        assert_eq!(last_wish.title, "The Last Wish (PL)");
        assert_eq!(last_wish.description.as_deref(), Some("About The Last Wish"), "English fallback");
        assert_eq!(last_wish.quest_type, QuestType::SecondaryQuest);
        assert_eq!(last_wish.region, Region::Skellige);
        assert_eq!(last_wish.recommended_level, Some(15), "wiki level when the game has none");
        assert_eq!(last_wish.wiki_page_id, Some(1002));
        assert_eq!(last_wish.cutoff_quest_id, Some(id_of(&conn, 3)));
        assert_eq!(last_wish.prerequisite_ids, vec![id_of(&conn, 1)]);
        assert!(last_wish.important_notes.is_some());

        // "Isle of Mists" is a redirect alias of the Isle of Mists page.
        let raids = quests::get(&conn, id_of(&conn, 5), "en").unwrap();
        assert_eq!(raids.cutoff_quest_id, Some(id_of(&conn, 4)));
        assert!(raids.is_unmarked);
        assert_eq!(raids.region, Region::Novigrad, "wiki region wins over the game's world");

        let gear = quests::get(&conn, id_of(&conn, 6), "en").unwrap();
        assert_eq!(gear.quest_type, QuestType::ScavengerHunt);
        assert_eq!(gear.region, Region::Skellige, "game world without wiki data");
        assert_eq!(gear.wiki_title, None);
    }

    #[test]
    fn remerge_updates_data_and_preserves_progress() {
        let mut conn = open_in_memory().unwrap();
        merge(&mut conn, &fetched()).unwrap();
        let last_wish = id_of(&conn, 2);
        let pt = new_playthrough(&conn);
        progress::set_manual_status(&conn, pt, last_wish, Some(QuestStatus::Completed)).unwrap();
        progress::set_notes(&conn, pt, last_wish, Some("Kept the bond")).unwrap();

        let mut changed = fetched();
        changed.catalog.quests[1].recommended_level = Some(18);
        changed.wiki.as_mut().unwrap().quests[1].previous_titles.clear();
        let report = merge(&mut conn, &changed).unwrap();
        assert_eq!((report.inserted, report.updated, report.removed), (0, 6, 0));
        assert_eq!(id_of(&conn, 2), last_wish);

        let quest = quests::get(&conn, last_wish, "en").unwrap();
        assert_eq!(quest.recommended_level, Some(18), "game level wins");
        assert!(quest.prerequisite_ids.is_empty());
        let prog = progress::get(&conn, pt, last_wish).unwrap().unwrap();
        assert_eq!(prog.status(), QuestStatus::Completed);
        assert_eq!(prog.notes.as_deref(), Some("Kept the bond"));
    }

    #[test]
    fn merge_removes_quests_gone_from_the_game_unless_they_have_user_data() {
        let mut conn = open_in_memory().unwrap();
        merge(&mut conn, &fetched()).unwrap();
        let pt = new_playthrough(&conn);
        progress::set_notes(&conn, pt, id_of(&conn, 2), Some("note")).unwrap();

        let mut changed = fetched();
        changed.catalog.quests.retain(|q| q.journal_path != path(2) && q.journal_path != path(6));
        let report = merge(&mut conn, &changed).unwrap();
        assert_eq!(report.removed, 1);
        assert_eq!(report.kept_stale, vec!["Q2"]);
        assert!(quests::get_id_by_journal_path(&conn, &path(6)).unwrap().is_none());
        // The wiki still lists The Last Wish, so it comes back as a wiki-only quest; the kept
        // game quest gives up the wiki page.
        assert_eq!(report.wiki_only, 1);
        let wiki_only = quests::get_id_by_journal_path(&conn, "wiki:1002").unwrap().unwrap();
        assert_eq!(quests::get(&conn, wiki_only, "en").unwrap().wiki_page_id, Some(1002));
        assert_eq!(quests::get(&conn, id_of(&conn, 2), "en").unwrap().wiki_page_id, None);
    }

    #[test]
    fn wiki_quests_without_a_journal_entry_are_added_untracked() {
        let mut conn = open_in_memory().unwrap();
        let mut f = fetched();
        f.catalog.quests.retain(|q| q.journal_path != path(5)); // Witch Hunter Raids: unmarked
        let report = merge(&mut conn, &f).unwrap();
        assert_eq!(report.wiki_only, 1);
        assert_eq!(report.unmatched_wiki, vec!["Witch Hunter Raids"]);
        let id = quests::get_id_by_journal_path(&conn, "wiki:1005").unwrap().unwrap();
        let raids = quests::get(&conn, id, "en").unwrap();
        assert!(!raids.is_trackable());
        assert!(raids.is_unmarked);
        assert_eq!(raids.title, "Witch Hunter Raids");
        assert_eq!(raids.cutoff_quest_id, Some(id_of(&conn, 4)), "links resolve for wiki-only quests");

        // Offline re-sync keeps it.
        let offline = Fetched { catalog: f.catalog.clone(), wiki: Err("offline".into()) };
        let report = merge(&mut conn, &offline).unwrap();
        assert_eq!(report.removed, 0);
        assert!(quests::get_id_by_journal_path(&conn, "wiki:1005").unwrap().is_some());
    }

    #[test]
    fn wiki_failure_keeps_previous_wiki_data() {
        let mut conn = open_in_memory().unwrap();
        merge(&mut conn, &fetched()).unwrap();
        let offline = Fetched { catalog: catalog(), wiki: Err("offline".into()) };
        let report = merge(&mut conn, &offline).unwrap();
        assert_eq!(report.wiki_error.as_deref(), Some("offline"));
        assert!(report.summary().contains("previous data was kept"));

        let last_wish = quests::get(&conn, id_of(&conn, 2), "en").unwrap();
        assert_eq!(last_wish.wiki_page_id, Some(1002));
        assert_eq!(last_wish.cutoff_quest_id, Some(id_of(&conn, 3)));
        assert_eq!(last_wish.prerequisite_ids, vec![id_of(&conn, 1)]);
        assert!(last_wish.important_notes.is_some());
        assert_eq!(last_wish.recommended_level, Some(15));
    }

    /// Full sync against a real install and the live wiki. Run with
    /// `W3_GAME_DIR=... cargo test live_full_sync -- --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "needs W3_GAME_DIR and hits the live Witcher wiki"]
    async fn live_full_sync() {
        let Some(dir) = std::env::var_os("W3_GAME_DIR") else { return };
        let started = std::time::Instant::now();
        let fetched = fetch(dir.into(), |_| {}).await.unwrap();
        if let Err(e) = &fetched.wiki {
            panic!("wiki: {e}");
        }
        let mut conn = open_in_memory().unwrap();
        let report = merge(&mut conn, &fetched).unwrap();
        println!("sync took {:.1?}", started.elapsed());
        println!("{}", report.summary());
        println!("unmatched wiki ({}): {:#?}", report.unmatched_wiki.len(), report.unmatched_wiki);
        println!("unmatched game ({}): {:#?}", report.unmatched_game.len(), report.unmatched_game);
        println!("unresolved ({}): {:?}", report.unresolved.len(), report.unresolved);
        let all = quests::list(&conn, "en").unwrap();
        let first: Vec<_> = all.iter().take(15).map(|q| q.title.as_str()).collect();
        println!("first in story order: {first:?}");
        assert!(report.wiki_matched > 300, "{}", report.wiki_matched);
    }

    #[test]
    fn journal_files_sharing_a_page_become_one_quest() {
        let mut conn = open_in_memory().unwrap();
        let mut f = fetched();
        // The Last Wish split over two files (like the epilogue variants), listed first.
        f.catalog.quests.insert(0, game_quest(7, "The Last Wish", JournalQuestType::Side, Some(30)));
        let report = merge(&mut conn, &f).unwrap();
        assert_eq!((report.wiki_matched, report.folded, report.inserted), (5, 1, 6));
        // Keyed by the first file; both files map to it.
        let id = id_of(&conn, 7);
        assert_eq!(quests::get_id_by_journal_path(&conn, &path(2)).unwrap(), None);
        let ids = quests::journal_ids(&conn).unwrap();
        assert_eq!((ids[&path(7)], ids[&path(2)]), (id, id));
        let q = quests::get(&conn, id, "en").unwrap();
        assert_eq!(q.wiki_page_id, Some(1002));
        assert_eq!(q.cutoff_quest_id, Some(id_of(&conn, 3)));
        // Links to the page resolve to the merged row.
        let rows = quests::list(&conn, "en").unwrap();
        assert_eq!(rows.iter().filter(|r| r.title == "The Last Wish").count(), 1);

        // Re-sync keeps the same row.
        let report = merge(&mut conn, &f).unwrap();
        assert_eq!((report.inserted, report.removed), (0, 0));
        assert_eq!(id_of(&conn, 7), id);
    }

    #[test]
    fn a_file_added_later_is_folded_into_the_existing_row() {
        let mut conn = open_in_memory().unwrap();
        let mut f = fetched();
        merge(&mut conn, &f).unwrap();
        f.catalog.quests.push(game_quest(7, "The Last Wish", JournalQuestType::Side, None));
        let report = merge(&mut conn, &f).unwrap();
        assert_eq!((report.inserted, report.removed, report.folded), (0, 0, 1));
        assert_eq!(quests::journal_ids(&conn).unwrap()[&path(7)], id_of(&conn, 2));
    }

    #[test]
    fn excluded_journal_files_are_left_out() {
        let mut conn = open_in_memory().unwrap();
        let mut f = fetched();
        f.catalog.quests.push(GameQuest {
            journal_path: EXCLUDED_JOURNALS[0].to_string(),
            ..game_quest(8, "Regatta: Helmsman's Dash", JournalQuestType::Side, None)
        });
        let report = merge(&mut conn, &f).unwrap();
        assert_eq!(report.excluded, 1);
        assert!(!report.unmatched_game.iter().any(|t| t.starts_with("Regatta")));
        assert!(quests::get_id_by_journal_path(&conn, EXCLUDED_JOURNALS[0]).unwrap().is_none());
    }

    #[test]
    fn same_title_sharing_needs_a_unique_wiki_title() {
        let mut f = fetched();
        f.catalog.quests.push(game_quest(7, "The Last Wish", JournalQuestType::Side, None));
        // A second, different wiki quest with the same name: the extra file stays unmatched.
        let mut twin = f.wiki.as_ref().unwrap().quests[1].clone();
        twin.page_id = 2002;
        twin.wiki_title = "The Last Wish (other quest)".into();
        twin.source = QuestSource::BloodAndWine;
        f.wiki.as_mut().unwrap().quests.push(twin);
        let mut report = SyncReport::default();
        let matches = match_wiki(&f.catalog, f.wiki.as_ref().unwrap(), &[], &mut report);
        let pages: Vec<i64> =
            [1usize, 6].iter().filter_map(|i| matches.get(i)).map(|&w| f.wiki.as_ref().unwrap().quests[w].page_id).collect();
        assert_eq!(pages.len(), 2, "both files get some page: {pages:?}");
        assert_ne!(pages[0], pages[1], "never the same page for two distinct wiki quests");
    }

    #[test]
    fn overrides_map_journal_files_with_different_titles() {
        let mut f = fetched();
        f.catalog.quests.push(game_quest(7, "The Last Wish II", JournalQuestType::Side, None));
        let wiki = f.wiki.as_ref().unwrap();
        let mut report = SyncReport::default();
        let without = match_wiki(&f.catalog, wiki, &[], &mut SyncReport::default());
        assert!(!without.contains_key(&6));
        let p7 = path(7);
        let overrides = [(p7.as_str(), 1002), ("not-in-the-game.journal", 1003)];
        let with = match_wiki(&f.catalog, wiki, &overrides, &mut report);
        assert_eq!(wiki.quests[with[&6]].page_id, 1002);
        assert_eq!(report.unmatched_game, vec!["Scavenger Hunt: Griffin School Gear"]);
    }

    #[test]
    fn normalizes_titles() {
        assert_eq!(normalize("Contract: The Griffin from the Highlands"), "contract the griffin from the highlands");
        assert_eq!(normalize("Ciri\u{2019}s Room"), normalize("Ciri's Room"));
        assert_eq!(normalize("  A  Matter of Life  and Death "), "a matter of life and death");
    }

    #[test]
    fn story_order_puts_the_prologue_and_main_quests_first() {
        let node = |id, stage, main, level: Option<i32>, name: &str| SortNode {
            id,
            source: QuestSource::BaseGame,
            level,
            stage,
            main,
            name: name.into(),
        };
        let nodes = vec![
            node(1, 1, false, Some(1), "Collect 'Em All"),
            node(2, 0, false, Some(1), "A Frying Pan, Spick and Span"),
            node(3, 0, true, Some(1), "Kaer Morhen"),
            node(4, 2, true, Some(1), "Something Ends, Something Begins"),
        ];
        assert_eq!(story_order(&nodes, &HashMap::new()), vec![3, 2, 1, 4]);
    }

    #[test]
    fn placeholder_game_level_defers_to_the_wiki() {
        assert_eq!(level_of(Some(1), Some(11)), Some(11));
        assert_eq!(level_of(Some(1), None), Some(1));
        assert_eq!(level_of(None, Some(5)), Some(5));
        assert_eq!(level_of(Some(14), Some(12)), Some(14));
    }

    #[test]
    fn story_order_uses_source_level_and_chain_depth() {
        let node = |id, source, level: Option<i32>, name: &str| SortNode { id, source, level, stage: 1, main: false, name: name.into() };
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
        let node = |id, level: Option<i32>, name: &str| SortNode { id, source: QuestSource::BaseGame, level, stage: 1, main: false, name: name.into() };
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
        let node = |id, name: &str| SortNode { id, source: QuestSource::BaseGame, level: Some(1), stage: 1, main: false, name: name.into() };
        let nodes = vec![node(1, "A"), node(2, "B")];
        let prereqs = HashMap::from([(1, vec![2]), (2, vec![1])]);
        let order = story_order(&nodes, &prereqs);
        assert_eq!(order.len(), 2);
    }

}
