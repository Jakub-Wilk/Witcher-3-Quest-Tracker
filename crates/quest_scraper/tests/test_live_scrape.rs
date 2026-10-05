//! Live tests against the Witcher wiki.

use quest_scraper::{Language, WikiScraperClient, parse_quest};

#[tokio::test]
async fn live_fetch_and_parse_the_last_wish() {
    let client = WikiScraperClient::new().expect("failed to create client");
    let pages = client
        .fetch_pages(&["The Last Wish (quest)".to_string()], |_, _| {})
        .await
        .expect("fetch failed");
    assert_eq!(pages.len(), 1);

    let quest = parse_quest(&pages[0]).expect("parse failed");
    assert_eq!(quest.name, "The Last Wish");
    assert_eq!(quest.cutoff_titles, vec!["Ugly Baby"]);
    assert!(quest.important_notes.is_some());
}

#[tokio::test]
async fn live_langlinks_polish() {
    let client = WikiScraperClient::new().expect("failed to create client");
    let links = client
        .fetch_langlinks(&["The Last Wish (quest)".to_string()], Language::Polish, |_, _| {})
        .await
        .expect("fetch failed");
    assert_eq!(links.get("The Last Wish (quest)").map(String::as_str), Some("Ostatnie życzenie (zadanie)"));
}
