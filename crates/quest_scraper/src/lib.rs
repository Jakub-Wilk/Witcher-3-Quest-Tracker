pub mod client;
pub mod error;
pub mod language;
#[cfg(any(test, feature = "mock"))]
pub mod mock;
pub mod models;
pub mod wikitext;

pub use client::{QUEST_CATEGORIES, WikiScraperClient};
pub use error::{Result, ScraperError};
pub use language::Language;
#[cfg(any(test, feature = "mock"))]
pub use mock::mock_scrape_result;
pub use models::{ScrapeProgress, ScrapeResult, ScrapedQuest, WikiPage};
pub use wikitext::parse_quest;
