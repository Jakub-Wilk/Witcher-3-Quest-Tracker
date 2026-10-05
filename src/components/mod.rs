mod cutoff_pane;
mod header;
mod new_playthrough_modal;
mod quest_card;
mod quest_list;
mod sidebar;

pub use cutoff_pane::CutoffPane;
pub use header::Header;
pub use new_playthrough_modal::NewPlaythroughModal;
pub use quest_card::QuestCard;
pub use quest_list::QuestList;
pub use sidebar::Sidebar;

use quest_db::{Difficulty, QuestSource, QuestStatus, QuestType, Region};

pub const SOURCES: [QuestSource; 3] =
    [QuestSource::BaseGame, QuestSource::HeartsOfStone, QuestSource::BloodAndWine];

pub const QUEST_TYPES: [QuestType; 5] = [
    QuestType::MainQuest,
    QuestType::SecondaryQuest,
    QuestType::WitcherContract,
    QuestType::TreasureHunt,
    QuestType::ScavengerHunt,
];

pub const DIFFICULTIES: [Difficulty; 5] = [
    Difficulty::JustTheStory,
    Difficulty::StoryAndSword,
    Difficulty::BloodAndBrokenBones,
    Difficulty::DeathMarch,
    Difficulty::Custom,
];

pub fn source_label(source: QuestSource) -> &'static str {
    match source {
        QuestSource::BaseGame => "Base",
        QuestSource::HeartsOfStone => "HoS",
        QuestSource::BloodAndWine => "B&W",
    }
}

pub fn type_label(quest_type: QuestType) -> &'static str {
    match quest_type {
        QuestType::MainQuest => "Main",
        QuestType::SecondaryQuest => "Secondary",
        QuestType::WitcherContract => "Contracts",
        QuestType::TreasureHunt => "Treasure Hunts",
        QuestType::ScavengerHunt => "Scavenger Hunts",
    }
}

pub fn region_label(region: Region) -> &'static str {
    match region {
        Region::WhiteOrchard => "White Orchard",
        Region::Velen => "Velen",
        Region::Novigrad => "Novigrad",
        Region::Skellige => "Skellige",
        Region::KaerMorhen => "Kaer Morhen",
        Region::Toussaint => "Toussaint",
        Region::OxenFurtSewers => "Oxenfurt",
        Region::Unknown => "Unknown",
    }
}

pub fn difficulty_label(difficulty: Difficulty) -> &'static str {
    match difficulty {
        Difficulty::JustTheStory => "Just the Story",
        Difficulty::StoryAndSword => "Story and Sword",
        Difficulty::BloodAndBrokenBones => "Blood and Broken Bones!",
        Difficulty::DeathMarch => "Death March!",
        Difficulty::Custom => "Custom",
    }
}

/// Completed or failed quests can no longer be locked out.
pub fn is_open(status: QuestStatus) -> bool {
    matches!(status, QuestStatus::NotStarted | QuestStatus::InProgress)
}
