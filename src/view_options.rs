//! Quest list filtering and sorting options, persisted in [`crate::settings::AppSettings`].

use std::collections::HashMap;

use quest_db::{Quest, QuestProgress, QuestSource, QuestStatus, QuestType, Region};
use serde::{Deserialize, Serialize};

/// Quest statuses in the order the status filter shows them.
pub const STATUSES: [QuestStatus; 4] =
    [QuestStatus::NotStarted, QuestStatus::InProgress, QuestStatus::Completed, QuestStatus::Failed];

/// Whether `value` passes a multi-select filter; an empty selection allows everything.
fn allows<T: PartialEq>(selected: &[T], value: T) -> bool {
    selected.is_empty() || selected.contains(&value)
}

/// Adds `value` to a multi-select filter, or removes it if already selected.
pub fn toggle<T: PartialEq>(selected: &mut Vec<T>, value: T) {
    match selected.iter().position(|v| *v == value) {
        Some(i) => {
            selected.remove(i);
        }
        None => selected.push(value),
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
    /// By the story order of the quest's cutoff point, earliest first; no cutoff last.
    Cutoff,
}

impl SortKey {
    pub const ALL: [SortKey; 5] = [SortKey::Story, SortKey::Level, SortKey::Name, SortKey::Region, SortKey::Cutoff];

    pub fn label(self) -> &'static str {
        match self {
            SortKey::Story => "Story order",
            SortKey::Level => "Level",
            SortKey::Name => "Name",
            SortKey::Region => "Region",
            SortKey::Cutoff => "Cutoff",
        }
    }
}

/// Filters and sort order for the quest list. Everything but the search text is persisted.
/// Each filter is a multi-select: a quest passes when it matches any selected value, and an
/// empty selection means "all".
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ViewOptions {
    pub sources: Vec<QuestSource>,
    pub quest_types: Vec<QuestType>,
    pub regions: Vec<Region>,
    pub statuses: Vec<QuestStatus>,
    pub sort: SortKey,
    #[serde(skip)]
    pub search: String,
}

impl ViewOptions {
    /// Whether a quest passes the expansion and region filters (used for category counts).
    pub fn matches_scope(&self, quest: &Quest) -> bool {
        allows(&self.sources, quest.source) && allows(&self.regions, quest.region)
    }

    pub fn matches(&self, quest: &Quest, status: QuestStatus) -> bool {
        let search = self.search.trim().to_lowercase();
        self.matches_scope(quest)
            && allows(&self.quest_types, quest.quest_type)
            && allows(&self.statuses, status)
            && (search.is_empty()
                || quest.title.to_lowercase().contains(&search)
                || quest.base_name.to_lowercase().contains(&search))
    }

    /// Filters `rows` and returns them in the selected order.
    pub fn apply<'a>(&self, rows: &'a [(Quest, QuestProgress)]) -> Vec<&'a (Quest, QuestProgress)> {
        let mut visible: Vec<_> = rows.iter().filter(|(q, p)| self.matches(q, p.status())).collect();
        let story = |q: &Quest| q.sort_order.unwrap_or(i32::MAX);
        match self.sort {
            SortKey::Story => visible.sort_by_key(|(q, _)| story(q)),
            SortKey::Level => {
                visible.sort_by_key(|(q, _)| (q.recommended_level.unwrap_or(i32::MAX), story(q)))
            }
            SortKey::Name => visible.sort_by_cached_key(|(q, _)| q.title.to_lowercase()),
            SortKey::Region => visible.sort_by_key(|(q, _)| (region_rank(q.region), story(q))),
            SortKey::Cutoff => {
                // Look cutoffs up in all rows: the cutoff quest itself may be filtered out.
                let cutoff_order: HashMap<i64, i32> = rows.iter().map(|(q, _)| (q.id, story(q))).collect();
                let cutoff = |q: &Quest| match q.cutoff_quest_id {
                    Some(id) => (false, cutoff_order.get(&id).copied().unwrap_or(i32::MAX)),
                    None => (true, i32::MAX),
                };
                visible.sort_by_key(|(q, _)| (cutoff(q), story(q)))
            }
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

    fn row(id: i64, name: &str, level: Option<i32>, region: Region, order: i32, status: QuestStatus) -> (Quest, QuestProgress) {
        (
            Quest {
                id,
                journal_path: format!("q{id}.journal"),
                base_name: format!("Q{id}"),
                source: QuestSource::BaseGame,
                quest_type: QuestType::MainQuest,
                region,
                recommended_level: level,
                sort_order: Some(order),
                wiki_page_id: None,
                wiki_title: None,
                important_notes: None,
                is_unmarked: false,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
                title: name.into(),
                description: None,
            },
            QuestProgress {
                playthrough_id: 1,
                quest_id: id,
                manual_status: Some(status),
                save_status: None,
                notes: None,
                completed_at: None,
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
            row(4, "Delta", Some(20), Region::Toussaint, 4, QuestStatus::InProgress),
        ];
        let mut options = ViewOptions::default();
        assert_eq!(ids(&options, &rows), vec![2, 1, 3, 4]);

        options.sort = SortKey::Level;
        assert_eq!(ids(&options, &rows), vec![3, 1, 4, 2]);
        options.sort = SortKey::Name;
        assert_eq!(ids(&options, &rows), vec![2, 1, 3, 4]);
        options.sort = SortKey::Region;
        assert_eq!(ids(&options, &rows), vec![2, 3, 1, 4]);

        options.sort = SortKey::Cutoff;
        let mut cut = rows.clone();
        cut[0].0.cutoff_quest_id = Some(4);
        cut[2].0.cutoff_quest_id = Some(2);
        assert_eq!(ids(&options, &cut), vec![3, 1, 2, 4]);
        options.sort = SortKey::Region;

        options.statuses = vec![QuestStatus::NotStarted, QuestStatus::InProgress];
        assert_eq!(ids(&options, &rows), vec![2, 4]);
        options.statuses = vec![QuestStatus::InProgress];
        assert_eq!(ids(&options, &rows), vec![4]);
        options.statuses.clear();
        options.regions = vec![Region::Velen, Region::Skellige];
        assert_eq!(ids(&options, &rows), vec![2, 3, 1]);
        options.regions = vec![Region::Velen];
        options.search = "CHAR".into();
        assert_eq!(ids(&options, &rows), vec![3]);
    }

    #[test]
    fn toggles_selection() {
        let mut selected = vec![];
        toggle(&mut selected, Region::Velen);
        toggle(&mut selected, Region::Skellige);
        assert_eq!(selected, vec![Region::Velen, Region::Skellige]);
        toggle(&mut selected, Region::Velen);
        assert_eq!(selected, vec![Region::Skellige]);
    }
}
