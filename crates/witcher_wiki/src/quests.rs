//! The quest scraper: every Witcher 3 quest page with its infobox.

use std::collections::BTreeMap;

use companion_db::QuestSource;

use crate::client::WikiClient;
use crate::error::Result;
use crate::language::Language;
use crate::models::{ScrapeProgress, ScrapeResult};
use crate::wikitext::{normalize_title, parse_quest, strip_disambiguator};

/// Wiki categories listing every Witcher 3 quest. The expansion categories also tell the
/// quest's source, used when the infobox type has no expansion suffix.
pub const QUEST_CATEGORIES: &[(&str, Option<QuestSource>)] = &[
    ("Category:The Witcher 3 main quests", None),
    ("Category:The Witcher 3 secondary quests", None),
    ("Category:The Witcher 3 contracts", None),
    ("Category:The Witcher 3 treasure hunts", None),
    ("Category:Hearts of Stone quests", Some(QuestSource::HeartsOfStone)),
    ("Category:Blood and Wine quests", Some(QuestSource::BloodAndWine)),
];

impl WikiClient {
    /// Scrapes every quest in [`QUEST_CATEGORIES`]: lists the categories, fetches all pages
    /// as wikitext in batches, parses them and — for non-English `language` — fetches the
    /// translated titles. Fails as a whole if any request fails, so callers never see a
    /// partial quest list.
    pub async fn fetch_all_quests(
        &self,
        language: Language,
        mut on_progress: impl FnMut(ScrapeProgress),
    ) -> Result<ScrapeResult> {
        on_progress(ScrapeProgress::ListingCategories);
        let mut source_hints: BTreeMap<String, Option<QuestSource>> = BTreeMap::new();
        for &(category, hint) in QUEST_CATEGORIES {
            for title in self.list_category(category).await? {
                let entry = source_hints.entry(title).or_insert(None);
                if hint.is_some() {
                    *entry = hint;
                }
            }
        }

        let titles: Vec<String> = source_hints.keys().cloned().collect();
        let pages = self
            .fetch_pages(&titles, |done, total| on_progress(ScrapeProgress::FetchingPages { done, total }))
            .await?;

        let mut result = ScrapeResult { language, ..Default::default() };
        for page in pages {
            match parse_quest(&page) {
                Ok(mut quest) => {
                    if !quest.source_from_infobox {
                        if let Some(Some(source)) = source_hints.get(&page.title) {
                            quest.source = *source;
                        }
                    }
                    result.aliases.insert(normalize_title(&page.title), page.page_id);
                    for redirect in &page.redirects {
                        result.aliases.insert(normalize_title(redirect), page.page_id);
                    }
                    result.quests.push(quest);
                }
                Err(e) => result.skipped.push((page.title, e.to_string())),
            }
        }

        if language != Language::English {
            let quest_titles: Vec<String> = result.quests.iter().map(|q| q.wiki_title.clone()).collect();
            let translated = self
                .fetch_langlinks(&quest_titles, language, |done, total| {
                    on_progress(ScrapeProgress::Translating { done, total })
                })
                .await?;
            for quest in &mut result.quests {
                quest.localized_name = translated
                    .get(&quest.wiki_title)
                    .map(|t| strip_disambiguator(t).to_string())
                    .filter(|t| !t.is_empty());
            }
        }

        Ok(result)
    }
}
