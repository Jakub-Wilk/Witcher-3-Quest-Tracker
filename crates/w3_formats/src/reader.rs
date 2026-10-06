//! Little-endian byte cursor and the REDengine string / GUID encodings shared by saves and CR2W
//! resources.

use crate::error::{Error, Result};

pub(crate) struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    pub fn at(buf: &'a [u8], pos: usize) -> Self {
        Self { buf, pos }
    }

    pub fn pos(&self) -> usize {
        self.pos
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self.pos.checked_add(n).ok_or(Error::Truncated)?;
        let out = self.buf.get(self.pos..end).ok_or(Error::Truncated)?;
        self.pos = end;
        Ok(out)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_le_bytes(self.bytes(2)?.try_into().unwrap()))
    }

    pub fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_le_bytes(self.bytes(4)?.try_into().unwrap()))
    }

    pub fn u64(&mut self) -> Result<u64> {
        Ok(u64::from_le_bytes(self.bytes(8)?.try_into().unwrap()))
    }
}

/// Reads a `u16` at `pos` without a cursor.
pub(crate) fn u16_at(buf: &[u8], pos: usize) -> Result<u16> {
    Reader::at(buf, pos).u16()
}

/// Reads a `u32` at `pos` without a cursor.
pub(crate) fn u32_at(buf: &[u8], pos: usize) -> Result<u32> {
    Reader::at(buf, pos).u32()
}

/// Decodes a REDengine serialized string.
///
/// The first byte holds the low 6 length bits, `0x40` if more length bytes follow (7 bits each,
/// `0x80` continuation) and `0x80` for a single-byte (ANSI) string; without it the string is
/// UTF-16. Trailing NULs, which pre-Remastered saves include, are stripped.
pub(crate) fn red_string(buf: &[u8]) -> Result<String> {
    let mut r = Reader::at(buf, 0);
    let head = r.u8()?;
    let mut len = (head & 0x3f) as usize;
    if head & 0x40 != 0 {
        let mut shift = 6;
        loop {
            let b = r.u8()?;
            len |= ((b & 0x7f) as usize) << shift;
            shift += 7;
            if b & 0x80 == 0 || shift > 27 {
                break;
            }
        }
    }
    let s = if head & 0x80 != 0 {
        r.bytes(len)?.iter().map(|&b| b as char).collect::<String>()
    } else {
        let units: Vec<u16> =
            r.bytes(len * 2)?.chunks_exact(2).map(|c| u16::from_le_bytes([c[0], c[1]])).collect();
        String::from_utf16_lossy(&units)
    };
    Ok(s.trim_end_matches('\0').to_string())
}

/// Formats a 16-byte REDengine `CGUID` like a Windows GUID (first three groups little-endian).
pub(crate) fn guid_string(b: &[u8]) -> Result<String> {
    let b: &[u8; 16] = b.get(..16).ok_or(Error::Truncated)?.try_into().unwrap();
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
        u16::from_le_bytes([b[6], b[7]]),
        b[8],
        b[9],
        b[10],
        b[11],
        b[12],
        b[13],
        b[14],
        b[15]
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ansi_string() {
        assert_eq!(red_string(b"\x84abcd").unwrap(), "abcd");
        assert_eq!(red_string(b"\x80").unwrap(), "");
    }

    #[test]
    fn ansi_string_strips_trailing_nul() {
        assert_eq!(red_string(b"\x83ab\0").unwrap(), "ab");
    }

    #[test]
    fn long_ansi_string() {
        let text = "x".repeat(70);
        let mut buf = vec![0x80 | 0x40 | (70 & 0x3f), 70 >> 6];
        buf.extend_from_slice(text.as_bytes());
        assert_eq!(red_string(&buf).unwrap(), text);
    }

    #[test]
    fn utf16_string() {
        assert_eq!(red_string(&[0x02, b'h', 0, b'i', 0]).unwrap(), "hi");
    }

    #[test]
    fn truncated_string() {
        assert!(matches!(red_string(b"\x85ab"), Err(Error::Truncated)));
    }

    #[test]
    fn guid_formatting() {
        let bytes = [
            0x41, 0x44, 0xaa, 0xc1, 0xe8, 0x64, 0xff, 0x48, 0xb9, 0x3b, 0x2b, 0x97, 0x91, 0xc1,
            0x0c, 0x25,
        ];
        assert_eq!(guid_string(&bytes).unwrap(), "c1aa4441-64e8-48ff-b93b-2b9791c10c25");
    }
}
