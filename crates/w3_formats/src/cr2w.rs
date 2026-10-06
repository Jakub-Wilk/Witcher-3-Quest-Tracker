//! Minimal CR2W resource reader: enough to read the exported objects' scalar properties.
//!
//! Header: `"CR2W"`, version, flags, timestamp, build, file size, buffer size, CRC, chunk count,
//! then ten (offset, count, CRC) tables starting at byte 40. Table 0 is the string pool, table 1
//! the name table (string offset, hash), table 4 the exports (class name `u16`, flags `u16`,
//! parent `u32`, data size, data offset, template, CRC).
//!
//! An export's data is a zero byte followed by properties — name `u16`, type `u16`, size `u32`
//! (counting itself) and the value — until a zero name. Properties left at their default value
//! are not stored at all.

use crate::error::{Error, Result};
use crate::reader::{Reader, guid_string, red_string, u16_at, u32_at};

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// `CName`, or an enum stored by value name.
    Name(String),
    String(String),
    Guid(String),
    /// `LocalizedString`: a string id in the `.w3strings` files.
    Localized(u32),
    U32(u32),
    I32(i32),
    U8(u8),
    Bool(bool),
    /// Anything else, undecoded.
    Raw(Vec<u8>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Property {
    pub name: String,
    pub ty: String,
    pub value: Value,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Export {
    pub class: String,
    /// 1-based index of the parent export, 0 for none.
    pub parent: u32,
    pub properties: Vec<Property>,
}

impl Export {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.properties.iter().find(|p| p.name == name).map(|p| &p.value)
    }

    pub fn string(&self, name: &str) -> Option<&str> {
        match self.get(name)? {
            Value::String(s) | Value::Name(s) | Value::Guid(s) => Some(s),
            _ => None,
        }
    }

    pub fn localized(&self, name: &str) -> Option<u32> {
        match self.get(name)? {
            Value::Localized(id) if *id != 0 => Some(*id),
            _ => None,
        }
    }

    pub fn u32(&self, name: &str) -> Option<u32> {
        match self.get(name)? {
            Value::U32(v) => Some(*v),
            _ => None,
        }
    }
}

/// Parses all exports of a CR2W resource.
pub fn parse(data: &[u8]) -> Result<Vec<Export>> {
    if data.get(..4) != Some(b"CR2W") {
        return Err(Error::format("not a CR2W resource"));
    }
    let table = |t: usize| -> Result<(usize, usize)> {
        Ok((u32_at(data, 40 + 12 * t)? as usize, u32_at(data, 44 + 12 * t)? as usize))
    };
    let (strings_at, strings_len) = table(0)?;
    let pool = data.get(strings_at..strings_at + strings_len).ok_or(Error::Truncated)?;
    let (names_at, name_count) = table(1)?;
    let mut names = Vec::with_capacity(name_count);
    for i in 0..name_count {
        let start = u32_at(data, names_at + 8 * i)? as usize;
        let rest = pool.get(start..).ok_or(Error::Truncated)?;
        let end = rest.iter().position(|&b| b == 0).unwrap_or(rest.len());
        names.push(rest[..end].iter().map(|&b| b as char).collect::<String>());
    }
    let name = |i: u16| names.get(i as usize).cloned().unwrap_or_default();

    let (exports_at, export_count) = table(4)?;
    let mut exports = Vec::with_capacity(export_count);
    for i in 0..export_count {
        let mut r = Reader::at(data, exports_at + 24 * i);
        let class = name(r.u16()?);
        let _flags = r.u16()?;
        let parent = r.u32()?;
        let size = r.u32()? as usize;
        let offset = r.u32()? as usize;
        let body = data.get(offset..offset + size).ok_or(Error::Truncated)?;
        exports.push(Export { class, parent, properties: properties(body, &name)? });
    }
    Ok(exports)
}

fn properties(body: &[u8], name: &dyn Fn(u16) -> String) -> Result<Vec<Property>> {
    let mut out = Vec::new();
    let mut pos = 1; // leading zero byte
    while pos + 2 <= body.len() {
        let name_index = u16_at(body, pos)?;
        if name_index == 0 {
            break;
        }
        let ty = name(u16_at(body, pos + 2)?);
        let size = u32_at(body, pos + 4)? as usize;
        if size < 4 {
            return Err(Error::format("corrupt CR2W property size"));
        }
        let raw = body.get(pos + 8..pos + 4 + size).ok_or(Error::Truncated)?;
        let value = decode(&ty, raw, name)?;
        out.push(Property { name: name(name_index), ty, value });
        pos += 4 + size;
    }
    Ok(out)
}

fn decode(ty: &str, raw: &[u8], name: &dyn Fn(u16) -> String) -> Result<Value> {
    let mut r = Reader::at(raw, 0);
    Ok(match ty {
        "CName" => Value::Name(name(r.u16()?)),
        "String" => Value::String(red_string(raw)?),
        "CGUID" => Value::Guid(guid_string(raw)?),
        "LocalizedString" => Value::Localized(r.u32()?),
        "Uint32" => Value::U32(r.u32()?),
        "Int32" => Value::I32(r.u32()? as i32),
        "Uint8" => Value::U8(r.u8()?),
        "Bool" => Value::Bool(r.u8()? != 0),
        // Enums are stored as the value's name.
        _ if raw.len() == 2 && ty.starts_with(['E', 'e']) => Value::Name(name(r.u16()?)),
        _ => Value::Raw(raw.to_vec()),
    })
}
