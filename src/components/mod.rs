mod cutoff_pane;
mod header;
mod playthrough_modal;
mod quest_card;
mod quest_list;
mod sidebar;

pub use cutoff_pane::CutoffPane;
pub use header::Header;
pub use playthrough_modal::PlaythroughModal;
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
        Region::Oxenfurt => "Oxenfurt",
        Region::Skellige => "Skellige",
        Region::KaerMorhen => "Kaer Morhen",
        Region::Vizima => "Vizima",
        Region::Toussaint => "Toussaint",
        Region::Unknown => "Unknown region",
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

/// URL of a quest's page on the Witcher wiki.
pub fn wiki_url(wiki_title: &str) -> String {
    let mut url = String::from("https://witcher.fandom.com/wiki/");
    for byte in wiki_title.replace(' ', "_").bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b':' | b'(' | b')'
            | b'!' | b',' | b'\'' | b'*' => url.push(byte as char),
            _ => url.push_str(&format!("%{byte:02X}")),
        }
    }
    url
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wiki_url_encodes_reserved_characters() {
        assert_eq!(
            wiki_url("What Was This About Again?"),
            "https://witcher.fandom.com/wiki/What_Was_This_About_Again%3F"
        );
        assert_eq!(wiki_url("The Last Wish (quest)"), "https://witcher.fandom.com/wiki/The_Last_Wish_(quest)");
        assert_eq!(wiki_url("Ä"), "https://witcher.fandom.com/wiki/%C3%84");
    }
}
