//! Localized text from the game's `<lang>.w3strings` files.
//!
//! Layout: `"RTSW"`, version, `key1: u16`; the file's last two bytes are `key2`. `key1 << 16 |
//! key2` selects a per-language magic (0 for languages added later, whose text is not
//! encrypted). From byte 10:
//! - block 1: count, then (id ^ magic, offset, length) per string, `u32` each;
//! - block 2: count, then 8-byte key-hash entries (unused here);
//! - block 3: the size of the string data that follows.
//!
//! Counts are "bit6" variable-length integers. Each string is XOR-encrypted unit by unit with the
//! low bits of `(length + 1) * k`, where `k` starts at `(magic >> 8) & 0xffff` and rotates left by
//! one bit per unit. Up to version 162 a unit is a UTF-16 code unit (offsets and lengths count
//! units); Remastered (version 164) stores UTF-8 bytes and counts bytes.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::bundle::content_dir;
use crate::error::{Error, Result};
use crate::reader::Reader;

/// `(key1 << 16 | key2, magic)` for the encrypted languages.
const MAGICS: &[(u32, u32)] = &[
    (0x8349_6237, 0x7394_6816), // pl
    (0x4397_5139, 0x7932_1793), // en
    (0x7588_6138, 0x4279_1159), // de
    (0x4593_1894, 0x1237_5973), // it
    (0x2386_3176, 0x7592_1975), // fr
    (0x2498_7354, 0x2179_3217), // cz
    (0x1879_6651, 0x4238_7566), // es
    (0x1863_2176, 0x1687_5467), // zh
    (0x6348_1486, 0x4238_6347), // ru
    (0x4237_8932, 0x6782_3218), // hu
    (0x5483_4893, 0x5982_5646), // jp
];

/// The first version that stores UTF-8 bytes instead of UTF-16 units.
const UTF8_VERSION: u32 = 164;

/// Finds every `.w3strings` file under `<game>\content\*\`, grouped by language code (the file
/// stem, e.g. `en`, `pl`, `esmx`).
pub fn language_files(game_dir: &Path) -> Result<BTreeMap<String, Vec<PathBuf>>> {
    let content = content_dir(game_dir);
    let mut out: BTreeMap<String, Vec<PathBuf>> = BTreeMap::new();
    let dirs = std::fs::read_dir(&content).map_err(|e| Error::io(&content, e))?;
    for dir in dirs.flatten() {
        let Ok(files) = std::fs::read_dir(dir.path()) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if !path.extension().is_some_and(|e| e.eq_ignore_ascii_case("w3strings")) {
                continue;
            }
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                out.entry(stem.to_ascii_lowercase()).or_default().push(path);
            }
        }
    }
    for files in out.values_mut() {
        files.sort();
    }
    Ok(out)
}

/// Loads the strings with the `wanted` ids from one language's files.
pub fn load(files: &[PathBuf], wanted: &HashSet<u32>) -> Result<HashMap<u32, String>> {
    let mut out = HashMap::new();
    for path in files {
        let bytes = std::fs::read(path).map_err(|e| Error::io(path, e))?;
        parse(&bytes, |id| wanted.contains(&id), &mut out)
            .map_err(|e| Error::format(format!("{}: {e}", path.display())))?;
    }
    Ok(out)
}

/// Decodes the strings whose id passes `keep` into `out`.
pub fn parse(bytes: &[u8], keep: impl Fn(u32) -> bool, out: &mut HashMap<u32, String>) -> Result<()> {
    if bytes.get(..4) != Some(b"RTSW") || bytes.len() < 12 {
        return Err(Error::format("not a w3strings file"));
    }
    let mut r = Reader::at(bytes, 4);
    let version = r.u32()?;
    let key1 = r.u16()? as u32;
    let key2 = u16::from_le_bytes([bytes[bytes.len() - 2], bytes[bytes.len() - 1]]) as u32;
    let key = key1 << 16 | key2;
    let magic = match MAGICS.iter().find(|(k, _)| *k == key) {
        Some(&(_, magic)) => magic,
        None if key == 0 => 0,
        None => return Err(Error::format(format!("unknown language key {key:#010x}"))),
    };

    let mut r = Reader::at(bytes, 10);
    let count = bit6(&mut r)? as usize;
    let mut entries = Vec::with_capacity(count.min(1 << 20));
    for _ in 0..count {
        let id = r.u32()? ^ magic;
        let offset = r.u32()? as usize;
        let len = r.u32()? as usize;
        if keep(id) {
            entries.push((id, offset, len));
        }
    }
    let hashes = bit6(&mut r)? as usize;
    r.bytes(hashes.checked_mul(8).ok_or(Error::Truncated)?)?;
    let _data_size = bit6(&mut r)?;
    let data = &bytes[r.pos()..];

    let utf8 = version >= UTF8_VERSION;
    let unit = if utf8 { 1 } else { 2 };
    let start_key = ((magic >> 8) & 0xffff) as u16;
    for (id, offset, len) in entries {
        let raw = data.get(offset * unit..(offset + len) * unit).ok_or(Error::Truncated)?;
        let mut k = start_key;
        let mut next_key = || {
            let key = ((len as u32 + 1).wrapping_mul(k as u32) & 0xffff) as u16;
            k = k.rotate_left(1);
            key
        };
        let text = if utf8 {
            let plain: Vec<u8> = raw.iter().map(|&b| b ^ next_key() as u8).collect();
            String::from_utf8_lossy(&plain).into_owned()
        } else {
            let units: Vec<u16> = raw
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]) ^ next_key())
                .collect();
            String::from_utf16_lossy(&units)
        };
        out.insert(id, text);
    }
    Ok(())
}

/// The "bit6" variable-length integer, decoded exactly like the reference w3strings tool: the
/// first byte carries 6 value bits (`0x40` = more follow), later bytes 7 bits (`0x80` = more
/// follow), and a lone `0x80` means zero.
fn bit6(r: &mut Reader<'_>) -> Result<u32> {
    let mut value = 0u32;
    let mut shift = 0u32;
    let mut i = 1;
    loop {
        let b = r.u8()?;
        if b == 0x80 {
            return Ok(0);
        }
        let (mask, bits) = if b > 127 {
            (0x7f, 7)
        } else if b > 63 && i == 1 {
            (0x3f, 6)
        } else {
            (0xff, 6)
        };
        if shift < 32 {
            value |= ((b & mask) as u32) << shift;
        }
        shift += bits;
        i += 1;
        if b < 64 || (i >= 3 && b < 128) {
            return Ok(value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a w3strings file with one string, encrypting it the way the game does.
    fn build(version: u32, key: u32, magic: u32, id: u32, text: &str) -> Vec<u8> {
        let units: Vec<u16> = if version >= UTF8_VERSION {
            text.bytes().map(u16::from).collect()
        } else {
            text.encode_utf16().collect()
        };
        let len = units.len();
        let mut k = ((magic >> 8) & 0xffff) as u16;
        let mut data = Vec::new();
        for u in units {
            let key = ((len as u32 + 1).wrapping_mul(k as u32) & 0xffff) as u16;
            k = k.rotate_left(1);
            if version >= UTF8_VERSION {
                data.push(u as u8 ^ key as u8);
            } else {
                data.extend_from_slice(&(u ^ key).to_le_bytes());
            }
        }
        let mut out = b"RTSW".to_vec();
        out.extend_from_slice(&version.to_le_bytes());
        out.extend_from_slice(&((key >> 16) as u16).to_le_bytes());
        out.push(1); // block 1 count
        out.extend_from_slice(&(id ^ magic).to_le_bytes());
        out.extend_from_slice(&0u32.to_le_bytes());
        out.extend_from_slice(&(len as u32).to_le_bytes());
        out.push(0); // block 2 count
        out.push(data.len() as u8); // block 3 size (fits one byte in these tests)
        out.extend_from_slice(&data);
        out.extend_from_slice(&(key as u16).to_le_bytes());
        out
    }

    fn decode_one(bytes: &[u8]) -> HashMap<u32, String> {
        let mut out = HashMap::new();
        parse(bytes, |_| true, &mut out).unwrap();
        out
    }

    #[test]
    fn remastered_utf8_encrypted() {
        let file = build(164, 0x4397_5139, 0x7932_1793, 343480, "Kaer Morhen");
        assert_eq!(decode_one(&file)[&343480], "Kaer Morhen");
    }

    #[test]
    fn remastered_utf8_unencrypted_language() {
        let file = build(164, 0, 0, 7, "Кер-Морен");
        assert_eq!(decode_one(&file)[&7], "Кер-Морен");
    }

    #[test]
    fn classic_utf16() {
        let file = build(162, 0x8349_6237, 0x7394_6816, 42, "Zażółć");
        assert_eq!(decode_one(&file)[&42], "Zażółć");
    }

    #[test]
    fn unknown_key_is_an_error() {
        let file = build(164, 0x1234_5678, 0, 1, "x");
        assert!(parse(&file, |_| true, &mut HashMap::new()).is_err());
    }

    #[test]
    fn bit6_multi_byte() {
        // 96863 as stored at the start of the Remastered en.w3strings.
        let bytes = [0x5f, 0xe9, 0x0b];
        assert_eq!(bit6(&mut Reader::at(&bytes, 0)).unwrap(), 96863);
        assert_eq!(bit6(&mut Reader::at(&[0x05], 0)).unwrap(), 5);
    }
}
