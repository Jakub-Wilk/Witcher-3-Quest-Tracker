use std::collections::HashMap;

use quest_db::{QuestSource, QuestType, Region};

use crate::language::Language;

/// A raw page fetched from the wiki.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiPage {
    pub page_id: i64,
    pub title: String,
    pub wikitext: String,
    /// Titles of redirect pages pointing at this page.
    pub redirects: Vec<String>,
}

/// A quest parsed from its wiki page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScrapedQuest {
    /// MediaWiki page id — stable across page renames.
    pub page_id: i64,
    /// Page title, e.g. "The Last Wish (quest)".
    pub wiki_title: String,
    /// In-game quest name, e.g. "The Last Wish".
    pub name: String,
    /// Quest title in the requested sync language, if not English and a translation exists.
    pub localized_name: Option<String>,
    pub source: QuestSource,
    /// Whether `source` came from the infobox `type` suffix (otherwise it is a default that
    /// category membership may override).
    pub source_from_infobox: bool,
    pub quest_type: QuestType,
    pub region: Region,
    pub recommended_level: Option<i32>,
    pub is_unmarked: bool,
    /// First paragraph of the in-game journal entry.
    pub description: Option<String>,
    /// Missable-quest warnings from the page lead, newline separated.
    pub important_notes: Option<String>,
    /// Normalized page titles of cutoff quests (completing one locks this quest out).
    pub cutoff_titles: Vec<String>,
    /// Normalized page titles of the quests that lead into this one.
    pub previous_titles: Vec<String>,
}

/// Everything a full scrape produced.
#[derive(Debug, Clone, Default)]
pub struct ScrapeResult {
    pub quests: Vec<ScrapedQuest>,
    /// Normalized page title or redirect title -> page id, for resolving quest links.
    pub aliases: HashMap<String, i64>,
    /// `(page title, reason)` for pages that were listed but are not parseable quests.
    pub skipped: Vec<(String, String)>,
    /// Language of `ScrapedQuest::localized_name`.
    pub language: Language,
}

impl ScrapeResult {
    /// Resolves a normalized page title (or redirect) to a page id.
    pub fn resolve(&self, title: &str) -> Option<i64> {
        self.aliases.get(title).copied()
    }

    /// Number of quests with a localized name.
    pub fn translated_count(&self) -> usize {
        self.quests.iter().filter(|q| q.localized_name.is_some()).count()
    }
}

/// Progress of [`crate::WikiScraperClient::fetch_all_quests`], for UI feedback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrapeProgress {
    ListingCategories,
    FetchingPages { done: usize, total: usize },
    Translating { done: usize, total: usize },
}
