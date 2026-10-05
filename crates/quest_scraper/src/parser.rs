use regex::Regex;
use scraper::{Html, Selector};

use crate::error::Result;
use crate::models::{ScrapedQuest, ScrapedQuestSource, ScrapedQuestType, ScrapedRegion};

/// Parses raw Fandom wiki HTML into a `ScrapedQuest` struct.
pub fn parse_quest_html(html_str: &str, page_title: &str, wiki_url: &str) -> Result<ScrapedQuest> {
    let document = Html::parse_document(html_str);

    let infobox_sel = Selector::parse(".portable-infobox, table.infobox").unwrap();
    let data_item_sel = Selector::parse(".pi-data, tr").unwrap();
    let label_sel = Selector::parse(".pi-data-label, th").unwrap();
    let value_sel = Selector::parse(".pi-data-value, td").unwrap();

    let infobox = document.select(&infobox_sel).next();

    let mut source = ScrapedQuestSource::BaseGame;
    let mut quest_type = ScrapedQuestType::MainQuest;
    let mut region = ScrapedRegion::Unknown;
    let mut level = None;
    let mut is_unmarked = false;
    let mut cutoff_quest_name = None;
    let mut prerequisite_quest_names = Vec::new();

    if let Some(box_elem) = infobox {
        for data_item in box_elem.select(&data_item_sel) {
            let label = data_item
                .select(&label_sel)
                .next()
                .map(|e| e.text().collect::<String>().trim().to_lowercase())
                .unwrap_or_default();

            let val_elem = data_item.select(&value_sel).next();
            let value = val_elem
                .map(|e| e.text().collect::<String>().trim().to_string())
                .unwrap_or_default();

            if label.is_empty() || value.is_empty() {
                continue;
            }

            match label.as_str() {
                l if l.contains("expansion") || l.contains("dlc") || l.contains("source") => {
                    source = parse_source(&value);
                }
                l if l.contains("type") => {
                    quest_type = parse_quest_type(&value);
                }
                l if l.contains("location") || l.contains("region") => {
                    region = parse_region(&value);
                }
                l if l.contains("level") || l.contains("suggested") || l.contains("rec") => {
                    level = parse_level(&value);
                }
                l if l.contains("unmark") || l.contains("marker") => {
                    is_unmarked = value.eq_ignore_ascii_case("yes")
                        || value.eq_ignore_ascii_case("true")
                        || value.to_lowercase().contains("unmarked");
                }
                l if l.contains("cutoff") => {
                    cutoff_quest_name = extract_first_link_or_text(val_elem, &value);
                }
                l if l.contains("previous") || l.contains("prereq") || l.contains("preceded") || l.contains("requires") => {
                    extract_links_or_text(val_elem, &value, &mut prerequisite_quest_names);
                }
                _ => {}
            }
        }
    }

    // Also extract prerequisite quest titles from condition banners in lead text
    extract_condition_prerequisites(&document, &mut prerequisite_quest_names);

    // Also check document text for unmarked indicators
    let full_text = document.root_element().text().collect::<String>().to_lowercase();
    if full_text.contains("unmarked") || full_text.contains("no map marker") {
        is_unmarked = true;
    }

    // Infer quest_type or region from text if infobox was missing or incomplete
    if quest_type == ScrapedQuestType::MainQuest {
        if full_text.contains("secondary quest")
            || full_text.contains("side quest")
            || full_text.contains("unmarked quest")
        {
            quest_type = ScrapedQuestType::SecondaryQuest;
        } else if full_text.contains("witcher contract") || full_text.contains("contract:") {
            quest_type = ScrapedQuestType::WitcherContract;
        } else if full_text.contains("scavenger hunt:") {
            quest_type = ScrapedQuestType::ScavengerHunt;
        } else if full_text.contains("treasure hunt:") {
            quest_type = ScrapedQuestType::TreasureHunt;
        }
    }

    if region == ScrapedRegion::Unknown {
        region = parse_region(&full_text);
    }

    let description = extract_lead_paragraph(&document);

    // Clean title (remove underscore formatting)
    let clean_name = page_title.replace('_', " ");

    Ok(ScrapedQuest {
        name: clean_name,
        source,
        quest_type,
        region,
        recommended_level: level,
        is_unmarked,
        sort_order: None,
        description,
        cutoff_quest_name,
        prerequisite_quest_names,
        wiki_url: wiki_url.to_string(),
    })
}

fn extract_first_link_or_text(val_elem: Option<scraper::ElementRef>, raw_text: &str) -> Option<String> {
    let a_sel = Selector::parse("a").unwrap();
    if let Some(elem) = val_elem {
        if let Some(a) = elem.select(&a_sel).next() {
            let link_text = a.text().collect::<String>().trim().to_string();
            if !link_text.is_empty() {
                return Some(link_text);
            }
        }
    }
    if !raw_text.is_empty() && !raw_text.eq_ignore_ascii_case("none") {
        Some(raw_text.to_string())
    } else {
        None
    }
}

fn extract_links_or_text(
    val_elem: Option<scraper::ElementRef>,
    raw_text: &str,
    target_vec: &mut Vec<String>,
) {
    let a_sel = Selector::parse("a").unwrap();
    let mut found_any = false;

    if let Some(elem) = val_elem {
        for a in elem.select(&a_sel) {
            let link_text = a.text().collect::<String>().trim().to_string();
            if !link_text.is_empty()
                && !link_text.eq_ignore_ascii_case("none")
                && !target_vec.contains(&link_text)
            {
                target_vec.push(link_text);
                found_any = true;
            }
        }
    }

    if !found_any && !raw_text.is_empty() && !raw_text.eq_ignore_ascii_case("none") {
        for part in raw_text.split(',') {
            let cleaned = part.trim().to_string();
            if !cleaned.is_empty() && !target_vec.contains(&cleaned) {
                target_vec.push(cleaned);
            }
        }
    }
}

fn extract_condition_prerequisites(doc: &Html, target_vec: &mut Vec<String>) {
    let p_sel = Selector::parse(".mw-parser-output > p, .condition").unwrap();
    let a_sel = Selector::parse("a").unwrap();

    for elem in doc.select(&p_sel) {
        let text = elem.text().collect::<String>();
        if text.to_lowercase().contains("prerequisite") {
            for a in elem.select(&a_sel) {
                let link_text = a.text().collect::<String>().trim().to_string();
                if !link_text.is_empty()
                    && !link_text.to_lowercase().contains("quest")
                    && !target_vec.contains(&link_text)
                {
                    target_vec.push(link_text);
                }
            }
        }
    }
}

fn parse_source(val: &str) -> ScrapedQuestSource {
    let lower = val.to_lowercase();
    if lower.contains("hearts of stone") {
        ScrapedQuestSource::HeartsOfStone
    } else if lower.contains("blood and wine") {
        ScrapedQuestSource::BloodAndWine
    } else {
        ScrapedQuestSource::BaseGame
    }
}

fn parse_quest_type(val: &str) -> ScrapedQuestType {
    let lower = val.to_lowercase();
    if lower.contains("contract") {
        ScrapedQuestType::WitcherContract
    } else if lower.contains("scavenger") {
        ScrapedQuestType::ScavengerHunt
    } else if lower.contains("treasure") {
        ScrapedQuestType::TreasureHunt
    } else if lower.contains("secondary") || lower.contains("side") {
        ScrapedQuestType::SecondaryQuest
    } else {
        ScrapedQuestType::MainQuest
    }
}

fn parse_region(val: &str) -> ScrapedRegion {
    let lower = val.to_lowercase();
    if lower.contains("white orchard") {
        ScrapedRegion::WhiteOrchard
    } else if lower.contains("velen") || lower.contains("no man's land") {
        ScrapedRegion::Velen
    } else if lower.contains("novigrad") {
        ScrapedRegion::Novigrad
    } else if lower.contains("skellige") {
        ScrapedRegion::Skellige
    } else if lower.contains("kaer morhen") {
        ScrapedRegion::KaerMorhen
    } else if lower.contains("toussaint") {
        ScrapedRegion::Toussaint
    } else if lower.contains("oxenfurt") {
        ScrapedRegion::OxenFurtSewers
    } else {
        ScrapedRegion::Unknown
    }
}

fn parse_level(val: &str) -> Option<i32> {
    let re = Regex::new(r"\d+").unwrap();
    re.find(val).and_then(|m| m.as_str().parse::<i32>().ok())
}

fn extract_lead_paragraph(doc: &Html) -> Option<String> {
    let p_sel = Selector::parse(".mw-parser-output > p").unwrap();
    doc.select(&p_sel)
        .map(|p| p.text().collect::<String>().trim().to_string())
        .find(|text| !text.is_empty() && text.len() > 15)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_synthetic_html_with_prerequisites() {
        let sample_html = r#"
            <div class="mw-parser-output">
                <div class="portable-infobox">
                    <div class="pi-data">
                        <div class="pi-data-label">Location</div>
                        <div class="pi-data-value">Novigrad</div>
                    </div>
                    <div class="pi-data">
                        <div class="pi-data-label">Type</div>
                        <div class="pi-data-value">Unmarked secondary quest</div>
                    </div>
                    <div class="pi-data">
                        <div class="pi-data-label">Previous quest(s)</div>
                        <div class="pi-data-value"><a href="/wiki/Pyres_of_Novigrad">Pyres of Novigrad</a></div>
                    </div>
                </div>
                <p>Witch Hunter Raids is an unmarked secondary quest in Novigrad.</p>
            </div>
        "#;

        let quest = parse_quest_html(
            sample_html,
            "Witch_Hunter_Raids",
            "https://witcher.fandom.com/wiki/Witch_Hunter_Raids",
        )
        .unwrap();

        assert_eq!(quest.name, "Witch Hunter Raids");
        assert_eq!(quest.region, ScrapedRegion::Novigrad);
        assert_eq!(quest.quest_type, ScrapedQuestType::SecondaryQuest);
        assert_eq!(quest.prerequisite_quest_names, vec!["Pyres of Novigrad"]);
        assert!(quest.is_unmarked);
    }
}
