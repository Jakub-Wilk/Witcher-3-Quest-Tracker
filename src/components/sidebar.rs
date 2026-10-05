use dioxus::prelude::*;
use quest_db::QuestStatus;

use super::{QUEST_TYPES, SOURCES, source_label, type_label};
use crate::state::AppState;

/// Left panel: expansion filter, category filter with counts, and overall progress.
#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let filter = state.filter.read().clone();
    let summary = state.summary.read().clone();

    // (done, total) per quest type, within the selected expansion
    let counts: Vec<_> = {
        let rows = state.rows.read();
        QUEST_TYPES
            .iter()
            .map(|&t| {
                let in_type = rows
                    .iter()
                    .filter(|(q, _)| q.quest_type == t && filter.source.is_none_or(|s| s == q.source));
                let (done, total) = in_type.fold((0, 0), |(d, n), (_, p)| {
                    (d + usize::from(p.status == QuestStatus::Completed), n + 1)
                });
                (t, done, total)
            })
            .filter(|&(_, _, total)| total > 0)
            .collect()
    };

    let pct = if summary.total_quests > 0 {
        summary.completed as f64 * 100.0 / summary.total_quests as f64
    } else {
        0.0
    };

    rsx! {
        aside { class: "pane sidebar",
            section {
                h3 { class: "section-title", "Expansion" }
                div { class: "chips",
                    button {
                        class: if filter.source.is_none() { "chip active" } else { "chip" },
                        onclick: move |_| state.filter.write().source = None,
                        "All"
                    }
                    for s in SOURCES {
                        button {
                            class: if filter.source == Some(s) { "chip active" } else { "chip" },
                            onclick: move |_| state.filter.write().source = Some(s),
                            "{source_label(s)}"
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Category" }
                ul { class: "categories",
                    li {
                        class: if filter.quest_type.is_none() { "category active" } else { "category" },
                        onclick: move |_| state.filter.write().quest_type = None,
                        span { "All quests" }
                    }
                    for (t, done, total) in counts {
                        li {
                            class: if filter.quest_type == Some(t) { "category active" } else { "category" },
                            onclick: move |_| {
                                let mut f = state.filter.write();
                                f.quest_type = if f.quest_type == Some(t) { None } else { Some(t) };
                            },
                            span { "{type_label(t)}" }
                            span { class: "count", "{done}/{total}" }
                        }
                    }
                }
            }
            section { class: "progress-section",
                h3 { class: "section-title", "Progress" }
                div { class: "progress-bar",
                    div { class: "progress-fill", style: "width: {pct:.1}%" }
                }
                div { class: "progress-stats",
                    span { "{summary.completed}/{summary.total_quests} completed" }
                    span { class: "gold", "{pct:.1}%" }
                }
                if summary.failed > 0 {
                    div { class: "progress-failed", "{summary.failed} failed" }
                }
            }
        }
    }
}
