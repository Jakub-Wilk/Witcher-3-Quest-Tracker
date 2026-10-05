//! Offline sample data mirroring real wiki content, for tests.

use quest_db::{QuestSource, QuestType, Region};

use crate::language::Language;
use crate::models::{ScrapeResult, ScrapedQuest};
use crate::wikitext::normalize_title;

fn quest(page_id: i64, wiki_title: &str, name: &str, quest_type: QuestType, region: Region) -> ScrapedQuest {
    ScrapedQuest {
        page_id,
        wiki_title: wiki_title.into(),
        name: name.into(),
        localized_name: None,
        source: QuestSource::BaseGame,
        source_from_infobox: false,
        quest_type,
        region,
        recommended_level: None,
        is_unmarked: false,
        description: None,
        important_notes: None,
        cutoff_titles: vec![],
        previous_titles: vec![],
    }
}

/// A small, realistic scrape: The Last Wish is cut off by Ugly Baby and follows
/// The Calm Before the Storm. "Isle of Mists" is a redirect alias of the Isle of Mists quest.
pub fn mock_scrape_result() -> ScrapeResult {
    let quests = vec![
        ScrapedQuest {
            recommended_level: Some(14),
            ..quest(1001, "The Calm Before the Storm", "The Calm Before the Storm", QuestType::MainQuest, Region::Skellige)
        },
        ScrapedQuest {
            recommended_level: Some(15),
            description: Some("Before Geralt and Yennefer parted ...".into()),
            important_notes: Some("This quest will fail if not completed before retrieving Uma from Crow's Perch during Ugly Baby.".into()),
            cutoff_titles: vec!["Ugly Baby".into()],
            previous_titles: vec!["The Calm Before the Storm".into()],
            ..quest(1002, "The Last Wish (quest)", "The Last Wish", QuestType::SecondaryQuest, Region::Skellige)
        },
        ScrapedQuest {
            recommended_level: Some(23),
            previous_titles: vec!["The Calm Before the Storm".into()],
            ..quest(1003, "Ugly Baby", "Ugly Baby", QuestType::MainQuest, Region::KaerMorhen)
        },
        ScrapedQuest {
            recommended_level: Some(24),
            previous_titles: vec!["Ugly Baby".into()],
            ..quest(1004, "The Isle of Mists (quest)", "The Isle of Mists", QuestType::MainQuest, Region::Skellige)
        },
        ScrapedQuest {
            recommended_level: Some(12),
            is_unmarked: true,
            cutoff_titles: vec!["Isle of Mists".into()],
            ..quest(1005, "Witch Hunter Raids", "Witch Hunter Raids", QuestType::SecondaryQuest, Region::Novigrad)
        },
    ];

    let mut aliases: std::collections::HashMap<String, i64> =
        quests.iter().map(|q| (normalize_title(&q.wiki_title), q.page_id)).collect();
    aliases.insert("Isle of Mists".into(), 1004);

    ScrapeResult { quests, aliases, skipped: vec![], language: Language::English }
}
