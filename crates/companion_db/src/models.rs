use chrono::{DateTime, NaiveDateTime, Utc};
use serde::{Deserialize, Serialize};

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
    Oxenfurt,
    Skellige,
    KaerMorhen,
    Vizima,
    Toussaint,
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
    Oxenfurt => "Oxenfurt",
    Skellige => "Skellige",
    KaerMorhen => "KaerMorhen",
    Vizima => "Vizima",
    Toussaint => "Toussaint",
    Unknown => "Unknown",
});

impl_sql_enum!(QuestStatus {
    NotStarted => "NotStarted",
    InProgress => "InProgress",
    Completed => "Completed",
    Failed => "Failed",
});

/// Links a playthrough to a save-game lineage (every save of one in-game run).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveLink {
    /// Packed time of the run's new-game event, shared by all of its saves.
    pub lineage_root: i64,
    /// `saveInfo.playthroughId` (game version 4.0 and later).
    pub game_playthrough_id: Option<String>,
    /// When the run was started, local time.
    pub started_at: Option<NaiveDateTime>,
}

/// The newest save applied to a playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadSave {
    /// Packed save time; orders saves chronologically.
    pub key: i64,
    pub file: String,
    pub saved_at: Option<NaiveDateTime>,
}

/// Entity representing a game playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Playthrough {
    pub id: i64,
    pub name: String,
    pub is_new_game_plus: bool,
    pub notes: Option<String>,
    pub link: Option<SaveLink>,
    pub head: Option<HeadSave>,
    /// Packed time of the save the inventory was read from.
    pub inventory_save_key: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// DTO for creating a new playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewPlaythrough {
    pub name: String,
    pub is_new_game_plus: bool,
    pub notes: Option<String>,
    pub link: Option<SaveLink>,
}

/// DTO for updating an existing playthrough.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlaythroughUpdate {
    pub name: Option<String>,
    pub is_new_game_plus: Option<bool>,
    pub notes: Option<Option<String>>,
}

/// A quest with its text in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Quest {
    pub id: i64,
    /// Depot path of the quest's `.journal` file — the key save games use.
    pub journal_path: String,
    /// Internal name, e.g. `Q001 Dream`.
    pub base_name: String,
    pub source: QuestSource,
    pub quest_type: QuestType,
    pub region: Region,
    pub recommended_level: Option<i32>,
    pub sort_order: Option<i32>,
    pub wiki_page_id: Option<i64>,
    pub wiki_title: Option<String>,
    /// Missable-quest warnings, newline separated.
    pub important_notes: Option<String>,
    pub is_unmarked: bool,
    pub cutoff_quest_id: Option<i64>,
    pub prerequisite_ids: Vec<i64>,
    /// Title in the requested language, falling back to English.
    pub title: String,
    /// Journal description in the requested language, falling back to English.
    pub description: Option<String>,
}

/// `journal_path` prefix of quests known only from the wiki (no journal entry, so saves cannot
/// report them), followed by the wiki page id.
pub const WIKI_ONLY_PREFIX: &str = "wiki:";

impl Quest {
    /// Whether saves can report this quest's status.
    pub fn is_trackable(&self) -> bool {
        !self.journal_path.starts_with(WIKI_ONLY_PREFIX)
    }
}

/// A quest's text in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestText {
    pub language: String,
    pub title: String,
    pub description: Option<String>,
}

/// DTO for creating or updating a quest's static data. Cutoff, prerequisites and sort order are
/// set separately once every quest has an id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewQuest {
    pub journal_path: String,
    pub journal_guid: String,
    pub base_name: String,
    pub source: QuestSource,
    pub quest_type: QuestType,
    pub region: Region,
    pub recommended_level: Option<i32>,
    pub wiki_page_id: Option<i64>,
    pub wiki_title: Option<String>,
    pub important_notes: Option<String>,
    pub is_unmarked: bool,
    pub texts: Vec<QuestText>,
    /// Further journal files that are part of this quest (see `quest_journals`).
    pub extra_journal_paths: Vec<String>,
}

/// A quest's state within a playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuestProgress {
    pub playthrough_id: i64,
    pub quest_id: i64,
    /// Set by the user; wins over `save_status`.
    pub manual_status: Option<QuestStatus>,
    /// Read from the playthrough's newest save.
    pub save_status: Option<QuestStatus>,
    pub notes: Option<String>,
    pub completed_at: Option<DateTime<Utc>>,
}

impl QuestProgress {
    /// The status to show: the manual one if set, else the save's, else not started.
    pub fn status(&self) -> QuestStatus {
        self.manual_status.or(self.save_status).unwrap_or(QuestStatus::NotStarted)
    }

    /// Whether a manual status hides a different status read from the save.
    pub fn overrides_save(&self) -> bool {
        matches!((self.manual_status, self.save_status), (Some(m), Some(s)) if m != s)
    }
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

/// The expansion an item or recipe ships with; `None` means the base game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Expansion {
    HeartsOfStone,
    BloodAndWine,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RecipeKind {
    Crafting,
    Alchemy,
    Enchanting,
}

/// Which recipe set applies: New Game+ replaces some definitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RecipeMode {
    Base,
    Plus,
}

impl_sql_enum!(Expansion {
    HeartsOfStone => "HeartsOfStone",
    BloodAndWine => "BloodAndWine",
});

impl_sql_enum!(RecipeKind {
    Crafting => "Crafting",
    Alchemy => "Alchemy",
    Enchanting => "Enchanting",
});

impl_sql_enum!(RecipeMode {
    Base => "Base",
    Plus => "Plus",
});

/// DTO for creating or updating an item from the game files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewItem {
    /// Slug of the internal name, e.g. `swallow-2`.
    pub id: String,
    /// The game's internal name, as saves spell it.
    pub internal_name: String,
    pub name_key: String,
    pub category: String,
    /// Rarity, 1 (common) to 5 (set).
    pub quality: u8,
    pub expansion: Option<Expansion>,
    pub in_base: bool,
    pub in_plus: bool,
}

/// An item with its name in one language.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub id: String,
    pub internal_name: String,
    /// Name in the requested language, falling back to English and then the internal name.
    pub name: String,
    /// English name, for search in every language.
    pub english_name: String,
    pub category: String,
    pub quality: u8,
    pub expansion: Option<Expansion>,
    pub has_image: bool,
}

/// One ingredient line of a recipe.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Ingredient {
    pub item_id: String,
    pub qty: u32,
}

/// DTO for storing a recipe from the game files.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewRecipe {
    pub mode: RecipeMode,
    pub product_id: String,
    pub kind: RecipeKind,
    pub level: Option<String>,
    pub craftsman: Option<String>,
    pub yield_qty: u32,
    pub expansion: Option<Expansion>,
    /// The primary ingredient set first, then alternatives.
    pub variants: Vec<Vec<Ingredient>>,
}

/// A recipe, identified by its product.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Recipe {
    pub product_id: String,
    pub kind: RecipeKind,
    /// Crafting: `Journeyman`, `Master`, `GrandMaster`, `ArchMaster`; enchanting: `1`-`3`.
    pub level: Option<String>,
    /// Crafting: `Smith`, `Armorer`, `Crafter`.
    pub craftsman: Option<String>,
    pub yield_qty: u32,
    pub expansion: Option<Expansion>,
    /// The primary ingredient set first, then alternatives; never empty.
    pub variants: Vec<Vec<Ingredient>>,
}

impl Recipe {
    /// The ingredients of `variant`, or of the primary set if there is no such variant.
    pub fn ingredients(&self, variant: usize) -> &[Ingredient] {
        self.variants.get(variant).or(self.variants.first()).map_or(&[], Vec::as_slice)
    }
}

/// A craft planned in a playthrough.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QueueEntry {
    /// The product's item id (recipes are identified by their product).
    pub item_id: String,
    /// How many times to craft it.
    pub qty: u32,
    /// Which ingredient set to use (0 = primary).
    pub variant: usize,
}

/// A stored item picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemImage {
    pub image: Vec<u8>,
    pub content_type: String,
    pub wiki_file: String,
    pub sha1: String,
}
