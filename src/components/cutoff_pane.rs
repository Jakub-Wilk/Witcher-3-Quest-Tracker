use std::collections::{BTreeMap, HashMap};

use dioxus::prelude::*;
use quest_db::{Quest, QuestProgress, QuestStatus};

use super::is_open;
use crate::state::AppState;

/// Right panel: quests at risk from upcoming cutoff points, quests already locked out,
/// and open quests whose prerequisites are not yet completed.
#[component]
pub fn CutoffPane() -> Element {
    let state = use_context::<AppState>();
    let rows = state.rows.read();
    let by_id: HashMap<i64, &(Quest, QuestProgress)> = rows.iter().map(|r| (r.0.id, r)).collect();

    // cutoff quest id -> open quests that it will lock out
    let mut impending: BTreeMap<(i32, String, i64), Vec<&str>> = BTreeMap::new();
    let mut missed: Vec<(&str, &str)> = Vec::new();
    let mut prereq_warnings: Vec<(&str, Vec<&str>)> = Vec::new();

    for (quest, _) in rows.iter().filter(|(_, p)| is_open(p.status)) {
        if let Some((cutoff, cutoff_progress)) = quest.cutoff_quest_id.and_then(|id| by_id.get(&id)).map(|r| (&r.0, &r.1)) {
            if is_open(cutoff_progress.status) {
                let key = (cutoff.sort_order.unwrap_or(i32::MAX), cutoff.name.clone(), cutoff.id);
                impending.entry(key).or_default().push(&quest.name);
            } else {
                missed.push((&quest.name, &cutoff.name));
            }
        }

        let missing: Vec<&str> = quest
            .prerequisite_ids
            .iter()
            .filter_map(|id| by_id.get(id))
            .filter(|(_, p)| p.status != QuestStatus::Completed)
            .map(|(q, _)| q.name.as_str())
            .collect();
        if !missing.is_empty() {
            prereq_warnings.push((&quest.name, missing));
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

            h3 { class: "section-title prereq-title", "🔗 Prerequisites ({prereq_warnings.len()})" }
            if prereq_warnings.is_empty() {
                p { class: "muted", "All open quests have their prerequisites completed." }
            } else {
                ul { class: "prereq-list",
                    for (name, missing) in prereq_warnings.iter() {
                        li {
                            div { "{name}" }
                            div { class: "muted small", "needs: {missing.join(\", \")}" }
                        }
                    }
                }
            }
        }
    }
}
