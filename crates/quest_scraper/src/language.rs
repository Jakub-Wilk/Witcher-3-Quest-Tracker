use std::fmt;
use std::str::FromStr;

/// Languages quest titles can be synced in.
///
/// Titles come from the English wiki's interlanguage links. Only languages whose links cover
/// at least 80% of the quest pages are offered. Measured 2026-10-05 over 418 quest pages:
/// Polish 98%, Russian 83%; the next best were Arabic 46% and Turkish 33%.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Language {
    #[default]
    English,
    Polish,
    Russian,
}

impl Language {
    pub const ALL: [Language; 3] = [Language::English, Language::Polish, Language::Russian];

    /// MediaWiki language code.
    pub fn code(self) -> &'static str {
        match self {
            Language::English => "en",
            Language::Polish => "pl",
            Language::Russian => "ru",
        }
    }

    /// Language name in that language, for pickers.
    pub fn native_label(self) -> &'static str {
        match self {
            Language::English => "English",
            Language::Polish => "Polski",
            Language::Russian => "Русский",
        }
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.code())
    }
}

impl FromStr for Language {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Language::ALL
            .into_iter()
            .find(|l| l.code() == s)
            .ok_or_else(|| format!("unsupported language '{s}'"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_code() {
        for lang in Language::ALL {
            assert_eq!(lang.to_string().parse::<Language>(), Ok(lang));
        }
        assert!("de".parse::<Language>().is_err());
    }
}
