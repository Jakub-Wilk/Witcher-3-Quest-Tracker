use futures::stream::{self, StreamExt};
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::error::{Result, ScraperError};
use crate::models::{ScrapedQuest, ScrapedQuestStore};
use crate::parser::parse_quest_html;

/// Async client for scraping Witcher 3 quest data from the Witcher Fandom Wiki.
pub struct WikiScraperClient {
    client: reqwest::Client,
    base_url: String,
}

impl WikiScraperClient {
    /// Creates a new `WikiScraperClient` configured with a standard Fandom User-Agent.
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Mozilla/5.0 (Windows NT 10.0; Win64; x64) Witcher3QuestTracker/0.1.0"),
        );

        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;

        Ok(Self {
            client,
            base_url: "https://witcher.fandom.com".into(),
        })
    }

    /// Fetches and parses a single quest page by its wiki page title (e.g. "Witch_Hunter_Raids" or "The_Last_Wish").
    pub async fn fetch_quest(&self, page_title: &str) -> Result<ScrapedQuest> {
        let canonical_title = page_title.replace(' ', "_");
        let wiki_url = format!("{}/wiki/{}", self.base_url, canonical_title);

        // First attempt via MediaWiki parse API (bypasses CDN rate limiters and returns structured HTML)
        let api_url = format!(
            "{}/api.php?action=parse&page={}&prop=text&format=json",
            self.base_url, canonical_title
        );

        let resp = self.client.get(&api_url).send().await?;
        if resp.status().is_success() {
            let json: Value = resp.json().await?;
            if let Some(html_text) = json["parse"]["text"]["*"].as_str() {
                return parse_quest_html(html_text, page_title, &wiki_url);
            }
        }

        // Direct web fetch fallback
        let web_resp = self.client.get(&wiki_url).send().await?;
        if !web_resp.status().is_success() {
            return Err(ScraperError::PageNotFound(page_title.to_string()));
        }

        let html = web_resp.text().await?;
        parse_quest_html(&html, page_title, &wiki_url)
    }

    /// Fetches a list of page titles belonging to a given wiki category (e.g. "Category:The_Witcher_3_main_quests").
    pub async fn fetch_category_member_titles(&self, category_title: &str) -> Result<Vec<String>> {
        let api_url = format!(
            "{}/api.php?action=query&list=categorymembers&cmtitle={}&cmlimit=500&format=json",
            self.base_url, category_title
        );

        let resp = self.client.get(&api_url).send().await?.json::<Value>().await?;
        let mut titles = Vec::new();

        if let Some(members) = resp["query"]["categorymembers"].as_array() {
            for member in members {
                if let Some(title) = member["title"].as_str() {
                    // Skip subcategory pages
                    if !title.starts_with("Category:") {
                        titles.push(title.to_string());
                    }
                }
            }
        }

        Ok(titles)
    }

    /// Batch fetches multiple quest pages into an in-memory `ScrapedQuestStore`.
    /// Pages that fail to fetch or parse are collected in `BatchResult::failed`.
    pub async fn fetch_quests_batch(&self, page_titles: &[&str]) -> BatchResult {
        self.fetch_quests_batch_with_progress(page_titles, |_, _| {}).await
    }

    /// Like `fetch_quests_batch`, fetching up to `BATCH_CONCURRENCY` pages at once and calling
    /// `on_progress(done, total)` after each page completes.
    pub async fn fetch_quests_batch_with_progress(
        &self,
        page_titles: &[&str],
        mut on_progress: impl FnMut(usize, usize),
    ) -> BatchResult {
        let total = page_titles.len();
        let mut results = stream::iter(page_titles.iter().map(|&title| async move {
            (title, self.fetch_quest(title).await)
        }))
        .buffer_unordered(BATCH_CONCURRENCY);

        let mut batch = BatchResult::default();
        let mut done = 0;
        while let Some((title, result)) = results.next().await {
            match result {
                Ok(quest) => batch.store.push(quest),
                Err(e) => batch.failed.push((title.to_string(), e.to_string())),
            }
            done += 1;
            on_progress(done, total);
        }
        batch
    }
}

/// Maximum number of wiki pages fetched concurrently by batch operations.
const BATCH_CONCURRENCY: usize = 4;

/// Outcome of a batch fetch: the successfully scraped quests plus `(title, error)` for each failure.
#[derive(Debug, Default)]
pub struct BatchResult {
    pub store: ScrapedQuestStore,
    pub failed: Vec<(String, String)>,
}
