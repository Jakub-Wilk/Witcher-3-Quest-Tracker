use dioxus::prelude::*;
use quest_db::QuestStatus;

use super::{QUEST_TYPES, SOURCES, region_label, source_label, type_label};
use crate::state::AppState;
use crate::view_options::{REGIONS, STATUSES, SortKey, ViewOptions, toggle};

fn status_filter_label(status: QuestStatus) -> &'static str {
    match status {
        QuestStatus::NotStarted => "Not started",
        QuestStatus::InProgress => "In progress",
        QuestStatus::Completed => "Completed",
        QuestStatus::Failed => "Failed",
    }
}

/// Left panel: expansion, category, status and region filters, sort order, and progress.
/// Each filter is a multi-select where "All" clears the selection. All choices are persisted
/// (see `ViewOptions`).
#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let view = state.view.read().clone();
    let summary = state.summary.read().clone();

    let (counts, regions) = {
        let rows = state.rows.read();
        // (done, total) per quest type, within the selected expansion and region
        let counts: Vec<_> = QUEST_TYPES
            .iter()
            .map(|&t| {
                let in_type = rows.iter().filter(|(q, _)| q.quest_type == t && view.matches_scope(q));
                let (done, total) = in_type.fold((0, 0), |(d, n), (_, p)| {
                    (d + usize::from(p.status() == QuestStatus::Completed), n + 1)
                });
                (t, done, total)
            })
            .filter(|&(_, _, total)| total > 0)
            .collect();
        // Regions that have quests in the selected expansions (plus the selected ones, so an
        // active filter is never hidden)
        let regions: Vec<_> = REGIONS
            .into_iter()
            .filter(|&r| {
                view.regions.contains(&r)
                    || rows.iter().any(|(q, _)| {
                        q.region == r && (view.sources.is_empty() || view.sources.contains(&q.source))
                    })
            })
            .collect();
        (counts, regions)
    };

    let pct = if summary.total_quests > 0 {
        summary.completed as f64 * 100.0 / summary.total_quests as f64
    } else {
        0.0
    };
    let is_default = ViewOptions { search: view.search.clone(), ..Default::default() } == view;

    rsx! {
        aside { class: "pane sidebar",
            section {
                h3 { class: "section-title", "Expansion" }
                div { class: "chips",
                    button {
                        class: if view.sources.is_empty() { "chip active" } else { "chip" },
                        onclick: move |_| state.view.write().sources.clear(),
                        "All"
                    }
                    for s in SOURCES {
                        button {
                            class: if view.sources.contains(&s) { "chip active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().sources, s),
                            "{source_label(s)}"
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Category" }
                ul { class: "categories",
                    li {
                        class: if view.quest_types.is_empty() { "category active" } else { "category" },
                        onclick: move |_| state.view.write().quest_types.clear(),
                        span { "All quests" }
                    }
                    for (t, done, total) in counts {
                        li {
                            class: if view.quest_types.contains(&t) { "category active" } else { "category" },
                            onclick: move |_| toggle(&mut state.view.write().quest_types, t),
                            span { "{type_label(t)}" }
                            span { class: "count", "{done}/{total}" }
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Status" }
                div { class: "chips",
                    button {
                        class: if view.statuses.is_empty() { "chip active" } else { "chip" },
                        onclick: move |_| state.view.write().statuses.clear(),
                        "All"
                    }
                    for s in STATUSES {
                        button {
                            class: if view.statuses.contains(&s) { "chip active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().statuses, s),
                            "{status_filter_label(s)}"
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Region" }
                div { class: "chips",
                    button {
                        class: if view.regions.is_empty() { "chip active" } else { "chip" },
                        onclick: move |_| state.view.write().regions.clear(),
                        "All"
                    }
                    for r in regions {
                        button {
                            class: if view.regions.contains(&r) { "chip active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().regions, r),
                            "{region_label(r)}"
                        }
                    }
                }
            }
            section {
                label { class: "field",
                    span { class: "section-title", "Sort by" }
                    select {
                        class: "select",
                        onchange: move |e| {
                            if let Some(&key) = e.value().parse::<usize>().ok().and_then(|i| SortKey::ALL.get(i)) {
                                state.view.write().sort = key;
                            }
                        },
                        for (i, key) in SortKey::ALL.into_iter().enumerate() {
                            option { value: "{i}", selected: view.sort == key, "{key.label()}" }
                        }
                    }
                }
                if !is_default {
                    button {
                        class: "btn btn-ghost reset-filters",
                        onclick: move |_| {
                            let search = state.view.peek().search.clone();
                            state.view.set(ViewOptions { search, ..Default::default() });
                        },
                        "Reset filters"
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
