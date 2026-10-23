mod cutoff_pane;
mod quest_card;
mod quest_list;
mod sidebar;

use dioxus::prelude::*;

use cutoff_pane::CutoffPane;
use quest_list::QuestList;
use sidebar::Sidebar;

/// The quest tracker: filters and progress, the quest list, and cutoff warnings.
#[component]
pub fn QuestsView() -> Element {
    rsx! {
        div { class: "view quests-view",
            Sidebar {}
            QuestList {}
            CutoffPane {}
        }
    }
}
