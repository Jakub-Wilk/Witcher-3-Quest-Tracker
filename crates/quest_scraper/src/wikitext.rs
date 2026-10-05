//! Parsing of quest pages from raw MediaWiki wikitext.
//!
//! Quest pages on the Witcher wiki carry an `{{Infobox Quest3 ...}}` template whose parameters
//! (`type`, `starting_icon`, `previous`, `cutoff_quest`, ...) hold structured data. Reading those
//! directly is far more reliable than scraping rendered HTML.

use std::collections::HashMap;
use std::sync::LazyLock;

use quest_db::{QuestSource, QuestType, Region};
use regex::Regex;

use crate::error::{Result, ScraperError};
use crate::models::{ScrapedQuest, WikiPage};

/// Parses a fetched wiki page into a quest. Pages without a quest infobox (overview pages,
/// minigames such as "Horse racing") yield [`ScraperError::NotAQuest`].
pub fn parse_quest(page: &WikiPage) -> Result<ScrapedQuest> {
    let params = find_template(&page.wikitext, "infobox quest3")
        .ok_or_else(|| ScraperError::NotAQuest(page.title.clone()))?;
    let param = |key: &str| params.get(key).map(String::as_str).unwrap_or("");

    let (quest_type, is_unmarked, source) = parse_type(param("type"))
        .ok_or_else(|| ScraperError::ParseError {
            page: page.title.clone(),
            reason: format!("unknown quest type '{}'", param("type")),
        })?;

    let name = strip_markup(param("name"));
    let name = if name.is_empty() { page.title.clone() } else { name };

    Ok(ScrapedQuest {
        page_id: page.page_id,
        wiki_title: page.title.clone(),
        name,
        localized_name: None,
        source: source.unwrap_or(QuestSource::BaseGame),
        source_from_infobox: source.is_some(),
        quest_type,
        region: parse_region(param("starting_icon"), param("region")),
        recommended_level: parse_level(param("level")),
        is_unmarked,
        description: journal_entry(&page.wikitext),
        important_notes: important_notes(&page.wikitext),
        cutoff_titles: required_links(param("cutoff_quest")),
        previous_titles: required_links(param("previous")),
    })
}

/// Normalizes a page title the way MediaWiki does: underscores become spaces, whitespace is
/// collapsed and the first letter is uppercased.
pub fn normalize_title(title: &str) -> String {
    let collapsed = title.replace('_', " ").split_whitespace().collect::<Vec<_>>().join(" ");
    let mut chars = collapsed.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Removes a trailing disambiguator, e.g. "Ostatnie życzenie (zadanie)" -> "Ostatnie życzenie".
pub fn strip_disambiguator(title: &str) -> &str {
    let trimmed = title.trim();
    match trimmed.rfind(" (") {
        Some(idx) if trimmed.ends_with(')') && idx > 0 => trimmed[..idx].trim_end(),
        _ => trimmed,
    }
}

/// `type` holds e.g. "main", "secondary baw", "treasure hunt hos", "unmarked".
/// Returns `(type, is_unmarked, source if the expansion suffix is present)`.
fn parse_type(raw: &str) -> Option<(QuestType, bool, Option<QuestSource>)> {
    let lower = strip_markup(raw).to_lowercase();
    let (base, source) = match lower.rsplit_once(' ') {
        Some((base, "hos")) => (base, Some(QuestSource::HeartsOfStone)),
        Some((base, "baw")) => (base, Some(QuestSource::BloodAndWine)),
        _ => (lower.as_str(), None),
    };
    let (quest_type, unmarked) = match base.trim() {
        "main" => (QuestType::MainQuest, false),
        "secondary" => (QuestType::SecondaryQuest, false),
        "unmarked" => (QuestType::SecondaryQuest, true),
        "contract" => (QuestType::WitcherContract, false),
        "treasure hunt" => (QuestType::TreasureHunt, false),
        "scavenger hunt" => (QuestType::ScavengerHunt, false),
        _ => return None,
    };
    Some((quest_type, unmarked, source))
}

/// Region from the `starting_icon` parameter, falling back to the first `region` link
/// (needed for icon "multiple").
fn parse_region(starting_icon: &str, region: &str) -> Region {
    region_from_name(&strip_markup(starting_icon))
        .or_else(|| links(region).iter().find_map(|l| region_from_name(&l.target)))
        .unwrap_or(Region::Unknown)
}

fn region_from_name(name: &str) -> Option<Region> {
    let region = match name.trim().to_lowercase().as_str() {
        "white orchard" => Region::WhiteOrchard,
        "velen" | "no man's land" => Region::Velen,
        "novigrad" => Region::Novigrad,
        "oxenfurt" => Region::Oxenfurt,
        "skellige" | "ard skellig" | "an skellig" | "hindarsfjall" | "spikeroog" | "undvik"
        | "faroe" => Region::Skellige,
        "kaer morhen" => Region::KaerMorhen,
        "vizima" => Region::Vizima,
        "toussaint" | "beauclair" => Region::Toussaint,
        _ => return None,
    };
    Some(region)
}

/// First integer in the value ("9*" -> 9, "n/a*" -> None).
fn parse_level(raw: &str) -> Option<i32> {
    static NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\d+").unwrap());
    NUMBER.find(&strip_markup(raw)).and_then(|m| m.as_str().parse().ok())
}

/// A wiki link plus the `{{Small|...}}` annotation that follows it, if any.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Link {
    target: String,
    annotation: Option<String>,
}

/// Link targets in an infobox value, excluding links annotated as optional or dependent
/// (those only apply on some story paths).
fn required_links(value: &str) -> Vec<String> {
    let mut targets = Vec::new();
    for link in links(value) {
        let conditional = link
            .annotation
            .as_deref()
            .is_some_and(|a| a.contains("optional") || a.contains("dependent") || a.contains("dependant"));
        if !conditional && !targets.contains(&link.target) {
            targets.push(link.target);
        }
    }
    targets
}

/// Extracts `[[target|label]]` links (namespaced links such as files are skipped).
fn links(value: &str) -> Vec<Link> {
    let mut result: Vec<Link> = Vec::new();
    let mut rest = value;
    while let Some(start) = rest.find("[[") {
        let Some(len) = rest[start..].find("]]") else { break };
        let inner = &rest[start + 2..start + len];
        let after = &rest[start + len + 2..];

        // The annotation belongs to this link if it appears before the next link.
        let tail = &after[..after.find("[[").unwrap_or(after.len())];
        let annotation = find_template(tail, "small")
            .and_then(|params| params.get("1").cloned())
            .map(|a| a.to_lowercase());

        let target = inner.split(['|', '#']).next().unwrap_or("").trim();
        if !target.is_empty() && !is_namespaced(target) {
            result.push(Link { target: normalize_title(target), annotation });
        }
        rest = after;
    }
    result
}

/// Whether a link target points outside the article namespace (files, categories, ...).
/// Quest titles themselves often contain colons ("Contract: Dragon"), so only known
/// namespace prefixes count.
fn is_namespaced(target: &str) -> bool {
    const NAMESPACES: &[&str] =
        &["file", "image", "media", "category", "template", "user", "help", "special", "wikipedia", "w"];
    target
        .trim_start_matches(':')
        .split_once(':')
        .is_some_and(|(prefix, _)| NAMESPACES.contains(&prefix.trim().to_lowercase().as_str()))
}

/// Finds the first `{{name|...}}` template (name compared case-insensitively, `_` == ` `)
/// and returns its parameters. Named parameters are keyed by lowercased name, positional
/// ones by "1", "2", ...
fn find_template(text: &str, name: &str) -> Option<HashMap<String, String>> {
    let mut search_from = 0;
    while let Some(rel) = text[search_from..].find("{{") {
        let start = search_from + rel;
        let body_start = start + 2;
        let end = matching_close(text, body_start)?;
        let body = &text[body_start..end];
        let parts = split_top_level(body);
        let template_name = parts[0].replace('_', " ").trim().to_lowercase();
        if template_name == name {
            let mut params = HashMap::new();
            let mut position = 0;
            for part in &parts[1..] {
                match part.split_once('=') {
                    Some((key, value)) if !key.contains("[[") && !key.contains("{{") => {
                        params.insert(key.trim().to_lowercase(), value.trim().to_string());
                    }
                    _ => {
                        position += 1;
                        params.insert(position.to_string(), part.trim().to_string());
                    }
                }
            }
            return Some(params);
        }
        search_from = body_start;
    }
    None
}

/// Given the index just after an opening `{{`, returns the index of its matching `}}`.
fn matching_close(text: &str, from: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 1;
    let mut i = from;
    while i + 1 < bytes.len() {
        match &bytes[i..i + 2] {
            b"{{" => {
                depth += 1;
                i += 2;
            }
            b"}}" => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    None
}

/// Splits a template body on `|` that is not nested inside `{{...}}` or `[[...]]`.
fn split_top_level(body: &str) -> Vec<&str> {
    let bytes = body.as_bytes();
    let (mut braces, mut brackets) = (0usize, 0usize);
    let mut parts = Vec::new();
    let mut part_start = 0;
    let mut i = 0;
    while i < bytes.len() {
        let pair = bytes.get(i..i + 2);
        match pair {
            Some(b"{{") => {
                braces += 1;
                i += 2;
                continue;
            }
            Some(b"}}") => {
                braces = braces.saturating_sub(1);
                i += 2;
                continue;
            }
            Some(b"[[") => {
                brackets += 1;
                i += 2;
                continue;
            }
            Some(b"]]") => {
                brackets = brackets.saturating_sub(1);
                i += 2;
                continue;
            }
            _ => {}
        }
        if bytes[i] == b'|' && braces == 0 && brackets == 0 {
            parts.push(&body[part_start..i]);
            part_start = i + 1;
        }
        i += 1;
    }
    parts.push(&body[part_start..]);
    parts
}

/// First paragraph of the `== Journal entry ==` section, as plain text.
fn journal_entry(wikitext: &str) -> Option<String> {
    static HEADING: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?mi)^==+\s*journal entr(y|ies)\s*==+\s*$").unwrap());
    let start = HEADING.find(wikitext)?.end();
    let section = &wikitext[start..];
    let section = &section[..section.find("\n==").unwrap_or(section.len())];
    section
        .lines()
        .map(|line| strip_markup(line.trim_start_matches([':', '*', ' '])))
        .find(|line| line.len() > 15)
}

/// Text of each `{{Condition|Important}}` paragraph in the lead section (before the first
/// heading). Those are the missable-quest warnings; later ones are mostly combat tips.
fn important_notes(wikitext: &str) -> Option<String> {
    static IMPORTANT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?i)\{\{\s*condition\s*\|\s*important\s*\}\}").unwrap());
    let lead = &wikitext[..wikitext.find("\n==").unwrap_or(wikitext.len())];
    let notes: Vec<String> = IMPORTANT
        .find_iter(lead)
        .map(|m| {
            let rest = &lead[m.end()..];
            strip_markup(&rest[..rest.find('\n').unwrap_or(rest.len())])
        })
        .filter(|note| !note.is_empty())
        .map(|note| capitalize_first(&note))
        .collect();
    (!notes.is_empty()).then(|| notes.join("\n"))
}

fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Converts wikitext to plain text: links become their labels, `{{Small|x}}` becomes "(x)",
/// other templates, refs, HTML tags and bold/italic quotes are removed.
pub fn strip_markup(text: &str) -> String {
    static REFS: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)<ref[^>]*/>|<ref[^>]*>.*?</ref>").unwrap());
    static BR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)<br\s*/?>").unwrap());
    static TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

    let text = REFS.replace_all(text, "");
    let text = BR.replace_all(&text, "\n");
    let text = replace_templates(&text);
    let text = replace_links(&text);
    let text = TAGS.replace_all(&text, "");
    let text = text
        .replace("'''", "")
        .replace("''", "")
        .replace("&nbsp;", " ")
        .replace("&amp;", "&")
        .replace("&#39;", "'")
        .replace("&quot;", "\"");

    text.lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace_templates(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let Some(end) = matching_close(rest, start + 2) else {
            rest = &rest[start + 2..];
            continue;
        };
        let parts = split_top_level(&rest[start + 2..end]);
        let name = parts[0].trim().to_lowercase();
        if name == "small" && parts.len() > 1 {
            out.push_str(&format!("({})", replace_templates(parts[1].trim())));
        }
        rest = &rest[end + 2..];
    }
    out.push_str(rest);
    out
}

fn replace_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("[[") {
        out.push_str(&rest[..start]);
        let Some(len) = rest[start..].find("]]") else {
            out.push_str(&rest[start..]);
            return out;
        };
        let inner = &rest[start + 2..start + len];
        let target = inner.split('|').next().unwrap_or("");
        if !is_namespaced(target) || target.starts_with(':') {
            // [[target|label]] -> label, [[target]] -> target, [[:Category:x|label]] -> label
            out.push_str(inner.rsplit('|').next().unwrap_or(inner));
        }
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str, page_id: i64, title: &str) -> WikiPage {
        let path = format!("{}/tests/fixtures/{name}.wiki", env!("CARGO_MANIFEST_DIR"));
        WikiPage {
            page_id,
            title: title.into(),
            wikitext: std::fs::read_to_string(path).unwrap(),
            redirects: vec![],
        }
    }

    #[test]
    fn parses_the_last_wish() {
        let q = parse_quest(&fixture("the_last_wish", 35270, "The Last Wish (quest)")).unwrap();
        assert_eq!(q.name, "The Last Wish");
        assert_eq!(q.wiki_title, "The Last Wish (quest)");
        assert_eq!(q.quest_type, QuestType::SecondaryQuest);
        assert_eq!(q.source, QuestSource::BaseGame);
        assert!(!q.source_from_infobox);
        assert_eq!(q.region, Region::Skellige);
        assert_eq!(q.recommended_level, Some(15));
        assert!(!q.is_unmarked);
        assert_eq!(q.cutoff_titles, vec!["Ugly Baby"]);
        assert_eq!(q.previous_titles, vec!["The Calm Before the Storm"]);
        let notes = q.important_notes.unwrap();
        assert!(notes.starts_with("This quest will fail if not completed"), "{notes}");
        assert!(!notes.contains("Djinn does not deal"), "walkthrough tips must be excluded");
        assert!(q.description.unwrap().starts_with("Before Geralt and Yennefer parted"));
    }

    #[test]
    fn parses_expansion_type_suffix_and_drops_conditional_links() {
        let q = parse_quest(&fixture("wine_wars_vermentino", 45938, "Wine Wars: Vermentino")).unwrap();
        assert_eq!(q.name, "Wine Wars: Vermentino");
        assert_eq!(q.source, QuestSource::BloodAndWine);
        assert!(q.source_from_infobox);
        assert_eq!(q.quest_type, QuestType::SecondaryQuest);
        assert_eq!(q.region, Region::Toussaint);
        assert_eq!(q.recommended_level, Some(37));
        assert_eq!(q.previous_titles, vec!["Wine Wars: Belgaard"]);
    }

    #[test]
    fn parses_main_quest_with_disambiguated_title() {
        let q = parse_quest(&fixture("bloody_baron", 20890, "Bloody Baron (quest)")).unwrap();
        assert_eq!(q.name, "Bloody Baron");
        assert_eq!(q.quest_type, QuestType::MainQuest);
        assert_eq!(q.region, Region::Velen);
        assert_eq!(q.previous_titles, vec!["The Nilfgaardian Connection"]);
        assert!(q.cutoff_titles.is_empty());
        assert!(q.important_notes.is_none());
    }

    #[test]
    fn parses_unmarked_quest() {
        let q = parse_quest(&fixture("unmarked", 24688, "Deadly Crossing")).unwrap();
        assert_eq!(q.quest_type, QuestType::SecondaryQuest);
        assert!(q.is_unmarked);
        assert_eq!(q.region, Region::Velen);
        assert_eq!(q.recommended_level, None);
    }

    #[test]
    fn rejects_pages_without_quest_infobox() {
        let err = parse_quest(&fixture("horse_racing", 70942, "Horse racing")).unwrap_err();
        assert!(matches!(err, ScraperError::NotAQuest(_)));
    }

    #[test]
    fn link_annotations_and_namespaces() {
        let value = "[[Brothers In Arms: Novigrad]] {{Small|dependent}}<br/>[[The Isle of Mists (quest)|The Isle of Mists]]<br/>[[File:x.png]]<br/>[[Contract: Dragon]]";
        assert_eq!(required_links(value), vec!["The Isle of Mists (quest)", "Contract: Dragon"]);
        assert_eq!(
            required_links("[[ghosts_of_the_Past]] {{Small|if started, must be completed before continuing}}"),
            vec!["Ghosts of the Past"]
        );
    }

    #[test]
    fn strips_markup() {
        assert_eq!(
            strip_markup("''[[Geralt of Rivia|Geralt]] met [[Ciri]]'' {{Small|optional}} for 100 {{xp}}<br/>next"),
            "Geralt met Ciri (optional) for 100\nnext"
        );
    }

    #[test]
    fn strips_disambiguators() {
        assert_eq!(strip_disambiguator("Ostatnie życzenie (zadanie)"), "Ostatnie życzenie");
        assert_eq!(strip_disambiguator("Последнее желание (квест)"), "Последнее желание");
        assert_eq!(strip_disambiguator("Contract: Dragon"), "Contract: Dragon");
    }

    #[test]
    fn normalizes_titles() {
        assert_eq!(normalize_title("the_Last  Wish"), "The Last Wish");
        assert_eq!(normalize_title("ąb"), "Ąb");
    }
}
