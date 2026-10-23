//! Geralt's inventory inside a decompressed save. Reverse-engineered layout:
//!
//! An inventory is serialized inside its entity's binary data as `u16` name ref
//! (`CInventoryComponent`), `u16` next item id, `u16` item count, then the item records. Each
//! record starts with:
//! - `+0` `u16` item name ref (the internal name, e.g. `Swallow 2`)
//! - `+2` `u16` unique id
//! - `+4` flags (3 bytes), `+7` five zero bytes, `+12` `u8` (non-zero for gear with extra data)
//! - `+13` `u16` dye name ref, `+15` `u16` dye colour ref
//! - `+17` `u16` quantity
//! - `+19` `f32` durability (-1 when not applicable)
//!
//! A variable-length tail follows (oil charges, enchantments, upgrades, ...). It is not decoded:
//! the next record is found by scanning for the next valid record start, and an inventory only
//! counts as read when exactly `item count` records were found.
//!
//! Geralt's inventory is the largest one whose entity carries the `PLAYER` and `GERALT` tags.
//! The stash (the horse manager's inventory) is deliberately ignored. Only Remastered saves are
//! known to use this layout.

use std::collections::HashMap;

use crate::reader::u16_at;

/// How far past a record start the next one may begin.
const MAX_RECORD: usize = 4000;
/// How far before the inventory header the entity's tags are looked for.
const TAG_WINDOW: usize = 64;
/// The shortest record.
const MIN_RECORD: usize = 30;

/// Internal item name → total quantity, or `None` when no complete player inventory was found.
/// `names` is the save's 1-based name table.
pub fn read_player_inventory(raw: &[u8], names: &[String]) -> Option<HashMap<String, u32>> {
    let name_ref = |name: &str| names.iter().position(|n| n == name).map(|i| i as u16 + 1);
    let inventory = name_ref("CInventoryComponent")?;
    let player = name_ref("PLAYER")?;
    let geralt = name_ref("GERALT")?;

    let u16_at = |p: usize| u16_at(raw, p).unwrap_or(0);
    let valid_name = |n: u16| n > 0 && n as usize <= names.len();
    let is_record = |p: usize| {
        p + 23 <= raw.len()
            && valid_name(u16_at(p))
            && raw[p + 7..=p + 11].iter().all(|&b| b == 0)
            && valid_name(u16_at(p + 13))
            && valid_name(u16_at(p + 15))
            && f32::from_le_bytes([raw[p + 19], raw[p + 20], raw[p + 21], raw[p + 22]]) != 0.0
    };
    let has_tag = |header: usize, tag: u16| {
        (header.saturating_sub(TAG_WINDOW)..header).any(|k| u16_at(k) == tag)
    };

    let mut best: Option<(usize, HashMap<String, u32>)> = None;
    for p in 0..raw.len().saturating_sub(6) {
        if u16_at(p) != inventory {
            continue;
        }
        let count = u16_at(p + 4) as usize;
        if count == 0 || !is_record(p + 6) || best.as_ref().is_some_and(|(n, _)| count <= *n) {
            continue;
        }
        if !has_tag(p, player) || !has_tag(p, geralt) {
            continue;
        }

        let mut items: HashMap<String, u32> = HashMap::new();
        let mut r = p + 6;
        let mut read = 0;
        loop {
            let name = &names[u16_at(r) as usize - 1];
            *items.entry(name.clone()).or_default() += u16_at(r + 17) as u32;
            read += 1;
            if read == count {
                break;
            }
            match (r + MIN_RECORD..r + MAX_RECORD).find(|&next| is_record(next)) {
                Some(next) => r = next,
                None => break,
            }
        }
        if read == count {
            best = Some((count, items));
        }
    }
    best.map(|(_, items)| items)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: &[&str] =
        &["CInventoryComponent", "PLAYER", "GERALT", "Celandine", "Swallow 2", "Dye Default", "dye_default", "Viper sword"];

    fn r(name: &str) -> u16 {
        NAMES.iter().position(|n| *n == name).unwrap() as u16 + 1
    }

    fn record(out: &mut Vec<u8>, name: &str, qty: u16, tail: &[u8]) {
        out.extend(r(name).to_le_bytes());
        out.extend(0x1234u16.to_le_bytes());
        out.extend([0x0f, 0, 1, 0, 0, 0, 0, 0, 0]);
        out.extend(r("Dye Default").to_le_bytes());
        out.extend(r("dye_default").to_le_bytes());
        out.extend(qty.to_le_bytes());
        out.extend((-1f32).to_le_bytes());
        out.extend(tail);
    }

    fn inventory(out: &mut Vec<u8>, items: &[(&str, u16, &[u8])], tagged: bool, count: u16) {
        if tagged {
            out.extend(r("PLAYER").to_le_bytes());
            out.extend(r("GERALT").to_le_bytes());
        }
        out.extend([0; 8]);
        out.extend(r("CInventoryComponent").to_le_bytes());
        out.extend(99u16.to_le_bytes());
        out.extend(count.to_le_bytes());
        for (name, qty, tail) in items {
            record(out, name, *qty, tail);
        }
        out.extend([0xee; 80]);
    }

    fn names() -> Vec<String> {
        NAMES.iter().map(|s| s.to_string()).collect()
    }

    const SHORT: &[u8] = &[0, 1, 0, 0, 0, 0, 0];

    #[test]
    fn reads_the_tagged_inventory_and_sums_stacks() {
        let mut raw = vec![0; 32];
        // A larger, untagged inventory (the stash) must be ignored.
        let stash: Vec<(&str, u16, &[u8])> = (0..5).map(|_| ("Celandine", 7, SHORT)).collect();
        inventory(&mut raw, &stash, false, 5);
        let gear_tail: Vec<u8> = [0u8, 1, 0, 0, 0, 0, 0].into_iter().chain([0x55; 40]).collect();
        inventory(
            &mut raw,
            &[("Swallow 2", 3, SHORT), ("Celandine", 4, SHORT), ("Viper sword", 1, &gear_tail), ("Celandine", 2, SHORT)],
            true,
            4,
        );
        let items = read_player_inventory(&raw, &names()).unwrap();
        assert_eq!(items.len(), 3);
        assert_eq!(items["Celandine"], 6);
        assert_eq!(items["Swallow 2"], 3);
        assert_eq!(items["Viper sword"], 1);
    }

    #[test]
    fn incomplete_inventory_is_rejected() {
        let mut raw = vec![0; 32];
        inventory(&mut raw, &[("Swallow 2", 3, SHORT)], true, 2);
        assert_eq!(read_player_inventory(&raw, &names()), None);
    }

    #[test]
    fn save_without_player_has_no_inventory() {
        assert_eq!(read_player_inventory(&[0; 64], &["CInventoryComponent".to_string()]), None);
    }
}
