//! Quest list filtering and sorting options, persisted to a TOML file next to the DB.

use std::path::Path;

use quest_db::{Quest, QuestProgress, QuestSource, QuestStatus, QuestType, Region};
use serde::{Deserialize, Serialize};

/// Which completion states to show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum StatusFilter {
    #[default]
    All,
    /// Not started or in progress.
    Open,
    Completed,
    Failed,
}

impl StatusFilter {
    pub const ALL: [StatusFilter; 4] =
        [StatusFilter::All, StatusFilter::Open, StatusFilter::Completed, StatusFilter::Failed];

    pub fn label(self) -> &'static str {
        match self {
            StatusFilter::All => "All",
            StatusFilter::Open => "Open",
            StatusFilter::Completed => "Completed",
            StatusFilter::Failed => "Failed",
        }
    }

    fn matches(self, status: QuestStatus) -> bool {
        match self {
            StatusFilter::All => true,
            StatusFilter::Open => matches!(status, QuestStatus::NotStarted | QuestStatus::InProgress),
            StatusFilter::Completed => status == QuestStatus::Completed,
            StatusFilter::Failed => status == QuestStatus::Failed,
        }
    }
}

/// Quest list ordering.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SortKey {
    /// The story order computed at sync.
    #[default]
    Story,
    Level,
    Name,
    Region,
}

impl SortKey {
    pub const ALL: [SortKey; 4] = [SortKey::Story, SortKey::Level, SortKey::Name, SortKey::Region];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Story => "Story order",
            SortKey::Level => "Level",
            SortKey::Name => "Name",
            SortKey::Region => "Region",
        }
    }
}

/// Filters and sort order for the quest list. Everything but the search text is persisted.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOptions {
    pub source: Option<QuestSource>,
    pub quest_type: Option<QuestType>,
    pub region: Option<Region>,
    pub status: StatusFilter,
    pub sort: SortKey,
    #[serde(skip)]
    pub search: String,
}

impl ViewOptions {
    /// Loads options from `path`. A missing file gives defaults; an unreadable or invalid one
    /// is logged and also gives defaults, so a bad settings file never blocks startup.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("Ignoring invalid {}: {e}", path.display());
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!("Could not read {}: {e}", path.display());
                Self::default()
            }
        }
    }

    /// The persisted part of the options as TOML.
    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("view options always serialize")
    }

    /// Writes the options to `path` via a temporary file, so a crash never leaves a
    /// half-written settings file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml())?;
        std::fs::rename(&tmp, path)
    }

    /// Whether a quest passes every filter except status (used for category counts).
    pub fn matches_scope(&self, quest: &Quest) -> bool {
        self.source.is_none_or(|s| s == quest.source) && self.region.is_none_or(|r| r == quest.region)
    }

    pub fn matches(&self, quest: &Quest, status: QuestStatus) -> bool {
        let search = self.search.trim().to_lowercase();
        self.matches_scope(quest)
            && self.quest_type.is_none_or(|t| t == quest.quest_type)
            && self.status.matches(status)
            && (search.is_empty()
                || quest.name.to_lowercase().contains(&search)
                || quest.localized_name.as_ref().is_some_and(|n| n.to_lowercase().contains(&search)))
    }

    /// Filters `rows` and returns them in the selected order.
    pub fn apply<'a>(&self, rows: &'a [(Quest, QuestProgress)]) -> Vec<&'a (Quest, QuestProgress)> {
        let mut visible: Vec<_> = rows.iter().filter(|(q, p)| self.matches(q, p.status)).collect();
        let story = |q: &Quest| q.sort_order.unwrap_or(i32::MAX);
        match self.sort {
            SortKey::Story => visible.sort_by_key(|(q, _)| story(q)),
            SortKey::Level => {
                visible.sort_by_key(|(q, _)| (q.recommended_level.unwrap_or(i32::MAX), story(q)))
            }
            SortKey::Name => visible.sort_by_cached_key(|(q, _)| q.display_name().to_lowercase()),
            SortKey::Region => visible.sort_by_key(|(q, _)| (region_rank(q.region), story(q))),
        }
        visible
    }
}

/// Regions in rough travel order, with Unknown last.
pub const REGIONS: [Region; 9] = [
    Region::WhiteOrchard,
    Region::Vizima,
    Region::Velen,
    Region::Novigrad,
    Region::Oxenfurt,
    Region::Skellige,
    Region::KaerMorhen,
    Region::Toussaint,
    Region::Unknown,
];

fn region_rank(region: Region) -> usize {
    REGIONS.iter().position(|&r| r == region).unwrap_or(REGIONS.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn row(id: i64, name: &str, level: Option<i32>, region: Region, order: i32, status: QuestStatus) -> (Quest, QuestProgress) {
        let now = Utc::now();
        (
            Quest {
                id,
                wiki_page_id: id,
                wiki_title: name.into(),
                name: name.into(),
                localized_name: None,
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region,
                recommended_level: level,
                sort_order: Some(order),
                description: None,
                important_notes: None,
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
                created_at: now,
                updated_at: now,
            },
            QuestProgress {
                id: 0,
                playthrough_id: 1,
                quest_id: id,
                status,
                notes: None,
                started_at: None,
                completed_at: None,
                created_at: now,
                updated_at: now,
            },
        )
    }

    fn ids(options: &ViewOptions, rows: &[(Quest, QuestProgress)]) -> Vec<i64> {
        options.apply(rows).iter().map(|(q, _)| q.id).collect()
    }

    #[test]
    fn filters_and_sorts() {
        let rows = vec![
            row(1, "Bravo", Some(10), Region::Skellige, 2, QuestStatus::Completed),
            row(2, "alpha", None, Region::Velen, 1, QuestStatus::NotStarted),
            row(3, "Charlie", Some(5), Region::Velen, 3, QuestStatus::Failed),
        ];
        let mut options = ViewOptions::default();
        assert_eq!(ids(&options, &rows), vec![2, 1, 3]);

        options.sort = SortKey::Level;
        assert_eq!(ids(&options, &rows), vec![3, 1, 2]);
        options.sort = SortKey::Name;
        assert_eq!(ids(&options, &rows), vec![2, 1, 3]);
        options.sort = SortKey::Region;
        assert_eq!(ids(&options, &rows), vec![2, 3, 1]);

        options.status = StatusFilter::Open;
        assert_eq!(ids(&options, &rows), vec![2]);
        options.status = StatusFilter::All;
        options.region = Some(Region::Velen);
        options.search = "CHAR".into();
        assert_eq!(ids(&options, &rows), vec![3]);
    }

    #[test]
    fn persists_everything_but_search() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let options = ViewOptions {
            source: Some(QuestSource::BloodAndWine),
            quest_type: Some(QuestType::WitcherContract),
            region: Some(Region::Toussaint),
            status: StatusFilter::Open,
            sort: SortKey::Level,
            search: "not saved".into(),
        };
        options.save(&path).unwrap();

        let loaded = ViewOptions::load(&path);
        assert_eq!(loaded, ViewOptions { search: String::new(), ..options });
        assert!(!std::fs::read_to_string(&path).unwrap().contains("not saved"));
    }

    #[test]
    fn missing_or_invalid_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert_eq!(ViewOptions::load(&path), ViewOptions::default());

        std::fs::write(&path, "sort = \"Sideways\"").unwrap();
        assert_eq!(ViewOptions::load(&path), ViewOptions::default());

        // Missing keys fall back individually.
        std::fs::write(&path, "sort = \"Name\"").unwrap();
        assert_eq!(ViewOptions::load(&path), ViewOptions { sort: SortKey::Name, ..Default::default() });
    }
}
