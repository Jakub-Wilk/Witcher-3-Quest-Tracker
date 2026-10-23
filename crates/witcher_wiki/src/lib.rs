pub mod client;
pub mod error;
pub mod items;
pub mod language;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod models;
pub mod quests;
pub mod wikitext;

pub use client::WikiClient;
pub use error::{Result, ScraperError};
pub use items::{ItemInfobox, WikiFile};
pub use language::Language;
#[cfg(any(test, feature = "mock"))]
pub use mock::mock_scrape_result;
pub use models::{ScrapeProgress, ScrapeResult, ScrapedQuest, WikiPage};
pub use quests::QUEST_CATEGORIES;
pub use wikitext::parse_quest;
