use serde::{Deserialize, Serialize};

/// Expansion or base game source of a scraped quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrapedQuestSource {
    BaseGame,
    HeartsOfStone,
    BloodAndWine,
}

impl From<ScrapedQuestSource> for quest_db::models::QuestSource {
    fn from(s: ScrapedQuestSource) -> Self {
        match s {
            ScrapedQuestSource::BaseGame => quest_db::models::QuestSource::BaseGame,
            ScrapedQuestSource::HeartsOfStone => quest_db::models::QuestSource::HeartsOfStone,
            ScrapedQuestSource::BloodAndWine => quest_db::models::QuestSource::BloodAndWine,
        }
    }
}

/// Category/type of a scraped quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrapedQuestType {
    MainQuest,
    SecondaryQuest,
    WitcherContract,
    TreasureHunt,
    ScavengerHunt,
}

impl From<ScrapedQuestType> for quest_db::models::QuestType {
    fn from(t: ScrapedQuestType) -> Self {
        match t {
            ScrapedQuestType::MainQuest => quest_db::models::QuestType::MainQuest,
            ScrapedQuestType::SecondaryQuest => quest_db::models::QuestType::SecondaryQuest,
            ScrapedQuestType::WitcherContract => quest_db::models::QuestType::WitcherContract,
            ScrapedQuestType::TreasureHunt => quest_db::models::QuestType::TreasureHunt,
            ScrapedQuestType::ScavengerHunt => quest_db::models::QuestType::ScavengerHunt,
        }
    }
}

/// World region or realm of a scraped quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ScrapedRegion {
    WhiteOrchard,
    Velen,
    Novigrad,
    Skellige,
    KaerMorhen,
    Toussaint,
    OxenFurtSewers,
    Unknown,
}

impl From<ScrapedRegion> for quest_db::models::Region {
    fn from(r: ScrapedRegion) -> Self {
        match r {
            ScrapedRegion::WhiteOrchard => quest_db::models::Region::WhiteOrchard,
            ScrapedRegion::Velen => quest_db::models::Region::Velen,
            ScrapedRegion::Novigrad => quest_db::models::Region::Novigrad,
            ScrapedRegion::Skellige => quest_db::models::Region::Skellige,
            ScrapedRegion::KaerMorhen => quest_db::models::Region::KaerMorhen,
            ScrapedRegion::Toussaint => quest_db::models::Region::Toussaint,
            ScrapedRegion::OxenFurtSewers => quest_db::models::Region::OxenFurtSewers,
            ScrapedRegion::Unknown => quest_db::models::Region::Unknown,
        }
    }
}

/// In-memory representation of a quest scraped from the Witcher Fandom Wiki.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrapedQuest {
    pub name: String,
    pub source: ScrapedQuestSource,
    pub quest_type: ScrapedQuestType,
    pub region: ScrapedRegion,
    pub recommended_level: Option<i32>,
    pub is_unmarked: bool,
    pub sort_order: Option<i32>,
    pub description: Option<String>,
    pub cutoff_quest_name: Option<String>,
    pub prerequisite_quest_names: Vec<String>,
    pub wiki_url: String,
}

impl ScrapedQuest {
    /// Converts this in-memory scraped quest into a `quest_db::NewQuest` for insertion.
    pub fn into_new_quest(
        self,
        cutoff_quest_id: Option<i64>,
        prerequisite_ids: Vec<i64>,
    ) -> quest_db::models::NewQuest {
        quest_db::models::NewQuest {
            name: self.name,
            source: self.source.into(),
            quest_type: self.quest_type.into(),
            region: self.region.into(),
            recommended_level: self.recommended_level,
            sort_order: self.sort_order,
            description: self.description,
            is_unmarked: self.is_unmarked,
            cutoff_quest_id,
            prerequisite_ids,
        }
    }
}

/// In-memory container for holding collections of scraped quests with filtering helper methods.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrapedQuestStore {
    quests: Vec<ScrapedQuest>,
}

impl ScrapedQuestStore {
    /// Constructs a new store wrapping a vector of scraped quests.
    pub fn new(quests: Vec<ScrapedQuest>) -> Self {
        Self { quests }
    }

    /// Returns a slice of all stored quests.
    pub fn quests(&self) -> &[ScrapedQuest] {
        &self.quests
    }

    /// Adds a quest to the in-memory store.
    pub fn push(&mut self, quest: ScrapedQuest) {
        self.quests.push(quest);
    }

    /// Returns the total number of stored quests.
    pub fn len(&self) -> usize {
        self.quests.len()
    }

    /// Returns true if the store contains no quests.
    pub fn is_empty(&self) -> bool {
        self.quests.is_empty()
    }

    /// Filters in-memory quests by expansion source.
    pub fn filter_by_source(&self, source: ScrapedQuestSource) -> Vec<&ScrapedQuest> {
        self.quests.iter().filter(|q| q.source == source).collect()
    }

    /// Filters in-memory quests by category/type.
    pub fn filter_by_type(&self, quest_type: ScrapedQuestType) -> Vec<&ScrapedQuest> {
        self.quests.iter().filter(|q| q.quest_type == quest_type).collect()
    }

    /// Filters in-memory quests by world region.
    pub fn filter_by_region(&self, region: ScrapedRegion) -> Vec<&ScrapedQuest> {
        self.quests.iter().filter(|q| q.region == region).collect()
    }

    /// Finds a quest by case-insensitive name match.
    pub fn find_by_name(&self, name: &str) -> Option<&ScrapedQuest> {
        self.quests
            .iter()
            .find(|q| q.name.eq_ignore_ascii_case(name))
    }
}
