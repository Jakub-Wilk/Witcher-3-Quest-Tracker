//! Witcher 3 `.sav` reader: playthrough lineage and journal (quest) statuses.
//!
//! Layout, outermost first:
//! - `SNFH` `FZLC` chunked LZ4 container: chunk count, header size, then per chunk
//!   (compressed size, decompressed size, end offset). The chunks are raw LZ4 *blocks* whose
//!   output is laid out after `header size` bytes, so offsets inside the save are relative to
//!   the start of the file.
//! - `SAV3` header (save version, game version, ...) and a footer: the last 6 bytes are the
//!   variable table offset and `"SE"`. Ten bytes before the variable table sit the offsets of the
//!   `NM` section (a `MANU` name table, 1-based) and the `RB` section.
//! - The variable table: (offset, size) per serialized variable. Sorted by offset they form a
//!   token stream (`BS` block start, `VL`/`OP` typed values, ...). A `BS` token's size covers its
//!   whole subtree.
//!
//! Two subtrees matter here:
//! - `saveInfo`: `playthroughId` (4.0+) and the save history, a list of
//!   `time`/`type`/`v` records. The first record (type 1) is the new-game event, so its time
//!   identifies the playthrough in every game version; the last one is the save itself.
//! - `CJournalManager` → `JActiveEntries`: one `SJournalEntryStatus` per journal entry the player
//!   has seen, holding the entry's path (`guid`/`resource`/`flags` per level, outermost first)
//!   and its `EJournalStatus`.

use std::collections::HashMap;
use std::path::Path;

use chrono::{NaiveDate, NaiveDateTime};

use crate::error::{Error, Result};
use crate::reader::{Reader, red_string, u16_at, u32_at};

/// A packed save timestamp, local time. The bit layout nests fields from most to least
/// significant, so comparing the raw values compares the times.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SaveTime(pub u64);

impl SaveTime {
    /// Decodes the packed fields: the high word holds `year:12 | month-1:5 | day-1:5`, the low
    /// word `hour:5 | minute:6 | second:6 | millisecond:10`.
    pub fn to_datetime(self) -> Option<NaiveDateTime> {
        let hi = (self.0 >> 32) as u32;
        let lo = self.0 as u32;
        let date = NaiveDate::from_ymd_opt(
            (hi >> 20) as i32,
            ((hi >> 15) & 0x1f) + 1,
            ((hi >> 10) & 0x1f) + 1,
        )?;
        date.and_hms_milli_opt((lo >> 22) & 0x1f, (lo >> 16) & 0x3f, (lo >> 10) & 0x3f, lo & 0x3ff)
    }
}

/// One entry of the save history kept in `saveInfo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HistoryRecord {
    pub time: SaveTime,
    /// 1 = new game started, 2 = saved, 3 = (apparently) loaded.
    pub kind: u8,
    /// Save format version the event happened in.
    pub version: u16,
}

pub const HISTORY_NEW_GAME: u8 = 1;

/// A journal entry's state, as the game stores it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum JournalStatus {
    Inactive,
    Active,
    Success,
    Failed,
}

impl JournalStatus {
    fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "JS_Inactive" => Self::Inactive,
            "JS_Active" => Self::Active,
            "JS_Success" => Self::Success,
            "JS_Failed" => Self::Failed,
            _ => return None,
        })
    }
}

/// Everything the tracker needs from one save.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SaveSnapshot {
    /// First `SAV3` header code: 64 up to 4.0x, 66 in Remastered.
    pub save_version: u32,
    /// Second `SAV3` header code (18/19 for 1.3x, 27 for 4.0x, 29 for Remastered).
    pub game_version: u32,
    /// `saveInfo.playthroughId`, present from 4.0 on.
    pub playthrough_id: Option<String>,
    /// Save history, oldest first; the last record is this save.
    pub history: Vec<HistoryRecord>,
    /// Status of every journal resource the player has seen, keyed by its resource path as the
    /// game spells it (lowercase, backslash separated, e.g.
    /// `gameplay\journal\quests\q001beggining.journal`). Covers quests as well as quest groups,
    /// tutorials, bestiary and so on; phases and objectives (which live inside a quest's file)
    /// are not included.
    pub journal: HashMap<String, JournalStatus>,
}

impl SaveSnapshot {
    /// Time of the new-game event this save descends from: the same for every save of a
    /// playthrough.
    pub fn lineage_root(&self) -> Option<SaveTime> {
        self.history.first().map(|r| r.time)
    }

    /// When this save was made (its own history record).
    pub fn self_key(&self) -> Option<SaveTime> {
        self.history.last().map(|r| r.time)
    }

    /// Whether the save made at `key` is this save or one of its ancestors.
    pub fn descends_from(&self, key: SaveTime) -> bool {
        self.history.iter().any(|r| r.time == key)
    }
}

/// Reads and parses a save file. The file is only opened for reading.
pub fn read_save(path: &Path) -> Result<SaveSnapshot> {
    let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
    parse_save(&bytes)
}

/// Parses an in-memory save file.
pub fn parse_save(bytes: &[u8]) -> Result<SaveSnapshot> {
    let raw = decompress(bytes)?;
    let save = RawSave::parse(&raw)?;
    save.snapshot()
}

/// Inflates the `SNFHFZLC` container. The result keeps the container header as zero padding so
/// offsets stored in the save stay valid.
pub fn decompress(bytes: &[u8]) -> Result<Vec<u8>> {
    if bytes.get(..8) != Some(b"SNFHFZLC") {
        return Err(Error::format("not a Witcher 3 save (missing SNFHFZLC header)"));
    }
    let mut r = Reader::at(bytes, 8);
    let chunk_count = r.u32()? as usize;
    let header_size = r.u32()? as usize;
    if chunk_count == 0 || chunk_count > 4096 {
        return Err(Error::format(format!("implausible save chunk count {chunk_count}")));
    }
    let mut chunks = Vec::with_capacity(chunk_count);
    for _ in 0..chunk_count {
        let compressed = r.u32()? as usize;
        let decompressed = r.u32()? as usize;
        let _end = r.u32()?;
        chunks.push((compressed, decompressed));
    }
    let total: usize = chunks.iter().map(|c| c.1).sum();
    let mut out = vec![0u8; header_size + total];
    let mut src = header_size;
    let mut dst = header_size;
    for (compressed, decompressed) in chunks {
        let input = bytes.get(src..src + compressed).ok_or(Error::Truncated)?;
        let written = lz4_flex::block::decompress_into(input, &mut out[dst..dst + decompressed])
            .map_err(|_| Error::Truncated)?;
        if written != decompressed {
            return Err(Error::Truncated);
        }
        src += compressed;
        dst += decompressed;
    }
    Ok(out)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// `BS`: start of a named block.
    Block,
    /// `VL` / `OP`: a named, typed value.
    Value,
    Other,
}

struct Token<'a> {
    kind: Kind,
    offset: usize,
    /// For a block: the size of its whole subtree.
    size: usize,
    name: &'a str,
    ty: &'a str,
    value: &'a [u8],
}

struct RawSave<'a> {
    raw: &'a [u8],
    header: (u32, u32),
    names: Vec<String>,
    /// (offset, size) sorted by offset.
    vars: Vec<(usize, usize)>,
}

impl<'a> RawSave<'a> {
    fn parse(raw: &'a [u8]) -> Result<Self> {
        let header_at = raw
            .windows(4)
            .take(1 << 16)
            .position(|w| w == b"SAV3")
            .ok_or_else(|| Error::format("missing SAV3 header"))?;
        let header = (u32_at(raw, header_at + 4)?, u32_at(raw, header_at + 8)?);

        if raw.len() < 16 || &raw[raw.len() - 2..] != b"SE" {
            return Err(Error::Truncated);
        }
        let var_table = u32_at(raw, raw.len() - 6)? as usize;
        let names_at = u32_at(raw, var_table.checked_sub(10).ok_or(Error::Truncated)?)? as usize;
        let names = read_names(raw, names_at)?;

        let mut r = Reader::at(raw, var_table);
        let count = r.u32()? as usize;
        if count > raw.len() / 8 {
            return Err(Error::format("implausible variable count"));
        }
        let mut vars = Vec::with_capacity(count);
        for _ in 0..count {
            let offset = r.u32()? as usize;
            let size = r.u32()? as usize;
            vars.push((offset, size));
        }
        vars.sort_unstable();
        Ok(Self { raw, header, names, vars })
    }

    fn name(&self, index: u16) -> &str {
        (index as usize)
            .checked_sub(1)
            .and_then(|i| self.names.get(i))
            .map_or("", String::as_str)
    }

    fn token(&self, i: usize) -> Token<'_> {
        let (offset, size) = self.vars[i];
        let end = self.vars.get(i + 1).map_or(offset + size, |next| next.0);
        let tok = self.raw.get(offset..end.min(self.raw.len())).unwrap_or(&[]);
        let other = Token { kind: Kind::Other, offset, size, name: "", ty: "", value: &[] };
        if tok.starts_with(b"BS") && tok.len() >= 4 {
            Token { kind: Kind::Block, name: self.name(u16_at(tok, 2).unwrap_or(0)), ..other }
        } else if (tok.starts_with(b"VL") || tok.starts_with(b"OP")) && tok.len() >= 6 {
            Token {
                kind: Kind::Value,
                name: self.name(u16_at(tok, 2).unwrap_or(0)),
                ty: self.name(u16_at(tok, 4).unwrap_or(0)),
                value: &tok[6..],
                ..other
            }
        } else {
            other
        }
    }

    fn snapshot(&self) -> Result<SaveSnapshot> {
        let mut snap = SaveSnapshot {
            save_version: self.header.0,
            game_version: self.header.1,
            playthrough_id: None,
            history: Vec::new(),
            journal: HashMap::new(),
        };
        let mut save_info_end = None;
        let mut journal_end = None;
        // Path resources of the journal entry being read; `None` marks a level without its own
        // resource (a phase or objective inside the parent's file).
        let mut entry_path: Vec<Option<String>> = Vec::new();

        for i in 0..self.vars.len() {
            let t = self.token(i);
            match (t.kind, t.name) {
                (Kind::Block, "saveInfo") if save_info_end.is_none() => {
                    save_info_end = Some(t.offset + t.size);
                    continue;
                }
                (Kind::Block, "JActiveEntries") if journal_end.is_none() => {
                    journal_end = Some(t.offset + t.size);
                    continue;
                }
                _ => {}
            }

            if save_info_end.is_some_and(|end| t.offset < end) && t.kind == Kind::Value {
                match (t.name, t.ty) {
                    ("playthroughId", ty @ ("String" | "StringAnsi")) => {
                        let id = if ty == "String" { red_string(t.value)? } else { ansi_string(t.value)? };
                        snap.playthrough_id = Some(id).filter(|id| !id.is_empty());
                    }
                    ("time", "Uint64") => snap.history.push(HistoryRecord {
                        time: SaveTime(Reader::at(t.value, 0).u64()?),
                        kind: 0,
                        version: 0,
                    }),
                    ("type", "Uint8") => {
                        if let Some(last) = snap.history.last_mut() {
                            last.kind = Reader::at(t.value, 0).u8()?;
                        }
                    }
                    ("v", "Uint16") => {
                        if let Some(last) = snap.history.last_mut() {
                            last.version = Reader::at(t.value, 0).u16()?;
                        }
                    }
                    _ => {}
                }
            }

            if journal_end.is_some_and(|end| t.offset < end) {
                match (t.kind, t.name) {
                    (Kind::Block, "SJournalEntryStatus") => entry_path.clear(),
                    (Kind::Value, "guid") => entry_path.push(None),
                    (Kind::Value, "resource") if t.ty == "String" => {
                        let resource = red_string(t.value)?;
                        if let Some(level) = entry_path.last_mut() {
                            *level = Some(resource).filter(|r| !r.is_empty());
                        }
                    }
                    (Kind::Value, "status") if t.ty == "EJournalStatus" => {
                        let status = JournalStatus::from_name(self.name(u16_at(t.value, 0)?));
                        if let (Some(status), Some(Some(resource))) = (status, entry_path.last()) {
                            snap.journal.insert(resource.to_ascii_lowercase(), status);
                        }
                    }
                    _ => {}
                }
            }
        }

        if save_info_end.is_none() {
            return Err(Error::format("save has no saveInfo block"));
        }
        if journal_end.is_none() {
            return Err(Error::format("save has no journal (JActiveEntries) block"));
        }
        Ok(snap)
    }
}

/// A `StringAnsi` value (4.0x saves): a length byte, then that many bytes including a NUL.
fn ansi_string(value: &[u8]) -> Result<String> {
    let mut r = Reader::at(value, 0);
    let len = r.u8()? as usize;
    let s: String = r.bytes(len)?.iter().map(|&b| b as char).collect();
    Ok(s.trim_end_matches('\0').to_string())
}

/// Reads the `NM` section: `"NM"`, `"MANU"`, count, an unknown `u32`, then length-prefixed names.
fn read_names(raw: &[u8], at: usize) -> Result<Vec<String>> {
    let mut r = Reader::at(raw, at);
    if r.bytes(2)? != b"NM" || r.bytes(4)? != b"MANU" {
        return Err(Error::format("missing save name table"));
    }
    let count = r.u32()? as usize;
    let _ = r.u32()?;
    if count > raw.len() {
        return Err(Error::format("implausible name count"));
    }
    let mut names = Vec::with_capacity(count);
    for _ in 0..count {
        let len = r.u8()? as usize;
        names.push(r.bytes(len)?.iter().map(|&b| b as char).collect());
    }
    Ok(names)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_save_time() {
        // First Remastered history record of the user's 2026 playthrough.
        let t = SaveTime(0x7ea4_8400_03e3_550d).to_datetime().unwrap();
        assert_eq!(t.to_string(), "2026-10-02 15:35:21.269");
    }

    #[test]
    fn save_time_orders_chronologically() {
        assert!(SaveTime(0x7ea4_8400_03e3_550d) < SaveTime(0x7ea4_9400_0027_df70));
    }

    #[test]
    fn rejects_non_save() {
        assert!(matches!(parse_save(b"not a save at all"), Err(Error::Format(_))));
    }

    #[test]
    fn truncated_save_reports_truncation() {
        let mut bytes = b"SNFHFZLC".to_vec();
        bytes.extend_from_slice(&1u32.to_le_bytes()); // chunk count
        bytes.extend_from_slice(&32u32.to_le_bytes()); // header size
        bytes.extend_from_slice(&100u32.to_le_bytes()); // compressed size
        bytes.extend_from_slice(&200u32.to_le_bytes()); // decompressed size
        bytes.extend_from_slice(&0u32.to_le_bytes());
        bytes.resize(40, 0);
        assert!(matches!(parse_save(&bytes), Err(Error::Truncated)));
    }
}
