use companion_db::{Expansion, RecipeKind};
use dioxus::prelude::*;

use super::WorkbenchUi;
use super::item_icon::ItemIcon;
use crate::components::ornament::{Divider, Glyph, Icon};
use crate::state::AppState;
use crate::t;
use crate::workbench::readiness;

/// Results shown at first and added by "Show more".
const PAGE: usize = 60;

const KINDS: [RecipeKind; 3] = [RecipeKind::Alchemy, RecipeKind::Crafting, RecipeKind::Enchanting];

/// Left panel: search and filters over every recipe of the playthrough's set. Each row opens
/// into the recipe's details.
#[component]
pub fn RecipeBrowser() -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let bench = state.bench.read().clone();
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let mut limit = use_signal(|| PAGE);
    let results = catalog.search(&bench, &plan.inventory);
    let categories = catalog.categories(bench.kind);
    let shown = results.len().min(limit());
    let has_inventory = !plan.inventory.is_empty();
    let list_key = format!("{:?}|{:?}|{:?}|{}", bench.search, bench.kind, bench.category, bench.craftable_only);

    rsx! {
        section { class: "panel browser",
            div { class: "panel-head",
                h2 { {t!(tr, "bench.recipes")} }
                Divider {}
            }
            label { class: "search-box",
                Glyph { icon: Icon::Search, class: "search-glyph" }
                input {
                    r#type: "search",
                    placeholder: t!(tr, "bench.search"),
                    value: "{bench.search}",
                    oninput: move |e| {
                        state.bench.write().search = e.value();
                        limit.set(PAGE);
                    },
                }
            }
            div { class: "filters",
                div { class: "chips",
                    button {
                        class: if bench.kind.is_none() { "chip is-active" } else { "chip" },
                        onclick: move |_| state.bench.write().kind = None,
                        {t!(tr, "common.all")}
                    }
                    for kind in KINDS {
                        button {
                            class: if bench.kind == Some(kind) { "chip is-active" } else { "chip" },
                            onclick: move |_| {
                                let mut b = state.bench.write();
                                b.kind = Some(kind);
                                b.category = None;
                            },
                            Glyph { icon: kind_icon(kind) }
                            span { {catalog.kind_label(kind)} }
                        }
                    }
                }
                select {
                    class: "select",
                    onchange: move |e| {
                        let value = e.value();
                        state.bench.write().category = (!value.is_empty()).then_some(value);
                    },
                    option { value: "", selected: bench.category.is_none(), {t!(tr, "bench.all_categories")} }
                    for c in categories.iter() {
                        option { value: "{c}", selected: bench.category.as_ref() == Some(c), {catalog.category_label(c)} }
                    }
                }
                label { class: if has_inventory { "toggle" } else { "toggle is-disabled" }, title: t!(tr, "bench.craftable_hint"),
                    input {
                        r#type: "checkbox",
                        checked: bench.craftable_only,
                        disabled: !has_inventory,
                        onchange: move |e| state.bench.write().craftable_only = e.checked(),
                    }
                    span { class: "toggle-track", span { class: "toggle-thumb" } }
                    span { {t!(tr, "bench.craftable")} }
                }
            }
            p { class: "result-count", {t!(tr, "bench.found", n = results.len())} }
            div { class: "recipe-list", key: "{list_key}",
                if results.is_empty() {
                    p { class: "empty-note muted", {t!(tr, "bench.no_results")} }
                }
                for (i, recipe) in results.iter().take(shown).enumerate() {
                    RecipeRow { key: "{recipe.product_id}", index: i % PAGE, product_id: recipe.product_id.clone() }
                }
            }
            if results.len() > shown {
                button { class: "btn btn-ghost more", onclick: move |_| limit += PAGE,
                    {t!(tr, "bench.show_more", n = results.len() - shown)}
                }
            }
        }
    }
}

pub fn kind_icon(kind: RecipeKind) -> Icon {
    match kind {
        RecipeKind::Alchemy => Icon::Flask,
        RecipeKind::Crafting => Icon::Hammer,
        RecipeKind::Enchanting => Icon::Rune,
    }
}

#[component]
pub fn ExpansionBadge(expansion: Option<Expansion>) -> Element {
    let state = use_context::<AppState>();
    let catalog = state.catalog.read();
    match expansion {
        Some(Expansion::HeartsOfStone) => rsx! {
            span { class: "badge badge-hos", title: catalog.label("dlc_hearts_of_stone", "Hearts of Stone"), "HoS" }
        },
        Some(Expansion::BloodAndWine) => rsx! {
            span { class: "badge badge-baw", title: catalog.label("dlc_blood_and_wine", "Blood and Wine"), "B&W" }
        },
        None => rsx! {},
    }
}

/// One recipe in the browser; click to open its details.
#[component]
fn RecipeRow(index: usize, product_id: String) -> Element {
    let state = use_context::<AppState>();
    let tr = *state.tr.read();
    let mut ui = use_context::<WorkbenchUi>();
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let Some(recipe) = catalog.recipe(&product_id) else { return rsx! {} };
    let open = ui.expanded.read().as_deref() == Some(product_id.as_str());
    let ready = readiness(recipe, &plan.inventory);
    let queued = plan.queue.iter().find(|e| e.item_id == product_id).map(|e| e.qty);
    let subtitle = match catalog.craftsman_label(recipe) {
        Some(craftsman) => format!("{} · {craftsman}", catalog.kind_label(recipe.kind)),
        None => catalog.kind_label(recipe.kind).to_string(),
    };
    let id = product_id.clone();

    rsx! {
        div { class: if open { "recipe-row is-open" } else { "recipe-row" }, style: "--i: {index.min(14)}",
            button {
                class: "recipe-row-head",
                onclick: move |_| {
                    let next = if open { None } else { Some(id.clone()) };
                    ui.expanded.set(next);
                },
                ItemIcon { item_id: product_id.clone(), size: 44 }
                span { class: "recipe-row-text",
                    span { class: "recipe-row-name", {catalog.name(&product_id)} }
                    span { class: "recipe-row-meta",
                        Glyph { icon: kind_icon(recipe.kind) }
                        "{subtitle}"
                    }
                }
                ExpansionBadge { expansion: recipe.expansion }
                if let Some(qty) = queued {
                    span { class: "badge badge-queued", title: t!(tr, "bench.in_plan_hint"), {t!(tr, "bench.in_plan", n = qty)} }
                }
                if !plan.inventory.is_empty() {
                    if ready.is_ready() {
                        span { class: "ready ready-yes", title: t!(tr, "bench.ready_hint"), Glyph { icon: Icon::Check } }
                    } else {
                        span { class: "ready num", title: t!(tr, "bench.partial_hint"), "{ready.covered}/{ready.total}" }
                    }
                }
                Glyph { icon: Icon::Chevron, class: "recipe-row-chevron" }
            }
            if open {
                RecipeDetail { product_id: product_id.clone() }
            }
        }
    }
}

/// A recipe's details: ingredients with what the inventory holds, alternatives, and adding it to
/// the plan.
#[component]
fn RecipeDetail(product_id: String) -> Element {
    let mut state = use_context::<AppState>();
    let tr = *state.tr.read();
    let ui = use_context::<WorkbenchUi>();
    let catalog = state.catalog.read();
    let plan = state.plan.read();
    let mut variant = use_signal(|| plan.queue.iter().find(|e| e.item_id == product_id).map_or(0, |e| e.variant));
    let mut qty = use_signal(|| 1u32);
    let Some(recipe) = catalog.recipe(&product_id).cloned() else { return rsx! {} };
    let has_playthrough = state.current.read().is_some();
    let quality = catalog.items.get(&product_id).map_or(1, |i| i.quality);
    let variants = recipe.variants.len();
    let chosen = variant().min(variants.saturating_sub(1));

    rsx! {
        div { class: "recipe-detail",
            div { class: "recipe-detail-meta",
                span { class: "tier-text tier-q{quality}", {catalog.quality_label(quality)} }
                if let Some(category) = catalog.items.get(&product_id).map(|i| i.category.clone()) {
                    span { {catalog.category_label(&category)} }
                }
                if recipe.yield_qty > 1 {
                    span { class: "num", {t!(tr, "bench.yields", n = recipe.yield_qty)} }
                }
            }
            if variants > 1 {
                div { class: "chips variant-chips",
                    for v in 0..variants {
                        button {
                            class: if v == chosen { "chip is-active" } else { "chip" },
                            onclick: move |_| variant.set(v),
                            {t!(tr, "bench.variant", i = v + 1, n = variants)}
                        }
                    }
                }
            }
            div { class: "ingredient-label", {catalog.label("panel_crafting_ingredients_start", t!(tr, "bench.ingredients"))} }
            ul { class: "ingredients",
                for ing in recipe.ingredients(chosen).iter().cloned() {
                    {
                        let have = plan.have(&ing.item_id);
                        let craftable = catalog.recipe(&ing.item_id).is_some();
                        let class = if plan.inventory.is_empty() { "ingredient" } else if have >= ing.qty { "ingredient is-stocked" } else { "ingredient is-short" };
                        let target = ing.item_id.clone();
                        rsx! {
                            li { key: "{ing.item_id}", class,
                                span { class: "ingredient-qty num", "{ing.qty}×" }
                                ItemIcon { item_id: ing.item_id.clone(), size: 30 }
                                if craftable {
                                    button { class: "ingredient-name is-link", title: t!(tr, "bench.open_recipe"),
                                        onclick: move |_| ui.reveal(state, &target),
                                        {catalog.name(&ing.item_id)}
                                        Glyph { icon: Icon::Hammer, class: "inline-glyph" }
                                    }
                                } else {
                                    span { class: "ingredient-name", {catalog.name(&ing.item_id)} }
                                }
                                if !plan.inventory.is_empty() {
                                    span { class: "ingredient-have num", title: t!(tr, "bench.in_bag"),
                                        Glyph { icon: Icon::Bag }
                                        "{have}"
                                    }
                                }
                            }
                        }
                    }
                }
            }
            div { class: "recipe-detail-actions",
                div { class: "stepper",
                    button { class: "stepper-btn", disabled: qty() <= 1, title: t!(tr, "common.decrease"), onclick: move |_| qty -= 1,
                        Glyph { icon: Icon::Minus }
                    }
                    span { class: "stepper-value num", key: "{qty}", "{qty}" }
                    button { class: "stepper-btn", title: t!(tr, "common.increase"), onclick: move |_| qty += 1,
                        Glyph { icon: Icon::Plus }
                    }
                }
                button {
                    class: "btn btn-seal",
                    disabled: !has_playthrough,
                    title: if has_playthrough { t!(tr, "bench.add_hint") } else { t!(tr, "bench.needs_playthrough") },
                    onclick: move |_| {
                        let id = recipe.product_id.clone();
                        state.plan_add(&id, qty());
                        if chosen > 0 {
                            state.plan_set_variant(&id, chosen);
                        }
                        qty.set(1);
                    },
                    Glyph { icon: Icon::Plus }
                    span { {t!(tr, "bench.add")} }
                }
            }
        }
    }
}
