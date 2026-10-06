//! The quest catalog as the game defines it: one entry per `CJournalQuest` resource.
//!
//! Every quest lives in its own `.journal` file under a `journal\quests` folder. The root export
//! is a `CJournalQuest` (`guid`, `baseName`, `type`, `world`, `contentType`, `title`,
//! `parentGuid` = its group); `CJournalQuestDescriptionEntry` exports hold the journal text. The
//! group files (`questact1.journal`, `sidequests.journal`, ...) hold `CJournalQuestGroup`s.
//! Recommended levels come from `quest_levels.csv` files, keyed by `baseName`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use crate::bundle::BundleSet;
use crate::cr2w::{self, Export};
use crate::error::Result;
use crate::strings;

/// `CJournalQuest.type`. Unset means `Story`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JournalQuestType {
    Story,
    Chapter,
    Side,
    MonsterHunt,
    TreasureHunt,
}

/// Which content a quest ships with.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Content {
    /// Base game, including the free DLCs.
    Base,
    HeartsOfStone,
    BloodAndWine,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GameQuest {
    /// Depot path of the quest's `.journal` file: the key saves use for its status.
    pub journal_path: String,
    pub guid: String,
    /// Internal name, e.g. `Q001 Dream`.
    pub base_name: String,
    pub quest_type: JournalQuestType,
    pub content: Content,
    /// `CJournalQuest.world` area id (1 Novigrad/Velen map, 2 Skellige, 3 Kaer Morhen,
    /// 4 White Orchard, 5 Vizima, 8 White Orchard in winter, 9 Velen, 11 Toussaint).
    pub world: Option<u32>,
    /// `baseName` of the quest group, e.g. `Chapter 1`, `Sidequests`, `BoB LW`.
    pub group: String,
    pub recommended_level: Option<i32>,
    /// Title per language code.
    pub titles: BTreeMap<String, String>,
    /// Journal description (the first description entry) per language code.
    pub descriptions: BTreeMap<String, String>,
}

impl GameQuest {
    pub fn title(&self, language: &str) -> Option<&str> {
        self.titles.get(language).map(String::as_str)
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameCatalog {
    pub quests: Vec<GameQuest>,
    /// Language codes that had text, sorted.
    pub languages: Vec<String>,
    /// Non-fatal problems (unreadable files, unknown values) worth logging.
    pub warnings: Vec<String>,
}

/// Quest groups that only hold test or demo content.
const IGNORED_GROUPS: &[&str] = &["Demo Quests", "TEMP MinorTestingQuest", "testlevel"];

fn wanted_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    (p.ends_with(".journal") && p.contains("journal\\quests\\")) || p.ends_with("quest_levels.csv")
}

/// Reads the quest catalog from a game install (read-only).
pub fn read_game_catalog(game_dir: &Path) -> Result<GameCatalog> {
    let bundles = BundleSet::open(game_dir, &wanted_path)?;
    let mut catalog = GameCatalog::default();

    struct Raw {
        quest: GameQuest,
        parent: Option<String>,
        title_id: Option<u32>,
        description_id: Option<u32>,
    }
    let mut raws = Vec::new();
    let mut groups: HashMap<String, String> = HashMap::new(); // guid -> baseName
    let mut levels: HashMap<String, i32> = HashMap::new(); // lowercase baseName -> level

    let mut paths: Vec<&str> = bundles.paths().collect();
    paths.sort_unstable();
    for path in paths {
        let data = match bundles.read(path) {
            Ok(data) => data,
            Err(e) => {
                catalog.warnings.push(format!("{path}: {e}"));
                continue;
            }
        };
        if path.ends_with(".csv") {
            parse_levels(&data, &mut levels);
            continue;
        }
        let exports = match cr2w::parse(&data) {
            Ok(exports) => exports,
            Err(e) => {
                catalog.warnings.push(format!("{path}: {e}"));
                continue;
            }
        };
        // Export 0 is the CJournalResource wrapper; the entry it holds comes next.
        let Some(root) = exports.iter().find(|e| e.class != "CJournalResource") else { continue };
        match root.class.as_str() {
            "CJournalQuestGroup" => {
                if let (Some(guid), Some(name)) = (root.string("guid"), root.string("baseName")) {
                    groups.insert(guid.to_string(), name.to_string());
                }
            }
            "CJournalQuest" => {
                let description_id = exports
                    .iter()
                    .filter(|e| e.class == "CJournalQuestDescriptionEntry")
                    .find_map(|e| e.localized("description"));
                match quest_from(path, root) {
                    Ok(quest) => raws.push(Raw {
                        quest,
                        parent: root.string("parentGuid").map(str::to_string),
                        title_id: root.localized("title"),
                        description_id,
                    }),
                    Err(warning) => catalog.warnings.push(warning),
                }
            }
            _ => {}
        }
    }

    // Group names, level join, and dropping test content and untitled entries.
    raws.retain_mut(|raw| {
        raw.quest.group = raw.parent.as_ref().and_then(|g| groups.get(g)).cloned().unwrap_or_default();
        raw.quest.recommended_level = levels.get(&raw.quest.base_name.to_ascii_lowercase()).copied();
        raw.title_id.is_some() && !IGNORED_GROUPS.contains(&raw.quest.group.as_str())
    });

    // Text for every language the install has.
    let wanted: HashSet<u32> =
        raws.iter().flat_map(|r| [r.title_id, r.description_id]).flatten().collect();
    for (language, files) in strings::language_files(game_dir)? {
        let text = match strings::load(&files, &wanted) {
            Ok(text) => text,
            Err(e) => {
                catalog.warnings.push(format!("{language} strings: {e}"));
                continue;
            }
        };
        if text.is_empty() {
            continue;
        }
        for raw in &mut raws {
            let lookup = |id: Option<u32>| id.and_then(|id| text.get(&id)).map(|s| s.trim().to_string());
            if let Some(title) = lookup(raw.title_id).filter(|t| !t.is_empty()) {
                raw.quest.titles.insert(language.clone(), title);
            }
            if let Some(desc) = lookup(raw.description_id).filter(|d| !d.is_empty()) {
                raw.quest.descriptions.insert(language.clone(), desc);
            }
        }
        catalog.languages.push(language);
    }

    // Debug quests and templates have no English title; real quests always do.
    catalog.quests =
        raws.into_iter().map(|r| r.quest).filter(|q| q.titles.contains_key("en")).collect();
    Ok(catalog)
}

fn quest_from(path: &str, e: &Export) -> std::result::Result<GameQuest, String> {
    let quest_type = match e.string("type") {
        None => JournalQuestType::Story,
        Some("Story") => JournalQuestType::Story,
        Some("Chapter") => JournalQuestType::Chapter,
        Some("Side") => JournalQuestType::Side,
        Some("MonsterHunt") => JournalQuestType::MonsterHunt,
        Some("TreasureHunt") => JournalQuestType::TreasureHunt,
        Some(other) => return Err(format!("{path}: unknown quest type {other}")),
    };
    let content = match e.string("contentType") {
        Some("EJCT_EP1") => Content::HeartsOfStone,
        Some("EJCT_EP2") => Content::BloodAndWine,
        _ if path.starts_with("dlc\\ep1\\") => Content::HeartsOfStone,
        _ if path.starts_with("dlc\\bob\\") => Content::BloodAndWine,
        _ => Content::Base,
    };
    Ok(GameQuest {
        journal_path: path.to_string(),
        guid: e.string("guid").unwrap_or_default().to_string(),
        base_name: e.string("baseName").unwrap_or_default().to_string(),
        quest_type,
        content,
        world: e.u32("world"),
        group: String::new(),
        recommended_level: None,
        titles: BTreeMap::new(),
        descriptions: BTreeMap::new(),
    })
}

/// Parses a `Quest Name;Level` CSV. Later files win, so DLC copies can override.
fn parse_levels(data: &[u8], out: &mut HashMap<String, i32>) {
    let text = String::from_utf8_lossy(data);
    for line in text.lines().skip(1) {
        let Some((name, level)) = line.rsplit_once(';') else { continue };
        if let Ok(level) = level.trim().parse::<i32>() {
            out.insert(name.trim().trim_matches('"').to_ascii_lowercase(), level);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_quest_levels() {
        let mut levels = HashMap::new();
        parse_levels(b"Quest Name;Level\r\nSQ305 Scoiatael;6\r\nQ302 Mafia;12\r\nbroken\r\n", &mut levels);
        assert_eq!(levels.get("sq305 scoiatael"), Some(&6));
        assert_eq!(levels.get("q302 mafia"), Some(&12));
        assert_eq!(levels.len(), 2);
    }

    #[test]
    fn selects_quest_journals_and_level_tables() {
        assert!(wanted_path("gameplay\\journal\\quests\\q001beggining.journal"));
        assert!(wanted_path("dlc\\bob\\journal\\quests\\mq7002knight.journal"));
        assert!(wanted_path("gameplay\\globals\\quest_levels.csv"));
        assert!(!wanted_path("gameplay\\journal\\bestiary\\bestiarycrabspider.journal"));
    }
}
