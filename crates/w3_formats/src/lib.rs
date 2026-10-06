//! Readers for The Witcher 3 (Remastered) file formats: save games, game bundles, CR2W
//! resources and localized strings. Everything here only ever opens files for reading.

pub mod bundle;
pub mod catalog;
pub mod cr2w;
mod error;
mod reader;
pub mod save;
pub mod strings;

pub use catalog::{Content, GameCatalog, GameQuest, JournalQuestType, read_game_catalog};
pub use error::{Error, Result};
pub use save::{HistoryRecord, JournalStatus, SaveSnapshot, SaveTime, read_save};
