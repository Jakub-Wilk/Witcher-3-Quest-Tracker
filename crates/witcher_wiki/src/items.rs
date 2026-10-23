//! Item pictures. Every Witcher 3 item page has an `{{Infobox Item3}}` (or `{{Infobox
//! Item3/baw}}`) whose `id` field holds the game's internal item names and whose `image` field
//! names the picture file.

use std::collections::{BTreeSet, HashMap};

use crate::client::{TITLES_PER_REQUEST, WikiClient, array};
use crate::error::Result;
use crate::wikitext::find_templates;

/// Templates whose pages describe Witcher 3 items.
pub const ITEM_TEMPLATES: &[&str] = &["Template:Infobox Item3", "Template:Infobox Item3/baw"];

/// One item infobox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemInfobox {
    pub page_title: String,
    /// Internal item names (an infobox may cover several variants).
    pub ids: Vec<String>,
    /// The item's English name.
    pub name: String,
    /// Picture file name without the `File:` prefix, spaces for underscores.
    pub file: String,
}

/// Where a wiki file can be downloaded, and its hash to detect changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WikiFile {
    pub url: String,
    pub sha1: String,
    pub mime: String,
}

impl WikiClient {
    /// Titles of the articles that use `template`.
    pub async fn list_embedding(&self, template: &str) -> Result<Vec<String>> {
        let mut titles = Vec::new();
        let params = [
            ("action", "query"),
            ("list", "embeddedin"),
            ("eititle", template),
            ("einamespace", "0"),
            ("eilimit", "max"),
        ];
        self.query_all(&params, |json| {
            for page in array(&json["query"]["embeddedin"]) {
                if let Some(title) = page["title"].as_str() {
                    titles.push(title.to_string());
                }
            }
        })
        .await?;
        Ok(titles)
    }

    /// Every item infobox on the wiki, reporting `(done, total)` pages as they are read.
    pub async fn fetch_item_infoboxes(&self, on_progress: impl FnMut(usize, usize)) -> Result<Vec<ItemInfobox>> {
        let mut titles = BTreeSet::new();
        for template in ITEM_TEMPLATES {
            titles.extend(self.list_embedding(template).await?);
        }
        let titles: Vec<String> = titles.into_iter().collect();
        let pages = self.fetch_pages(&titles, on_progress).await?;
        Ok(pages.iter().flat_map(|page| item_infoboxes(&page.title, &page.wikitext)).collect())
    }

    /// Download URL and hash per file name (without `File:`); files the wiki lacks are omitted.
    pub async fn file_info(
        &self,
        files: &[String],
        mut on_progress: impl FnMut(usize, usize),
    ) -> Result<HashMap<String, WikiFile>> {
        let mut out = HashMap::new();
        for (i, chunk) in files.chunks(TITLES_PER_REQUEST).enumerate() {
            let joined = chunk.iter().map(|f| format!("File:{f}")).collect::<Vec<_>>().join("|");
            let params = [
                ("action", "query"),
                ("prop", "imageinfo"),
                ("iiprop", "url|sha1|mime"),
                ("titles", joined.as_str()),
            ];
            self.query_all(&params, |json| {
                // The wiki normalizes titles (first letter capitalized, `_` → space); map back.
                let normalized: HashMap<&str, &str> = array(&json["query"]["normalized"])
                    .iter()
                    .filter_map(|n| Some((n["to"].as_str()?, n["from"].as_str()?)))
                    .collect();
                for page in array(&json["query"]["pages"]) {
                    let Some(title) = page["title"].as_str() else { continue };
                    let info = &page["imageinfo"][0];
                    let (Some(url), Some(sha1)) = (info["url"].as_str(), info["sha1"].as_str()) else { continue };
                    let requested = normalized.get(title).copied().unwrap_or(title);
                    let file = requested.trim_start_matches("File:").replace('_', " ");
                    out.insert(file, WikiFile {
                        url: url.to_string(),
                        sha1: sha1.to_string(),
                        mime: info["mime"].as_str().unwrap_or("image/png").to_string(),
                    });
                }
            })
            .await?;
            on_progress(((i + 1) * TITLES_PER_REQUEST).min(files.len()), files.len());
        }
        Ok(out)
    }
}

/// The item infoboxes on one page. Infoboxes without a picture are skipped.
pub fn item_infoboxes(page_title: &str, wikitext: &str) -> Vec<ItemInfobox> {
    ["infobox item3", "infobox item3/baw"]
        .iter()
        .flat_map(|name| find_templates(wikitext, name))
        .filter_map(|params| {
            let file = image_file(params.get("image")?);
            if file.is_empty() {
                return None;
            }
            let ids = params
                .get("id")
                .map(|raw| {
                    split_lines(&strip_comments(raw))
                        .into_iter()
                        .map(|id| strip_inline(&id).replace("'''", "").replace("''", "").trim().to_string())
                        .filter(|id| !id.is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let name = params.get("name").map(|n| strip_inline(n).replace("[[", "").replace("]]", ""));
            Some(ItemInfobox {
                page_title: page_title.to_string(),
                ids,
                name: name.unwrap_or_default().trim().to_string(),
                file,
            })
        })
        .collect()
}

/// The first file named by an `image` value: a bare name, `[[File:...]]` or a `<gallery>`.
fn image_file(raw: &str) -> String {
    let mut s = raw.trim();
    if s.to_ascii_lowercase().starts_with("<gallery") {
        s = s.find('>').map_or("", |i| &s[i + 1..]).trim_start();
    }
    let owned;
    if let Some(start) = s.find("[[") {
        let inner = &s[start + 2..s[start..].find("]]").map_or(s.len(), |e| start + e)];
        if let Some((ns, rest)) = inner.split_once(':') {
            if ["file", "image"].contains(&ns.trim().to_ascii_lowercase().as_str()) {
                owned = rest.split('|').next().unwrap_or_default().to_string();
                s = &owned;
            }
        }
    }
    let end = [s.find("<br"), s.find('\n'), s.find('|')].into_iter().flatten().min().unwrap_or(s.len());
    s[..end].trim().replace('_', " ")
}

fn strip_comments(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    while let Some(start) = rest.find("<!--") {
        out.push_str(&rest[..start]);
        rest = rest[start..].find("-->").map_or("", |end| &rest[start + end + 3..]);
    }
    out.push_str(rest);
    out
}

/// Splits on `<br>` variants and newlines.
fn split_lines(s: &str) -> Vec<String> {
    let mut lines = Vec::new();
    let mut current = String::new();
    let mut rest = s;
    while !rest.is_empty() {
        if rest.len() >= 3 && rest[..3].eq_ignore_ascii_case("<br") {
            let end = rest.find('>').map_or(rest.len(), |i| i + 1);
            lines.push(std::mem::take(&mut current));
            rest = &rest[end..];
        } else {
            let c = rest.chars().next().expect("not empty");
            if c == '\n' {
                lines.push(std::mem::take(&mut current));
            } else {
                current.push(c);
            }
            rest = &rest[c.len_utf8()..];
        }
    }
    lines.push(current);
    lines
}

/// Removes HTML tags and `{{templates}}`.
fn strip_inline(s: &str) -> String {
    let mut out = String::new();
    let mut rest = s;
    loop {
        let next = [rest.find('<'), rest.find("{{")].into_iter().flatten().min();
        let Some(start) = next else { break };
        out.push_str(&rest[..start]);
        let (close, len) = if rest[start..].starts_with("{{") { ("}}", 2) } else { (">", 1) };
        rest = rest[start..].find(close).map_or("", |end| &rest[start + end + len..]);
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_item_infoboxes() {
        let text = r#"{{Infobox Item3
| name = '''Swallow''' {{Tooltip|x}}
| image = Tw3 potion swallow.png
| id = Swallow 1<br />Swallow 2 <!-- enhanced -->
| type = [[Potions|Potion]]
}}
Some text.
{{Infobox Item3/baw
| name = [[Hen Gaidth]] armor
| image = <gallery>
Tw3 hen gaidth armor.png|Male
Other.png
</gallery>
| id = '''Bob Hen Gaidth Armor'''
}}
{{Infobox Item3
| name = Missing picture
| id = Nothing
}}
{{Infobox Item3|name=Linked|image=[[File:Tw3_linked_item.png|64px]]|id=Linked 1}}"#;
        let boxes = item_infoboxes("Swallow", text);
        assert_eq!(boxes.len(), 3);
        assert_eq!(boxes[0].ids, vec!["Swallow 1", "Swallow 2"]);
        assert_eq!(boxes[0].file, "Tw3 potion swallow.png");
        assert_eq!(boxes[0].name, "'''Swallow'''");
        assert_eq!(boxes[1].file, "Tw3 linked item.png");
        assert_eq!(boxes[2].file, "Tw3 hen gaidth armor.png");
        assert_eq!(boxes[2].ids, vec!["Bob Hen Gaidth Armor"]);
        assert_eq!(boxes[2].name, "Hen Gaidth armor");
    }
}
