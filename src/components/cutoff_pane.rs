use std::collections::{BTreeMap, HashMap};

use dioxus::prelude::*;
use quest_db::{Quest, QuestProgress};

use super::is_open;
use crate::state::AppState;

/// Right panel: open quests that an upcoming cutoff point will lock out, and open quests
/// whose cutoff has already been passed.
#[component]
pub fn CutoffPane() -> Element {
    let state = use_context::<AppState>();
    let rows = state.rows.read();
    let by_id: HashMap<i64, &(Quest, QuestProgress)> = rows.iter().map(|r| (r.0.id, r)).collect();

    // (cutoff sort order, cutoff name, cutoff id) -> open quests it will lock out
    let mut impending: BTreeMap<(i32, &str, i64), Vec<&str>> = BTreeMap::new();
    let mut missed: Vec<(&str, &str)> = Vec::new();

    for (quest, _) in rows.iter().filter(|(_, p)| is_open(p.status())) {
        let Some((cutoff, cutoff_progress)) = quest.cutoff_quest_id.and_then(|id| by_id.get(&id)).map(|r| (&r.0, &r.1))
        else {
            continue;
        };
        if is_open(cutoff_progress.status()) {
            let key = (cutoff.sort_order.unwrap_or(i32::MAX), cutoff.title.as_str(), cutoff.id);
            impending.entry(key).or_default().push(quest.title.as_str());
        } else {
            missed.push((quest.title.as_str(), cutoff.title.as_str()));
        }
    }

    rsx! {
        aside { class: "pane cutoff-pane",
            h3 { class: "section-title warn-title", "⚠ Impending Cutoffs" }
            if impending.is_empty() {
                p { class: "muted", "No open quests are at risk of being locked out." }
            }
            for ((_, cutoff_name, cutoff_id), at_risk) in impending.iter() {
                div { key: "{cutoff_id}", class: "warning-pane",
                    div { class: "cutoff-name", "📌 {cutoff_name}" }
                    div { class: "muted small", "Completing it will lock out:" }
                    ul { class: "risk-list",
                        for name in at_risk.iter() {
                            li { "{name}" }
                        }
                    }
                }
            }

            if !missed.is_empty() {
                h3 { class: "section-title", "✖ Locked Out ({missed.len()})" }
                ul { class: "risk-list missed",
                    for (name, cutoff) in missed.iter() {
                        li { "{name} " span { class: "muted small", "— after {cutoff}" } }
                    }
                }
            }
        }
    }
}
