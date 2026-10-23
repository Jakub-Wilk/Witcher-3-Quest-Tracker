pub mod ornament;
pub mod quests;
pub mod shell;
pub mod workbench;

use companion_db::{QuestSource, QuestStatus, QuestType, Region};

use crate::i18n::Tr;
use crate::t;

pub const SOURCES: [QuestSource; 3] =
    [QuestSource::BaseGame, QuestSource::HeartsOfStone, QuestSource::BloodAndWine];

pub const QUEST_TYPES: [QuestType; 5] = [
    QuestType::MainQuest,
    QuestType::SecondaryQuest,
    QuestType::WitcherContract,
    QuestType::TreasureHunt,
    QuestType::ScavengerHunt,
];

pub fn source_label(tr: Tr, source: QuestSource) -> &'static str {
    match source {
        QuestSource::BaseGame => t!(tr, "source.base"),
        QuestSource::HeartsOfStone => t!(tr, "source.hos"),
        QuestSource::BloodAndWine => t!(tr, "source.baw"),
    }
}

pub fn type_label(tr: Tr, quest_type: QuestType) -> &'static str {
    match quest_type {
        QuestType::MainQuest => t!(tr, "type.main"),
        QuestType::SecondaryQuest => t!(tr, "type.secondary"),
        QuestType::WitcherContract => t!(tr, "type.contract"),
        QuestType::TreasureHunt => t!(tr, "type.treasure"),
        QuestType::ScavengerHunt => t!(tr, "type.scavenger"),
    }
}

pub fn region_label(tr: Tr, region: Region) -> &'static str {
    match region {
        Region::WhiteOrchard => t!(tr, "region.white_orchard"),
        Region::Velen => t!(tr, "region.velen"),
        Region::Novigrad => t!(tr, "region.novigrad"),
        Region::Oxenfurt => t!(tr, "region.oxenfurt"),
        Region::Skellige => t!(tr, "region.skellige"),
        Region::KaerMorhen => t!(tr, "region.kaer_morhen"),
        Region::Vizima => t!(tr, "region.vizima"),
        Region::Toussaint => t!(tr, "region.toussaint"),
        Region::Unknown => t!(tr, "region.unknown"),
    }
}

pub fn status_label(tr: Tr, status: QuestStatus) -> &'static str {
    match status {
        QuestStatus::NotStarted => t!(tr, "status.not_started"),
        QuestStatus::InProgress => t!(tr, "status.in_progress"),
        QuestStatus::Completed => t!(tr, "status.completed"),
        QuestStatus::Failed => t!(tr, "status.failed"),
    }
}

/// A local date and time from a save, e.g. `2026-10-06 00:39`.
pub fn format_local(tr: Tr, time: Option<chrono::NaiveDateTime>) -> String {
    time.map(|t| t.format("%Y-%m-%d %H:%M").to_string()).unwrap_or_else(|| t!(tr, "common.unknown_time").into())
}

/// Opens a folder picker starting at `start`.
pub async fn pick_folder(title: &str, start: Option<std::path::PathBuf>) -> Option<std::path::PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if let Some(start) = start.filter(|s| s.is_dir()) {
        dialog = dialog.set_directory(start);
    }
    dialog.pick_folder().await.map(|f| f.path().to_path_buf())
}

/// Completed or failed quests can no longer be locked out.
pub fn is_open(status: QuestStatus) -> bool {
    matches!(status, QuestStatus::NotStarted | QuestStatus::InProgress)
}

/// URL of a page on the Witcher wiki.
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
