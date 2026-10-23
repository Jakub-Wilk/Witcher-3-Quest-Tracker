use std::collections::{BTreeMap, HashMap};
use std::time::Duration;

use reqwest::header::{HeaderMap, HeaderValue, USER_AGENT};
use serde_json::Value;

use crate::error::{Result, ScraperError};
use crate::language::Language;
use crate::models::WikiPage;

/// MediaWiki's limit of titles per query for regular clients.
pub(crate) const TITLES_PER_REQUEST: usize = 50;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_RETRIES: u32 = 3;

/// Async client for the Witcher Fandom wiki's MediaWiki API. The quest and item scrapers are in
/// [`crate::quests`] and [`crate::items`].
pub struct WikiClient {
    client: reqwest::Client,
    api_url: String,
}

impl WikiClient {
    pub fn new() -> Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            USER_AGENT,
            HeaderValue::from_static("Witcher3Companion/0.4 (desktop companion app; reqwest)"),
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .timeout(REQUEST_TIMEOUT)
            .build()?;

        Ok(Self { client, api_url: "https://witcher.fandom.com/api.php".into() })
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
    pub(crate) async fn query_all(&self, params: &[(&str, &str)], mut on_response: impl FnMut(&Value)) -> Result<()> {
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

    /// Downloads a file (e.g. an image from the wiki's CDN), with the same retries as API calls.
    pub async fn download(&self, url: &str) -> Result<Vec<u8>> {
        let mut attempt = 0;
        loop {
            let response = self.client.get(url).send().await.and_then(reqwest::Response::error_for_status);
            match response {
                Ok(response) => return Ok(response.bytes().await?.to_vec()),
                Err(e) if attempt < MAX_RETRIES && is_transient(&e) => {
                    attempt += 1;
                    tokio::time::sleep(Duration::from_millis(500 * 2u64.pow(attempt))).await;
                }
                Err(e) => return Err(e.into()),
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

pub(crate) fn array(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or_default()
}
