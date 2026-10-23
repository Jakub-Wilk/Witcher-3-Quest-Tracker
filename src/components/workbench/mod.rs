mod browser;
mod item_icon;
mod plan;

use dioxus::prelude::*;

use crate::state::AppState;
use crate::t;
use browser::RecipeBrowser;
use plan::PlanPanel;

/// Workbench-wide UI state shared by the browser and the plan.
#[derive(Clone, Copy)]
pub struct WorkbenchUi {
    /// The recipe whose details are open in the browser.
    pub expanded: Signal<Option<String>>,
}

impl WorkbenchUi {
    /// Shows a recipe in the browser: clears the filters, searches for it and opens it.
    pub fn reveal(mut self, mut state: AppState, product_id: &str) {
        let name = state.catalog.peek().name(product_id);
        let mut bench = state.bench.write();
        bench.search = name;
        bench.kind = None;
        bench.category = None;
        bench.craftable_only = false;
        drop(bench);
        self.expanded.set(Some(product_id.to_string()));
    }
}

/// The recipe planner: browse recipes on the left, the plan and what it still needs on the right.
#[component]
pub fn WorkbenchView() -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    use_context_provider(|| WorkbenchUi { expanded: Signal::new(None) });
    let has_items = *state.item_count.read() > 0;

    rsx! {
        div { class: "view workbench-view",
            if has_items {
                RecipeBrowser {}
                PlanPanel {}
            } else {
                div { class: "panel empty-panel",
                    h3 { {t!(tr, "bench.no_items")} }
                    p { {t!(tr, "bench.no_items_hint")} }
                }
            }
        }
    }
}
