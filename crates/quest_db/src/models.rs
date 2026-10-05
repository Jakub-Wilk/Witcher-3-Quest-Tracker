use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Playthrough difficulty level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Difficulty {
    JustTheStory,
    StoryAndSword,
    BloodAndBrokenBones,
    DeathMarch,
    Custom,
}

/// Expansion or base game source of a quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuestSource {
    BaseGame,
    HeartsOfStone,
    BloodAndWine,
}

/// Category/type of quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuestType {
    MainQuest,
    SecondaryQuest,
    WitcherContract,
    TreasureHunt,
    ScavengerHunt,
}

/// World region or realm where the quest takes place.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Region {
    WhiteOrchard,
    Velen,
    Novigrad,
    Skellige,
    KaerMorhen,
    Toussaint,
    OxenFurtSewers,
    Unknown,
}

/// Completion status of a quest within a specific playthrough.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuestStatus {
    NotStarted,
    InProgress,
    Completed,
    Failed,
}

macro_rules! impl_sql_enum {
    ($ty:ident { $($variant:ident => $str_val:expr),* $(,)? }) => {
        impl rusqlite::types::ToSql for $ty {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                let s = match self {
                    $( Self::$variant => $str_val, )*
                };
                Ok(rusqlite::types::ToSqlOutput::from(s))
            }
        }

        impl rusqlite::types::FromSql for $ty {
            fn column_result(value: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                let s = value.as_str()?;
                match s {
                    $( $str_val => Ok(Self::$variant), )*
                    _ => Err(rusqlite::types::FromSqlError::Other(
                        format!("Unknown {} variant: {}", stringify!($ty), s).into(),
                    )),
                }
            }
        }
    };
}

impl_sql_enum!(Difficulty {
    JustTheStory => "JustTheStory",
    StoryAndSword => "StoryAndSword",
    BloodAndBrokenBones => "BloodAndBrokenBones",
    DeathMarch => "DeathMarch",
    Custom => "Custom",
});

impl_sql_enum!(QuestSource {
    BaseGame => "BaseGame",
    HeartsOfStone => "HeartsOfStone",
    BloodAndWine => "BloodAndWine",
});

impl_sql_enum!(QuestType {
    MainQuest => "MainQuest",
    SecondaryQuest => "SecondaryQuest",
    WitcherContract => "WitcherContract",
    TreasureHunt => "TreasureHunt",
    ScavengerHunt => "ScavengerHunt",
});

impl_sql_enum!(Region {
    WhiteOrchard => "WhiteOrchard",
    Velen => "Velen",
    Novigrad => "Novigrad",
    Skellige => "Skellige",
    KaerMorhen => "KaerMorhen",
    Toussaint => "Toussaint",
    OxenFurtSewers => "OxenFurtSewers",
    Unknown => "Unknown",
});

impl_sql_enum!(QuestStatus {
    NotStarted => "NotStarted",
    InProgress => "InProgress",
    Completed => "Completed",
    Failed => "Failed",
});

/// Entity representing a game playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playthrough {
    pub id: i64,
    pub name: String,
    pub difficulty: Difficulty,
    pub is_new_game_plus: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO for creating a new playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPlaythrough {
    pub name: String,
    pub difficulty: Difficulty,
    pub is_new_game_plus: bool,
    pub notes: Option<String>,
}

/// DTO for updating an existing playthrough.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaythroughUpdate {
    pub name: Option<String>,
    pub difficulty: Option<Difficulty>,
    pub is_new_game_plus: Option<bool>,
    pub notes: Option<Option<String>>,
}

/// Entity representing static quest reference data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quest {
    pub id: i64,
    pub name: String,
    pub source: QuestSource,
    pub quest_type: QuestType,
    pub region: Region,
    pub recommended_level: Option<i32>,
    pub is_failable: bool,
    pub sort_order: Option<i32>,
    pub description: Option<String>,
    pub is_unmarked: bool,
    pub cutoff_quest_id: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO for creating a new quest reference entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewQuest {
    pub name: String,
    pub source: QuestSource,
    pub quest_type: QuestType,
    pub region: Region,
    pub recommended_level: Option<i32>,
    pub is_failable: bool,
    pub sort_order: Option<i32>,
    pub description: Option<String>,
    pub is_unmarked: bool,
    pub cutoff_quest_id: Option<i64>,
}

/// Filter criteria for querying quests.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestFilter {
    pub source: Option<QuestSource>,
    pub quest_type: Option<QuestType>,
    pub region: Option<Region>,
    pub is_failable: Option<bool>,
    pub is_unmarked: Option<bool>,
    pub max_recommended_level: Option<i32>,
}

/// Entity representing progress tracking of a quest for a specific playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestProgress {
    pub id: i64,
    pub playthrough_id: i64,
    pub quest_id: i64,
    pub status: QuestStatus,
    pub notes: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO for creating or upserting quest progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewQuestProgress {
    pub playthrough_id: i64,
    pub quest_id: i64,
    pub status: QuestStatus,
    pub notes: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// DTO for updating quest progress.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestProgressUpdate {
    pub status: Option<QuestStatus>,
    pub notes: Option<Option<String>>,
    pub started_at: Option<Option<DateTime<Utc>>>,
    pub completed_at: Option<Option<DateTime<Utc>>>,
}

/// Filter criteria for progress queries within a playthrough.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressFilter {
    pub status: Option<QuestStatus>,
    pub source: Option<QuestSource>,
    pub quest_type: Option<QuestType>,
    pub region: Option<Region>,
}

/// Summary counts of quest progress for a playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct CompletionSummary {
    pub total_quests: i64,
    pub not_started: i64,
    pub in_progress: i64,
    pub completed: i64,
    pub failed: i64,
}
