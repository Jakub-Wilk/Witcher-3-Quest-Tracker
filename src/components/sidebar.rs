use dioxus::prelude::*;
use quest_db::QuestStatus;

use super::{QUEST_TYPES, SOURCES, region_label, source_label, type_label};
use crate::state::AppState;
use crate::view_options::{REGIONS, SortKey, StatusFilter, ViewOptions};

/// Left panel: expansion, category, status and region filters, sort order, and progress.
/// All choices are persisted (see `ViewOptions`).
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
        // Regions that have quests in the selected expansion (plus the selected one, so the
        // dropdown never misrepresents an active filter)
        let regions: Vec<_> = REGIONS
            .into_iter()
            .filter(|&r| {
                view.region == Some(r)
                    || rows.iter().any(|(q, _)| q.region == r && view.source.is_none_or(|s| s == q.source))
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
                        class: if view.source.is_none() { "chip active" } else { "chip" },
                        onclick: move |_| state.view.write().source = None,
                        "All"
                    }
                    for s in SOURCES {
                        button {
                            class: if view.source == Some(s) { "chip active" } else { "chip" },
                            onclick: move |_| state.view.write().source = Some(s),
                            "{source_label(s)}"
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Category" }
                ul { class: "categories",
                    li {
                        class: if view.quest_type.is_none() { "category active" } else { "category" },
                        onclick: move |_| state.view.write().quest_type = None,
                        span { "All quests" }
                    }
                    for (t, done, total) in counts {
                        li {
                            class: if view.quest_type == Some(t) { "category active" } else { "category" },
                            onclick: move |_| {
                                let mut v = state.view.write();
                                v.quest_type = if v.quest_type == Some(t) { None } else { Some(t) };
                            },
                            span { "{type_label(t)}" }
                            span { class: "count", "{done}/{total}" }
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", "Status" }
                div { class: "chips",
                    for s in StatusFilter::ALL {
                        button {
                            class: if view.status == s { "chip active" } else { "chip" },
                            onclick: move |_| state.view.write().status = s,
                            "{s.label()}"
                        }
                    }
                }
            }
            section {
                label { class: "field",
                    span { class: "section-title", "Region" }
                    select {
                        class: "select",
                        onchange: move |e| {
                            state.view.write().region = e.value().parse::<usize>().ok().and_then(|i| REGIONS.get(i).copied());
                        },
                        option { value: "", selected: view.region.is_none(), "All regions" }
                        for r in regions {
                            option {
                                value: "{REGIONS.iter().position(|&x| x == r).unwrap_or_default()}",
                                selected: view.region == Some(r),
                                "{region_label(r)}"
                            }
                        }
                    }
                }
                label { class: "field sort-field",
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
