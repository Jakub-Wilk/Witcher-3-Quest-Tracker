use crate::models::{
    ScrapedQuest, ScrapedQuestSource, ScrapedQuestStore, ScrapedQuestType, ScrapedRegion,
};

/// Provides an offline mock `ScrapedQuestStore` populated with realistic sample data.
pub fn mock_sample_quests() -> ScrapedQuestStore {
    ScrapedQuestStore::new(vec![
        ScrapedQuest {
            name: "Lilac and Gooseberries".into(),
            source: ScrapedQuestSource::BaseGame,
            quest_type: ScrapedQuestType::MainQuest,
            region: ScrapedRegion::WhiteOrchard,
            recommended_level: Some(1),
            is_unmarked: false,
            sort_order: Some(1),
            description: Some(
                "Geralt and Vesemir track Yennefer through White Orchard while searching for clues."
                    .into(),
            ),
            cutoff_quest_name: None,
            prerequisite_quest_names: vec![],
            wiki_url: "https://witcher.fandom.com/wiki/Lilac_and_Gooseberries".into(),
        },
        ScrapedQuest {
            name: "The Last Wish".into(),
            source: ScrapedQuestSource::BaseGame,
            quest_type: ScrapedQuestType::SecondaryQuest,
            region: ScrapedRegion::Skellige,
            recommended_level: Some(15),
            is_unmarked: false,
            sort_order: Some(5),
            description: Some(
                "Geralt helps Yennefer track down a djinn in Skellige to sever their magic bond."
                    .into(),
            ),
            cutoff_quest_name: Some("Isle of Mists".into()),
            prerequisite_quest_names: vec!["Nameless".into()],
            wiki_url: "https://witcher.fandom.com/wiki/The_Last_Wish".into(),
        },
        ScrapedQuest {
            name: "Witch Hunter Raids".into(),
            source: ScrapedQuestSource::BaseGame,
            quest_type: ScrapedQuestType::SecondaryQuest,
            region: ScrapedRegion::Novigrad,
            recommended_level: Some(12),
            is_unmarked: true,
            sort_order: Some(10),
            description: Some(
                "An unmarked secondary quest involving Witch Hunters searching Novigrad residences."
                    .into(),
            ),
            cutoff_quest_name: Some("Now or Never".into()),
            prerequisite_quest_names: vec!["Pyres of Novigrad".into()],
            wiki_url: "https://witcher.fandom.com/wiki/Witch_Hunter_Raids".into(),
        },
    ])
}
