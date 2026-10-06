//! App settings, persisted to `settings.toml` next to the DB: folder locations, UI language,
//! the last selected playthrough, quest list filters and save tracking.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::view_options::ViewOptions;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub paths: Paths,
    pub ui: Ui,
    pub view: ViewOptions,
    pub tracking: Tracking,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Paths {
    /// The game's save folder (`Documents\The Witcher 3\gamesaves`). Only ever read.
    pub save_dir: Option<PathBuf>,
    /// The game install folder (the one containing `content`). Only ever read.
    pub game_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Ui {
    /// Game language code of quest titles and descriptions (`en`, `pl`, ...).
    pub language: String,
    pub last_playthrough_id: Option<i64>,
}

impl Default for Ui {
    fn default() -> Self {
        Self { language: "en".into(), last_playthrough_id: None }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tracking {
    /// Watch the save folder while the app runs.
    pub watch_saves: bool,
}

impl Default for Tracking {
    fn default() -> Self {
        Self { watch_saves: true }
    }
}

impl AppSettings {
    /// Loads settings from `path`. A missing file gives defaults; an unreadable or invalid one
    /// is logged and also gives defaults, so a bad settings file never blocks startup.
    pub fn load(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!("Ignoring invalid {}: {e}", path.display());
                Self::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!("Could not read {}: {e}", path.display());
                Self::default()
            }
        }
    }

    pub fn to_toml(&self) -> String {
        toml::to_string(self).expect("settings always serialize")
    }

    /// Writes the settings to `path` via a temporary file, so a crash never leaves a
    /// half-written settings file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, self.to_toml())?;
        std::fs::rename(&tmp, path)
    }

    /// The configured save folder, if it exists.
    pub fn valid_save_dir(&self) -> Option<&Path> {
        self.paths.save_dir.as_deref().filter(|d| is_save_dir(d))
    }

    /// The configured game folder, if it looks like a game install.
    pub fn valid_game_dir(&self) -> Option<&Path> {
        self.paths.game_dir.as_deref().filter(|d| is_game_dir(d))
    }
}

pub fn is_save_dir(dir: &Path) -> bool {
    dir.is_dir()
}

/// A Witcher 3 (Remastered) install has its bundles under `content\content0\bundles`.
pub fn is_game_dir(dir: &Path) -> bool {
    dir.join("content").join("content0").join("bundles").is_dir()
}

/// The default save folder, if it exists.
pub fn detect_save_dir() -> Option<PathBuf> {
    let dir = dirs::document_dir()?.join("The Witcher 3").join("gamesaves");
    is_save_dir(&dir).then_some(dir)
}

/// Looks for the game in the Steam libraries and the usual GOG locations.
pub fn detect_game_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    for steam in [r"C:\Program Files (x86)\Steam", r"C:\Program Files\Steam"] {
        let vdf = Path::new(steam).join("steamapps").join("libraryfolders.vdf");
        let libraries = std::fs::read_to_string(&vdf).map(|t| steam_libraries(&t)).unwrap_or_default();
        for library in libraries.into_iter().chain([PathBuf::from(steam)]) {
            candidates.push(library.join("steamapps").join("common").join("The Witcher 3"));
        }
    }
    for gog in [
        r"C:\Program Files (x86)\GOG Galaxy\Games\The Witcher 3 Wild Hunt GOTY",
        r"C:\Program Files (x86)\GOG Galaxy\Games\The Witcher 3 Wild Hunt",
        r"C:\GOG Games\The Witcher 3 Wild Hunt GOTY",
        r"C:\GOG Games\The Witcher 3 Wild Hunt",
    ] {
        candidates.push(PathBuf::from(gog));
    }
    candidates.into_iter().find(|d| is_game_dir(d))
}

/// Extracts the library folders from Steam's `libraryfolders.vdf` (`"path"  "G:\\Steam"` lines).
fn steam_libraries(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut parts = line.split('"').filter(|s| !s.trim().is_empty());
            if parts.next()? != "path" {
                return None;
            }
            Some(PathBuf::from(parts.next()?.replace("\\\\", "\\")))
        })
        .collect()
}

/// Native name of a game language code, for pickers.
pub fn language_label(code: &str) -> &str {
    match code {
        "ar" => "العربية",
        "br" => "Português (Brasil)",
        "cn" => "简体中文",
        "cz" => "Čeština",
        "de" => "Deutsch",
        "en" => "English",
        "es" => "Español",
        "esmx" => "Español (México)",
        "fr" => "Français",
        "hu" => "Magyar",
        "it" => "Italiano",
        "jp" => "日本語",
        "kr" => "한국어",
        "pl" => "Polski",
        "ru" => "Русский",
        "tr" => "Türkçe",
        "ua" => "Українська",
        "zh" => "繁體中文",
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::view_options::SortKey;

    #[test]
    fn round_trips_through_toml() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        let mut settings = AppSettings::default();
        settings.paths.save_dir = Some(PathBuf::from(r"C:\Saves"));
        settings.ui.language = "pl".into();
        settings.ui.last_playthrough_id = Some(3);
        settings.view.sort = SortKey::Level;
        settings.view.search = "not saved".into();
        settings.tracking.watch_saves = false;
        settings.save(&path).unwrap();

        let loaded = AppSettings::load(&path);
        let mut expected = settings.clone();
        expected.view.search.clear();
        assert_eq!(loaded, expected);
    }

    #[test]
    fn missing_or_invalid_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        assert_eq!(AppSettings::load(&path), AppSettings::default());
        assert_eq!(AppSettings::load(&path).ui.language, "en");
        assert!(AppSettings::load(&path).tracking.watch_saves);

        std::fs::write(&path, "[view]\nsort = \"Sideways\"").unwrap();
        assert_eq!(AppSettings::load(&path), AppSettings::default());

        // Missing keys fall back individually.
        std::fs::write(&path, "[view]\nsort = \"Name\"").unwrap();
        assert_eq!(AppSettings::load(&path).view.sort, SortKey::Name);
        assert_eq!(AppSettings::load(&path).ui.language, "en");
    }

    #[test]
    fn parses_steam_library_folders() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
	}
	"1"
	{
		"path"		"G:\\Steam"
	}
}"#;
        assert_eq!(
            steam_libraries(vdf),
            vec![PathBuf::from(r"C:\Program Files (x86)\Steam"), PathBuf::from(r"G:\Steam")]
        );
    }

    #[test]
    fn validates_folders() {
        let dir = tempfile::tempdir().unwrap();
        assert!(is_save_dir(dir.path()));
        assert!(!is_game_dir(dir.path()));
        std::fs::create_dir_all(dir.path().join("content/content0/bundles")).unwrap();
        assert!(is_game_dir(dir.path()));
    }
}
