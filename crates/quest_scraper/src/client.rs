use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use quest_db::QuestSource;
use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::error::{Result, ScraperError};
use crate::language::Language;
use crate::models::{ScrapeProgress, ScrapeResult, WikiPage};
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

/// MediaWiki's limit of titles per query for regular clients.
const TITLES_PER_REQUEST: usize = 50;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: u32 = 3;

/// Async client for the Witcher Fandom wiki's MediaWiki API.
pub struct WikiScraperClient {
    client: reqwest::Client,
    api_url: String,
}

impl WikiScraperClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Witcher3QuestTracker/0.1 (desktop quest tracker; reqwest)"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self { client, api_url: "https://witcher.fandom.com/api.php".into() })
    }

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

    /// Lists the article titles (main namespace only) in a category.
    pub async fn list_category(&self, category: &str) -> Result<Vec<String>> {
        let mut titles = Vec::new();
        let params = [
            ("action", "query"),
            ("list", "categorymembers"),
            ("cmtitle", category),
            ("cmnamespace", "0"),
            ("cmlimit", "max"),
        ];
        self.query_all(&params, |json| {
            for member in array(&json["query"]["categorymembers"]) {
                if let Some(title) = member["title"].as_str() {
                    titles.push(title.to_string());
                }
            }
        })
        .await?;
        Ok(titles)
    }

    /// Fetches the wikitext and redirect titles of the given pages, `TITLES_PER_REQUEST` at
    /// a time, reporting `(done, total)` pages after each batch. Missing pages are omitted.
    pub async fn fetch_pages(
        &self,
        titles: &[String],
        mut on_progress: impl FnMut(usize, usize),
    ) -> Result<Vec<WikiPage>> {
        let mut pages: BTreeMap<i64, WikiPage> = BTreeMap::new();
        for (i, chunk) in titles.chunks(TITLES_PER_REQUEST).enumerate() {
            let joined = chunk.join("|");
            let params = [
                ("action", "query"),
                ("prop", "revisions|redirects"),
                ("rvprop", "content"),
                ("rvslots", "main"),
                ("rdlimit", "max"),
                ("titles", joined.as_str()),
            ];
            // Continuation responses repeat pages with more redirects; merge by page id.
            self.query_all(&params, |json| {
                for page in array(&json["query"]["pages"]) {
                    let Some(page_id) = page["pageid"].as_i64() else { continue };
                    let entry = pages.entry(page_id).or_insert_with(|| WikiPage {
                        page_id,
                        title: page["title"].as_str().unwrap_or_default().to_string(),
                        wikitext: String::new(),
                        redirects: Vec::new(),
                    });
                    if let Some(content) = page["revisions"][0]["slots"]["main"]["content"].as_str() {
                        entry.wikitext = content.to_string();
                    }
                    for redirect in array(&page["redirects"]) {
                        if let Some(title) = redirect["title"].as_str() {
                            entry.redirects.push(title.to_string());
                        }
                    }
                }
            })
            .await?;
            on_progress(((i + 1) * TITLES_PER_REQUEST).min(titles.len()), titles.len());
        }
        Ok(pages.into_values().collect())
    }

    /// Fetches interlanguage links: English page title -> page title on the `language` wiki.
    pub async fn fetch_langlinks(
        &self,
        titles: &[String],
        language: Language,
        mut on_progress: impl FnMut(usize, usize),
    ) -> Result<HashMap<String, String>> {
        let mut links = HashMap::new();
        for (i, chunk) in titles.chunks(TITLES_PER_REQUEST).enumerate() {
            let joined = chunk.join("|");
            let params = [
                ("action", "query"),
                ("prop", "langlinks"),
                ("lllang", language.code()),
                ("lllimit", "max"),
                ("titles", joined.as_str()),
            ];
            self.query_all(&params, |json| {
                for page in array(&json["query"]["pages"]) {
                    let (Some(title), Some(link)) =
                        (page["title"].as_str(), page["langlinks"][0]["title"].as_str())
                    else {
                        continue;
                    };
                    links.insert(title.to_string(), link.to_string());
                }
            })
            .await?;
            on_progress(((i + 1) * TITLES_PER_REQUEST).min(titles.len()), titles.len());
        }
        Ok(links)
    }

    /// Runs a query, following MediaWiki `continue` tokens until the result is complete.
    async fn query_all(&self, params: &[(&str, &str)], mut on_response: impl FnMut(&Value)) -> Result<()> {
        let mut continuation: Vec<(String, String)> = Vec::new();
        loop {
            let mut all: Vec<(&str, &str)> = params.to_vec();
            all.extend(continuation.iter().map(|(k, v)| (k.as_str(), v.as_str())));
            let json = self.request(&all).await?;
            on_response(&json);

            match json.get("continue").and_then(Value::as_object) {
                Some(next) => {
                    continuation = next
                        .iter()
                        .map(|(k, v)| (k.clone(), v.as_str().map(str::to_string).unwrap_or_else(|| v.to_string())))
                        .collect();
                }
                None => return Ok(()),
            }
        }
    }

    /// One API request with retries (exponential backoff) on timeouts, connection errors,
    /// HTTP 429 and 5xx.
    async fn request(&self, params: &[(&str, &str)]) -> Result<Value> {
        let mut attempt = 0;
        loop {
            let response = self
                .client
                .get(&self.api_url)
                .query(&[("format", "json"), ("formatversion", "2")])
                .query(params)
                .send()
                .await
                .and_then(reqwest::Response::error_for_status);

            match response {
                Ok(response) => {
                    let json: Value = response.json().await?;
                    if let Some(error) = json.get("error") {
                        return Err(ScraperError::ApiError(error.to_string()));
                    }
                    return Ok(json);
                }
                Err(e) if attempt < MAX_RETRIES && is_transient(&e) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                }
                Err(e) => return Err(e.into()),
            }
        }
    }
}

fn is_transient(e: &reqwest::Error) -> bool {
    e.is_timeout()
        || e.is_connect()
        || e.status().is_some_and(|s| s.as_u16() == 429 || s.is_server_error())
}

fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
