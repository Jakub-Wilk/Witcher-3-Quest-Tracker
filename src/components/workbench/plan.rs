use std::time::Duration;

use companion_db::needs::{Need, demand_for};
use dioxus::prelude::*;

use super::WorkbenchUi;
use super::item_icon::ItemIcon;
use crate::components::format_local;
use crate::components::ornament::{Divider, Glyph, Icon};
use crate::state::AppState;
use crate::t;

/// How long an element takes to animate out before it is removed.
const LEAVE: Duration = Duration::from_millis(220);

/// Right panel, read top to bottom: what I'm crafting, then what that still needs.
#[component]
pub fn PlanPanel() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let has_playthrough = state.current.read().is_some();
    let recipes = catalog.recipe_map();
    let needs = plan.needs(&recipes);
    let crafts: u32 = plan.queue.iter().map(|e| e.qty).sum();

    rsx! {
        aside { class: "panel plan",
            div { class: "panel-head",
                h2 { {t!(tr, "plan.title")} }
                Divider {}
            }
            if !has_playthrough {
                div { class: "empty",
                    h3 { {t!(tr, "plan.no_playthrough")} }
                    p { {t!(tr, "plan.no_playthrough_hint")} }
                }
            } else if plan.queue.is_empty() {
                div { class: "empty plan-empty",
                    Glyph { icon: Icon::Hammer, class: "empty-glyph" }
                    h3 { {t!(tr, "plan.empty")} }
                    p { {t!(tr, "plan.empty_hint")} }
                }
            } else {
                div { class: "plan-toolbar",
                    span { {t!(tr, "plan.summary", recipes = plan.queue.len(), crafts = crafts)} }
                    button { class: "btn btn-ghost danger small", onclick: move |_| state.plan_clear(), {t!(tr, "plan.clear")} }
                }
                ul { class: "queue",
                    for entry in plan.queue.iter() {
                        QueueItem { key: "{entry.item_id}", item_id: entry.item_id.clone(), qty: entry.qty, variant: entry.variant }
                    }
                }
                Needs { needs }
            }
            InventorySource {}
        }
    }
}

/// One planned craft: stepper, ingredient-set switch and remove.
#[component]
fn QueueItem(item_id: String, qty: u32, variant: usize) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let ui = use_context::<WorkbenchUi>();
    let mut leaving = use_signal(|| false);
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let variants = catalog.recipe(&item_id).map_or(1, |r| r.variants.len());
    let used = demand_for(&item_id, &plan.queue, &catalog.recipe_map());
    let (id_dec, id_inc, id_var, id_rm, id_open) =
        (item_id.clone(), item_id.clone(), item_id.clone(), item_id.clone(), item_id.clone());

    rsx! {
        li { class: if leaving() { "queue-item is-leaving" } else { "queue-item" },
            ItemIcon { item_id: item_id.clone(), size: 40 }
            div { class: "queue-text",
                button { class: "queue-name", title: t!(tr, "bench.open_recipe"), onclick: move |_| ui.reveal(state, &id_open),
                    {catalog.name(&item_id)}
                }
                div { class: "queue-meta",
                    if variants > 1 {
                        button {
                            class: "variant-switch",
                            title: t!(tr, "plan.variant_hint"),
                            onclick: move |_| state.plan_set_variant(&id_var, (variant + 1) % variants),
                            {t!(tr, "bench.variant", i = variant + 1, n = variants)}
                        }
                    }
                    if used > 0 {
                        span { class: "muted", {t!(tr, "plan.used_as_ingredient", n = used)} }
                    }
                }
            }
            div { class: "stepper",
                button { class: "stepper-btn", title: t!(tr, "common.decrease"), disabled: qty <= 1,
                    onclick: move |_| state.plan_set_qty(&id_dec, qty - 1),
                    Glyph { icon: Icon::Minus }
                }
                span { class: "stepper-value num", key: "{qty}", "{qty}" }
                button { class: "stepper-btn", title: t!(tr, "common.increase"), onclick: move |_| state.plan_set_qty(&id_inc, qty + 1),
                    Glyph { icon: Icon::Plus }
                }
            }
            button {
                class: "btn-x",
                title: t!(tr, "plan.remove"),
                onclick: move |_| {
                    leaving.set(true);
                    let id = id_rm.clone();
                    spawn(async move {
                        tokio::time::sleep(LEAVE).await;
                        state.plan_set_qty(&id, 0);
                    });
                },
                Glyph { icon: Icon::Close }
            }
        }
    }
}

/// What the plan still takes: gathered materials and the ingredients crafted along the way.
#[component]
fn Needs(needs: Vec<Need>) -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    let catalog = state.catalog.read();
    let plan = state.plan.read();

    let (mut crafted, mut gathered): (Vec<&Need>, Vec<&Need>) = needs.iter().partition(|n| n.crafts.is_some());
    // Most missing first; done ones last.
    let missing = |n: &Need| if plan.is_met(n) { 0 } else { n.needed.saturating_sub(plan.have(&n.item_id)).max(1) };
    gathered.sort_by_key(|n| (std::cmp::Reverse(missing(n)), catalog.name(&n.item_id)));
    crafted.sort_by_key(|n| catalog.name(&n.item_id));

    let kinds = gathered.len();
    let done = gathered.iter().filter(|n| plan.is_met(n)).count();
    let pieces: u32 = gathered.iter().map(|n| n.needed).sum();
    let have_pieces: u32 = gathered
        .iter()
        .map(|n| if plan.is_met(n) { n.needed } else { plan.have(&n.item_id).min(n.needed) })
        .sum();
    let progress = if pieces == 0 { 1.0 } else { have_pieces as f64 / pieces as f64 };

    rsx! {
        div { class: "needs",
            h3 { class: "section-title", {t!(tr, "plan.needed")} }
            div { class: "progress-bar",
                div { class: "progress-fill", style: "transform: scaleX({progress:.4})" }
            }
            p { class: "needs-stats",
                {t!(tr, "plan.stats", kinds = kinds, pieces = pieces, done = done)}
            }
            if !crafted.is_empty() {
                h4 { class: "needs-group", Glyph { icon: Icon::Hammer } {t!(tr, "plan.crafting_myself")} }
                ul { class: "need-list",
                    for (i, n) in crafted.iter().enumerate() {
                        NeedRow { key: "craft-{n.item_id}", index: i, need: (*n).clone() }
                    }
                }
            }
            h4 { class: "needs-group", Glyph { icon: Icon::Bag } {t!(tr, "plan.to_gather")} }
            if gathered.is_empty() {
                p { class: "muted empty-note", {t!(tr, "plan.nothing_to_gather")} }
            }
            ul { class: "need-list",
                for (i, n) in gathered.iter().enumerate() {
                    NeedRow { key: "gather-{n.item_id}", index: i, need: (*n).clone() }
                }
            }
        }
    }
}

#[component]
fn NeedRow(index: usize, need: Need) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let ui = use_context::<WorkbenchUi>();
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let id = need.item_id.clone();
    let have = plan.have(&id);
    let collected = plan.collected.contains(&id);
    let met = plan.is_met(&need);
    let recipe = catalog.recipe(&id).cloned();
    let craft_self = need.crafts.is_some();
    let has_inventory = !plan.inventory.is_empty();
    let class = match (collected, met, has_inventory) {
        (true, _, _) => "need is-collected",
        (false, true, _) => "need is-stocked",
        (false, false, true) => "need is-short",
        _ => "need",
    };
    let (id_toggle, id_craft, id_open) = (id.clone(), id.clone(), id.clone());

    rsx! {
        li { class, style: "--i: {index.min(14)}",
            ItemIcon { item_id: id.clone(), size: 36 }
            div { class: "need-text",
                if recipe.is_some() {
                    button { class: "need-name is-link", title: t!(tr, "bench.open_recipe"), onclick: move |_| ui.reveal(state, &id_open),
                        {catalog.name(&id)}
                    }
                } else {
                    span { class: "need-name", {catalog.name(&id)} }
                }
                span { class: "need-count num",
                    if has_inventory {
                        span { class: "need-have", key: "{have}", "{have.min(99_999)}" }
                        " / "
                    }
                    span { key: "{need.needed}", class: "need-needed", "{need.needed}" }
                }
                if let (Some(crafts), Some(recipe)) = (need.crafts, recipe.as_ref()) {
                    if crafts > 0 {
                        span { class: "need-from",
                            {t!(tr, "plan.crafts", n = crafts)}
                            " · "
                            {recipe.ingredients(0).iter().map(|i| format!("{}× {}", i.qty * crafts, catalog.name(&i.item_id))).collect::<Vec<_>>().join(", ")}
                        }
                    }
                }
            }
            div { class: "need-actions",
                if recipe.is_some() {
                    button {
                        class: if craft_self { "need-toggle craft is-on" } else { "need-toggle craft" },
                        title: if craft_self { t!(tr, "plan.gather_instead") } else { t!(tr, "plan.craft_instead") },
                        onclick: move |_| state.plan_set_craft_self(&id_craft, !craft_self),
                        Glyph { icon: Icon::Hammer }
                    }
                }
                button {
                    class: if collected { "need-toggle collect is-on" } else { "need-toggle collect" },
                    title: if collected { t!(tr, "plan.unmark_collected") } else { t!(tr, "plan.mark_collected") },
                    onclick: move |_| state.plan_set_collected(&id_toggle, !collected),
                    Glyph { icon: Icon::Check }
                }
            }
            if collected {
                span { class: "need-overlay" }
            }
        }
    }
}

/// Where the inventory comes from: the save it was read from, or how to get one.
#[component]
fn InventorySource() -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    let current = *state.current.read();
    let playthroughs = state.playthroughs.read();
    let Some(p) = playthroughs.iter().find(|p| Some(p.id) == current) else { return rsx! {} };
    let text = match (&p.link, &p.head, p.inventory_save_key) {
        (None, _, _) => t!(tr, "plan.inventory_unlinked").to_string(),
        (Some(_), Some(head), Some(key)) if head.key == key => {
            t!(tr, "plan.inventory_from", file = head.file, time = format_local(tr, head.saved_at))
        }
        (Some(_), Some(head), Some(_)) => t!(tr, "plan.inventory_stale", file = head.file),
        (Some(_), _, None) => t!(tr, "plan.inventory_none").to_string(),
        (Some(_), None, Some(_)) => t!(tr, "plan.inventory_none").to_string(),
    };
    rsx! {
        footer { class: "inventory-source",
            Glyph { icon: Icon::Bag }
            span { "{text}" }
        }
    }
}
