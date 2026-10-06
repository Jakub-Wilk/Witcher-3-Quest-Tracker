//! Read-only access to `POTATO70` game bundles.
//!
//! Header: `"POTATO70"`, bundle size, an unused `u32`, the TOC size, then padding up to 32
//! bytes. The TOC follows: in Remastered each entry is 304 bytes — a NUL-padded 256-byte path,
//! an MD5, `u32` data offset, an unused `u32`, `u32` size, `u32` stored size, a `u64` timestamp
//! and padding. Stored data is zlib when its size differs from the real size.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::reader::Reader;

const HEADER_SIZE: usize = 32;
const ENTRY_SIZE: usize = 304;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BundleEntry {
    /// Lowercase, backslash-separated depot path, e.g. `gameplay\journal\quests\q001beggining.journal`.
    pub path: String,
    pub offset: u64,
    pub size: u32,
    pub stored_size: u32,
}

#[derive(Debug, Clone)]
pub struct Bundle {
    pub file: PathBuf,
    pub entries: Vec<BundleEntry>,
}

impl Bundle {
    /// Reads the TOC of `file`, keeping only entries whose path passes `keep`.
    pub fn open(file: &Path, keep: &dyn Fn(&str) -> bool) -> Result<Self> {
        let mut f = File::open(file).map_err(|e| Error::io(file, e))?;
        let mut header = [0u8; HEADER_SIZE];
        f.read_exact(&mut header).map_err(|e| Error::io(file, e))?;
        if &header[..8] != b"POTATO70" {
            return Err(Error::format(format!("{} is not a POTATO70 bundle", file.display())));
        }
        let toc_size = Reader::at(&header, 16).u32()? as usize;
        if toc_size % ENTRY_SIZE != 0 {
            return Err(Error::format(format!(
                "{} has an unsupported bundle layout (TOC size {toc_size}); only the Remastered \
                 game version is supported",
                file.display()
            )));
        }
        let mut toc = vec![0u8; toc_size];
        f.read_exact(&mut toc).map_err(|e| Error::io(file, e))?;

        let mut entries = Vec::new();
        for e in toc.chunks_exact(ENTRY_SIZE) {
            let path_len = e[..256].iter().position(|&b| b == 0).unwrap_or(256);
            let path: String = e[..path_len].iter().map(|&b| b as char).collect();
            if !keep(&path) {
                continue;
            }
            let mut r = Reader::at(e, 272);
            let offset = r.u32()? as u64;
            let _ = r.u32()?;
            let size = r.u32()?;
            let stored_size = r.u32()?;
            entries.push(BundleEntry { path: path.to_ascii_lowercase(), offset, size, stored_size });
        }
        Ok(Self { file: file.to_path_buf(), entries })
    }

    /// Reads and, if needed, inflates one entry.
    pub fn read(&self, entry: &BundleEntry) -> Result<Vec<u8>> {
        let io = |e| Error::io(&self.file, e);
        let mut f = File::open(&self.file).map_err(io)?;
        f.seek(SeekFrom::Start(entry.offset)).map_err(io)?;
        let mut stored = vec![0u8; entry.stored_size as usize];
        f.read_exact(&mut stored).map_err(io)?;
        if entry.stored_size == entry.size {
            return Ok(stored);
        }
        let mut out = Vec::with_capacity(entry.size as usize);
        flate2::read::ZlibDecoder::new(&stored[..]).read_to_end(&mut out).map_err(|e| {
            Error::format(format!("cannot inflate {} in {}: {e}", entry.path, self.file.display()))
        })?;
        if out.len() != entry.size as usize {
            return Err(Error::format(format!("{}: size mismatch after inflating", entry.path)));
        }
        Ok(out)
    }
}

/// The filtered TOCs of every bundle in a game install, indexed by path.
#[derive(Debug, Clone, Default)]
pub struct BundleSet {
    bundles: Vec<Bundle>,
    index: HashMap<String, (usize, usize)>,
}

impl BundleSet {
    /// Opens every `*.bundle` under `<game>\content\*\bundles`. When a path is in several bundles
    /// the later one (by sorted directory and file name) wins, matching how patches override.
    pub fn open(game_dir: &Path, keep: &dyn Fn(&str) -> bool) -> Result<Self> {
        let mut files = bundle_files(game_dir)?;
        if files.is_empty() {
            return Err(Error::format(format!(
                "no game bundles found under {}",
                game_dir.join("content").display()
            )));
        }
        files.sort();
        let mut set = BundleSet::default();
        for file in files {
            let bundle = Bundle::open(&file, keep)?;
            let b = set.bundles.len();
            for (i, entry) in bundle.entries.iter().enumerate() {
                set.index.insert(entry.path.clone(), (b, i));
            }
            set.bundles.push(bundle);
        }
        Ok(set)
    }

    pub fn paths(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(String::as_str)
    }

    pub fn contains(&self, path: &str) -> bool {
        self.index.contains_key(path)
    }

    pub fn read(&self, path: &str) -> Result<Vec<u8>> {
        let &(b, i) = self
            .index
            .get(path)
            .ok_or_else(|| Error::format(format!("{path} is not in any bundle")))?;
        let bundle = &self.bundles[b];
        bundle.read(&bundle.entries[i])
    }
}

/// The bundle directory every supported install has; used to validate a game folder.
pub fn content_dir(game_dir: &Path) -> PathBuf {
    game_dir.join("content")
}

fn bundle_files(game_dir: &Path) -> Result<Vec<PathBuf>> {
    let content = content_dir(game_dir);
    let mut out = Vec::new();
    let dirs = std::fs::read_dir(&content).map_err(|e| Error::io(&content, e))?;
    for dir in dirs.flatten() {
        let bundles = dir.path().join("bundles");
        let Ok(files) = std::fs::read_dir(&bundles) else { continue };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("bundle")) {
                out.push(path);
            }
        }
    }
    Ok(out)
}
