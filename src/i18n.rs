//! Interface text in the game's 18 languages.
//!
//! Hand-written messages live in `assets/i18n/<code>.toml`, one flat `key = "text"` table per
//! language, keyed by the game's language code (the `.w3strings` file name). Everything the game
//! itself names (items, categories, crafting labels, quest titles) comes from the game's own
//! strings instead. Placeholders are `{name}`; counts are written as "Label: {n}" to sidestep
//! plural rules.

use std::collections::HashMap;
use std::fmt::Display;
use std::sync::OnceLock;

/// Which fonts a language needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Script {
    Latin,
    LatinExt,
    Cyrillic,
    Cjk,
    Arabic,
}

impl Script {
    /// Value of the root `data-script` attribute the stylesheet keys fonts on.
    pub fn attr(self) -> &'static str {
        match self {
            Script::Latin => "latin",
            Script::LatinExt => "latin-ext",
            Script::Cyrillic => "cyrillic",
            Script::Cjk => "cjk",
            Script::Arabic => "arabic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Language {
    /// The game's code, e.g. `en`, `esmx`, `cz`.
    pub code: &'static str,
    /// BCP 47 tag, e.g. `es-MX`.
    pub tag: &'static str,
    /// The language's own name.
    pub native: &'static str,
    pub script: Script,
}

impl Language {
    pub fn rtl(self) -> bool {
        self.script == Script::Arabic
    }
}

macro_rules! languages {
    ($(($code:literal, $tag:literal, $native:literal, $script:ident)),* $(,)?) => {
        pub const LANGUAGES: &[Language] = &[
            $(Language { code: $code, tag: $tag, native: $native, script: Script::$script }),*
        ];
        const SOURCES: &[(&str, &str)] = &[
            $(($code, include_str!(concat!("../assets/i18n/", $code, ".toml")))),*
        ];
    };
}

languages![
    ("en", "en", "English", Latin),
    ("pl", "pl", "Polski", LatinExt),
    ("de", "de", "Deutsch", Latin),
    ("fr", "fr", "Français", Latin),
    ("it", "it", "Italiano", Latin),
    ("es", "es-ES", "Español (España)", Latin),
    ("esmx", "es-MX", "Español (Latinoamérica)", Latin),
    ("br", "pt-BR", "Português (Brasil)", Latin),
    ("cz", "cs", "Čeština", LatinExt),
    ("hu", "hu", "Magyar", LatinExt),
    ("tr", "tr", "Türkçe", LatinExt),
    ("ru", "ru", "Русский", Cyrillic),
    ("ua", "uk", "Українська", Cyrillic),
    ("jp", "ja", "日本語", Cjk),
    ("kr", "ko", "한국어", Cjk),
    ("cn", "zh-Hans", "简体中文", Cjk),
    ("zh", "zh-Hant", "繁體中文", Cjk),
    ("ar", "ar", "العربية", Arabic),
];

/// The language with the game code `code`, or English.
pub fn language(code: &str) -> Language {
    LANGUAGES.iter().copied().find(|l| l.code == code).unwrap_or(LANGUAGES[0])
}

/// The best language for a BCP 47 locale such as `pl-PL` or `zh-TW`.
pub fn language_for_locale(locale: &str) -> &'static str {
    let p = locale.to_ascii_lowercase().replace('_', "-");
    let base = p.split('-').next().unwrap_or_default();
    match base {
        "es" if p == "es" || p == "es-es" => "es",
        "es" => "esmx",
        "pt" => "br",
        "zh" if ["zh-tw", "zh-hk", "zh-mo"].contains(&p.as_str()) || p.starts_with("zh-hant") => "zh",
        "zh" => "cn",
        "cs" => "cz",
        "uk" => "ua",
        "ja" => "jp",
        "ko" => "kr",
        other => LANGUAGES.iter().find(|l| l.code == other).map_or("en", |l| l.code),
    }
}

/// The language matching the operating system's locale.
pub fn detect_language() -> &'static str {
    sys_locale::get_locale().map_or("en", |l| language_for_locale(&l))
}

type Catalog = HashMap<String, String>;

fn catalogs() -> &'static HashMap<&'static str, Catalog> {
    static CATALOGS: OnceLock<HashMap<&'static str, Catalog>> = OnceLock::new();
    CATALOGS.get_or_init(|| {
        SOURCES
            .iter()
            .map(|&(code, source)| {
                let catalog = toml::from_str(source).unwrap_or_else(|e| {
                    tracing::error!("Invalid {code}.toml: {e}");
                    Catalog::new()
                });
                (code, catalog)
            })
            .collect()
    })
}

/// Translator for one language: falls back to English, then to the key itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tr {
    pub lang: Language,
}

impl Tr {
    pub fn new(code: &str) -> Self {
        Self { lang: language(code) }
    }

    pub fn get(self, key: &'static str) -> &'static str {
        let all = catalogs();
        all.get(self.lang.code)
            .and_then(|c| c.get(key))
            .or_else(|| all.get("en").and_then(|c| c.get(key)))
            .map_or(key, String::as_str)
    }

    /// The message with `{name}` placeholders filled in.
    pub fn fmt(self, key: &'static str, args: &[(&str, &dyn Display)]) -> String {
        fill(self.get(key), args)
    }
}

fn fill(template: &str, args: &[(&str, &dyn Display)]) -> String {
    let mut out = template.to_string();
    for (name, value) in args {
        out = out.replace(&format!("{{{name}}}"), &value.to_string());
    }
    out
}

/// `t!(tr, "key")` or `t!(tr, "key", n = count, name = title)`.
#[macro_export]
macro_rules! t {
    ($tr:expr, $key:literal) => {
        $tr.get($key)
    };
    ($tr:expr, $key:literal, $($name:ident = $value:expr),+ $(,)?) => {
        $tr.fmt($key, &[$((stringify!($name), &$value as &dyn std::fmt::Display)),+])
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(s: &str) -> Vec<&str> {
        let mut found: Vec<&str> = s.split('{').skip(1).filter_map(|p| p.split_once('}').map(|(n, _)| n)).collect();
        found.sort_unstable();
        found
    }

    #[test]
    fn every_language_has_every_message_with_the_same_placeholders() {
        let all = catalogs();
        let en = &all["en"];
        assert!(en.len() > 100);
        for lang in LANGUAGES {
            let catalog = &all[lang.code];
            for (key, text) in en {
                let translated = catalog.get(key).unwrap_or_else(|| panic!("{}: missing {key}", lang.code));
                assert_eq!(placeholders(translated), placeholders(text), "{}: {key}", lang.code);
            }
            for key in catalog.keys() {
                assert!(en.contains_key(key), "{}: unknown key {key}", lang.code);
            }
        }
    }

    #[test]
    fn fills_placeholders() {
        assert_eq!(fill("Recipes: {n} · Crafts: {m}", &[("n", &3), ("m", &"x")]), "Recipes: 3 · Crafts: x");
        let tr = Tr::new("en");
        assert_eq!(t!(tr, "missing.key.for.test"), "missing.key.for.test");
    }

    #[test]
    fn maps_locales() {
        assert_eq!(language_for_locale("pl-PL"), "pl");
        assert_eq!(language_for_locale("es-AR"), "esmx");
        assert_eq!(language_for_locale("es-ES"), "es");
        assert_eq!(language_for_locale("zh-TW"), "zh");
        assert_eq!(language_for_locale("zh-CN"), "cn");
        assert_eq!(language_for_locale("cs_CZ"), "cz");
        assert_eq!(language_for_locale("pt-PT"), "br");
        assert_eq!(language_for_locale("xx"), "en");
        assert_eq!(language("nope").code, "en");
    }
}
