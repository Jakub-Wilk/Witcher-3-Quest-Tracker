use std::collections::{BTreeMap, HashMap};

use companion_db::{Quest, QuestProgress, QuestStatus};
use dioxus::prelude::*;

use crate::components::is_open;
use crate::components::ornament::{Divider, Glyph, Icon};
use crate::state::AppState;
use crate::t;

/// Right panel: open quests that an upcoming cutoff point will lock out, and open quests
/// whose cutoff has already been passed.
#[component]
pub fn CutoffPane() -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    let rows = state.rows.read();
    let by_id: HashMap<i64, &(Quest, QuestProgress)> = rows.iter().map(|r| (r.0.id, r)).collect();

    // (cutoff sort order, cutoff name, cutoff id, cutoff in progress) -> open quests it will lock out
    let mut impending: BTreeMap<(i32, &str, i64, bool), Vec<&str>> = BTreeMap::new();
    let mut missed: Vec<(&str, &str)> = Vec::new();

    for (quest, _) in rows.iter().filter(|(_, p)| is_open(p.status())) {
        let Some((cutoff, cutoff_progress)) = quest.cutoff_quest_id.and_then(|id| by_id.get(&id)).map(|r| (&r.0, &r.1))
        else {
            continue;
        };
        if is_open(cutoff_progress.status()) {
            let active = cutoff_progress.status() == QuestStatus::InProgress;
            let key = (cutoff.sort_order.unwrap_or(i32::MAX), cutoff.title.as_str(), cutoff.id, active);
            impending.entry(key).or_default().push(quest.title.as_str());
        } else {
            missed.push((quest.title.as_str(), cutoff.title.as_str()));
        }
    }

    rsx! {
        aside { class: "panel cutoff-pane",
            div { class: "panel-head",
                h2 { {t!(tr, "cutoffs.title")} }
                Divider {}
            }
            if impending.is_empty() {
                p { class: "muted empty-note", {t!(tr, "cutoffs.none")} }
            }
            for (i, ((_, cutoff_name, cutoff_id, active), at_risk)) in impending.iter().enumerate() {
                div {
                    key: "{cutoff_id}",
                    class: if *active { "cutoff is-active" } else { "cutoff" },
                    style: "--i: {i.min(12)}",
                    title: if *active { t!(tr, "cutoffs.active_hint") },
                    div { class: "cutoff-name",
                        Glyph { icon: Icon::Pin }
                        span { "{cutoff_name}" }
                    }
                    div { class: "muted small", {t!(tr, "cutoffs.locks_out")} }
                    ul { class: "risk-list",
                        for name in at_risk.iter() {
                            li { "{name}" }
                        }
                    }
                }
            }

            if !missed.is_empty() {
                h3 { class: "section-title missed-title",
                    Glyph { icon: Icon::Close }
                    {t!(tr, "cutoffs.locked_out", n = missed.len())}
                }
                ul { class: "risk-list missed",
                    for (name, cutoff) in missed.iter() {
                        li { "{name} " span { class: "muted small", {t!(tr, "cutoffs.after", name = cutoff)} } }
                    }
                }
            }
        }
    }
}
