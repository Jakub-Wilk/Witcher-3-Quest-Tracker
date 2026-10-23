use companion_db::QuestStatus;
use dioxus::prelude::*;

use crate::components::ornament::Divider;
use crate::components::{QUEST_TYPES, SOURCES, region_label, source_label, status_label, type_label};
use crate::state::AppState;
use crate::t;
use crate::view_options::{REGIONS, STATUSES, SortKey, ViewOptions, toggle};

/// Left panel: expansion, category, status and region filters, sort order, and progress.
/// Each filter is a multi-select where "All" clears the selection. All choices are persisted
/// (see `ViewOptions`).
#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
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
        aside { class: "panel sidebar",
            section { class: "progress-section",
                h3 { class: "section-title", {t!(tr, "quests.progress")} }
                div { class: "progress-ring",
                    div { class: "progress-figure", "{pct:.0}" span { "%" } }
                    div { class: "progress-bar",
                        div { class: "progress-fill", style: "transform: scaleX({pct / 100.0:.4})" }
                    }
                    div { class: "progress-stats",
                        {t!(tr, "quests.completed_of", done = summary.completed, total = summary.total_quests)}
                    }
                    if summary.failed > 0 {
                        div { class: "progress-failed", {t!(tr, "quests.failed_count", n = summary.failed)} }
                    }
                }
            }
            Divider {}
            section {
                h3 { class: "section-title", {t!(tr, "quests.expansion")} }
                div { class: "chips chips-wrap",
                    button {
                        class: if view.sources.is_empty() { "chip is-active" } else { "chip" },
                        onclick: move |_| state.view.write().sources.clear(),
                        {t!(tr, "common.all")}
                    }
                    for s in SOURCES {
                        button {
                            class: if view.sources.contains(&s) { "chip is-active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().sources, s),
                            {source_label(tr, s)}
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", {t!(tr, "quests.category")} }
                ul { class: "categories",
                    li {
                        class: if view.quest_types.is_empty() { "category is-active" } else { "category" },
                        onclick: move |_| state.view.write().quest_types.clear(),
                        span { {t!(tr, "quests.all_quests")} }
                    }
                    for (t, done, total) in counts {
                        li {
                            class: if view.quest_types.contains(&t) { "category is-active" } else { "category" },
                            onclick: move |_| toggle(&mut state.view.write().quest_types, t),
                            span { {type_label(tr, t)} }
                            span { class: "count", "{done}/{total}" }
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", {t!(tr, "quests.status")} }
                div { class: "chips chips-wrap",
                    button {
                        class: if view.statuses.is_empty() { "chip is-active" } else { "chip" },
                        onclick: move |_| state.view.write().statuses.clear(),
                        {t!(tr, "common.all")}
                    }
                    for s in STATUSES {
                        button {
                            class: if view.statuses.contains(&s) { "chip is-active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().statuses, s),
                            {status_label(tr, s)}
                        }
                    }
                }
            }
            section {
                h3 { class: "section-title", {t!(tr, "quests.region")} }
                div { class: "chips chips-wrap",
                    button {
                        class: if view.regions.is_empty() { "chip is-active" } else { "chip" },
                        onclick: move |_| state.view.write().regions.clear(),
                        {t!(tr, "common.all")}
                    }
                    for r in regions {
                        button {
                            class: if view.regions.contains(&r) { "chip is-active" } else { "chip" },
                            onclick: move |_| toggle(&mut state.view.write().regions, r),
                            {region_label(tr, r)}
                        }
                    }
                }
            }
            section {
                label { class: "field",
                    span { class: "section-title", {t!(tr, "quests.sort_by")} }
                    select {
                        class: "select",
                        onchange: move |e| {
                            if let Some(&key) = e.value().parse::<usize>().ok().and_then(|i| SortKey::ALL.get(i)) {
                                state.view.write().sort = key;
                            }
                        },
                        for (i, key) in SortKey::ALL.into_iter().enumerate() {
                            option { value: "{i}", selected: view.sort == key, {sort_label(tr, key)} }
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
                        {t!(tr, "quests.reset_filters")}
                    }
                }
            }
        }
    }
}

fn sort_label(tr: crate::i18n::Tr, key: SortKey) -> &'static str {
    match key {
        SortKey::Story => t!(tr, "sort.story"),
        SortKey::Level => t!(tr, "sort.level"),
        SortKey::Name => t!(tr, "sort.name"),
        SortKey::Region => t!(tr, "sort.region"),
        SortKey::Cutoff => t!(tr, "sort.cutoff"),
    }
}
