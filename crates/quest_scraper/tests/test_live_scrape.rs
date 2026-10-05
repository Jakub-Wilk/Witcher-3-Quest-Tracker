use quest_scraper::WikiScraperClient;

#[tokio::test]
async fn test_live_scrape_witch_hunter_raids() {
    let client = WikiScraperClient::new().expect("failed to create client");
    match client.fetch_quest("Witch_Hunter_Raids").await {
        Ok(quest) => {
            println!("\n=== SCRAPED QUEST DATA ===");
            println!("{:#?}", quest);
            println!("===========================\n");
            assert_eq!(quest.name, "Witch Hunter Raids");
        }
        Err(e) => {
            panic!("Failed to scrape live quest: {}", e);
        }
    }
}
