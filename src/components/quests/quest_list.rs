use std::collections::HashMap;

use companion_db::QuestStatus;
use dioxus::prelude::*;

use super::quest_card::QuestCard;
use crate::components::ornament::{Divider, Glyph, Icon};
use crate::components::shell::PlaythroughModal;
use crate::state::AppState;
use crate::t;

/// Center panel: search and the filtered list of quest cards.
#[component]
pub fn QuestList() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let mut show_modal = use_signal(|| false);
    let view = state.view.read().clone();
    let current = *state.current.read();
    let linked = state.playthroughs.read().iter().any(|p| Some(p.id) == current && p.link.is_some());
    let rows = state.rows.read();

    let cutoffs: HashMap<i64, (&str, QuestStatus)> =
        rows.iter().map(|(q, p)| (q.id, (q.title.as_str(), p.status()))).collect();
    let visible = view.apply(&rows);
    let visible_done = visible.iter().filter(|(_, p)| p.status() == QuestStatus::Completed).count();
    let visible_pct = if visible.is_empty() { 0.0 } else { visible_done as f64 * 100.0 / visible.len() as f64 };
    // Restart the list's entrance animation whenever the filters change.
    let list_key = format!("{:?}{:?}{:?}{:?}{:?}{current:?}", view.sources, view.quest_types, view.regions, view.statuses, view.sort);

    rsx! {
        main { class: "panel quest-pane",
            div { class: "panel-head",
                h2 { {t!(tr, "quests.journal")} }
                Divider {}
            }
            label { class: "search-box",
                Glyph { icon: Icon::Search, class: "search-glyph" }
                input {
                    r#type: "search",
                    placeholder: t!(tr, "quests.search"),
                    value: "{view.search}",
                    oninput: move |e| state.view.write().search = e.value(),
                }
            }
            div { class: "quest-scroll", key: "{list_key}",
                if current.is_none() {
                    div { class: "empty",
                        h3 { {t!(tr, "quests.no_playthrough")} }
                        p { {t!(tr, "quests.no_playthrough_hint")} }
                        button { class: "btn btn-gold", onclick: move |_| show_modal.set(true), {t!(tr, "header.new_playthrough")} }
                    }
                } else if rows.is_empty() {
                    div { class: "empty",
                        h3 { {t!(tr, "quests.no_quests")} }
                        p { {t!(tr, "quests.no_quests_hint")} }
                    }
                } else if visible.is_empty() {
                    div { class: "empty", p { {t!(tr, "quests.no_match")} } }
                } else {
                    for (i, (quest, progress)) in visible.iter().enumerate() {
                        {
                            let cutoff = quest.cutoff_quest_id.and_then(|id| cutoffs.get(&id));
                            rsx! {
                                QuestCard {
                                    key: "{current:?}-{quest.id}",
                                    index: i,
                                    quest: quest.clone(),
                                    progress: progress.clone(),
                                    cutoff_name: cutoff.map(|(name, _)| name.to_string()),
                                    cutoff_active: cutoff.is_some_and(|&(_, s)| s == QuestStatus::InProgress),
                                    save_synced: linked && quest.is_trackable(),
                                }
                            }
                        }
                    }
                }
            }
            if !visible.is_empty() {
                div { class: "list-footer",
                    {t!(tr, "quests.footer", shown = visible.len(), done = visible_done, pct = format!("{visible_pct:.1}"))}
                }
            }
        }
        if show_modal() {
            PlaythroughModal { on_close: move |_| show_modal.set(false) }
        }
    }
}
