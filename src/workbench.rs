//! Recipe planner data for the UI: the item and recipe catalog in the current language, the
//! current playthrough's plan, search, and filter options.

use std::collections::{HashMap, HashSet};

use companion_db::needs::{Need, compute_needs};
use companion_db::{Item, QueueEntry, Recipe, RecipeKind};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;
use w3_formats::items::{CRAFTSMAN_LEVEL_KEYS, CRAFTSMAN_TYPE_KEYS, ENCHANT_LEVEL_KEYS, GAME_TEXT_KEYS, QUALITY_KEYS, category_key};

/// The recipe browser's filters, persisted with the settings (except the search text).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct WorkbenchOptions {
    pub kind: Option<RecipeKind>,
    pub category: Option<String>,
    /// Only recipes whose ingredients the inventory covers.
    pub craftable_only: bool,
    #[serde(skip)]
    pub search: String,
}

/// Items and one recipe set, with names in the UI language.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Catalog {
    pub items: HashMap<String, Item>,
    /// Sorted by product name in the UI language.
    pub recipes: Vec<Recipe>,
    /// Product id -> index in `recipes`.
    pub by_product: HashMap<String, usize>,
    /// Game text for UI labels, by localisation key, in the UI language.
    pub labels: HashMap<String, String>,
    /// Product id -> folded names (UI language and English) for search.
    search_names: HashMap<String, (String, String)>,
}

impl Catalog {
    pub fn new(items: Vec<Item>, mut recipes: Vec<Recipe>, labels: HashMap<String, String>) -> Self {
        let items: HashMap<String, Item> = items.into_iter().map(|i| (i.id.clone(), i)).collect();
        let name = |id: &str| items.get(id).map_or_else(|| id.to_string(), |i| i.name.clone());
        recipes.sort_by_cached_key(|r| sort_key(&name(&r.product_id)));
        let by_product = recipes.iter().enumerate().map(|(i, r)| (r.product_id.clone(), i)).collect();
        let search_names = recipes
            .iter()
            .map(|r| {
                let item = items.get(&r.product_id);
                let local = fold(&name(&r.product_id));
                let english = fold(item.map_or("", |i| i.english_name.as_str()));
                (r.product_id.clone(), (local, english))
            })
            .collect();
        Self { items, recipes, by_product, labels, search_names }
    }

    pub fn recipe(&self, product_id: &str) -> Option<&Recipe> {
        self.by_product.get(product_id).map(|&i| &self.recipes[i])
    }

    pub fn recipe_map(&self) -> HashMap<String, Recipe> {
        self.recipes.iter().map(|r| (r.product_id.clone(), r.clone())).collect()
    }

    /// An item's name, or its id for an item the catalog lacks.
    pub fn name(&self, id: &str) -> String {
        self.items.get(id).map_or_else(|| id.to_string(), |i| i.name.clone())
    }

    /// Game text for a localisation key, or `fallback`.
    pub fn label<'a>(&'a self, key: &str, fallback: &'a str) -> &'a str {
        self.labels.get(key).map_or(fallback, String::as_str)
    }

    pub fn kind_label(&self, kind: RecipeKind) -> &str {
        let (key, fallback) = match kind {
            RecipeKind::Alchemy => ("panel_title_alchemy", "Alchemy"),
            RecipeKind::Crafting => ("panel_title_crafting", "Crafting"),
            RecipeKind::Enchanting => ("panel_title_enchanting", "Enchanting"),
        };
        self.label(key, fallback)
    }

    pub fn category_label<'a>(&'a self, category: &'a str) -> &'a str {
        self.labels.get(&category_key(category)).map_or(category, String::as_str)
    }

    pub fn quality_label(&self, quality: u8) -> &str {
        QUALITY_KEYS.get(quality.saturating_sub(1) as usize).map_or("", |key| self.label(key, ""))
    }

    /// Craftsman and level of a recipe, e.g. "Armorer · Master", or the enchanting level.
    pub fn craftsman_label(&self, recipe: &Recipe) -> Option<String> {
        let lookup = |table: &[(&str, &str)], value: &str| {
            table.iter().find(|(k, _)| *k == value).map(|(_, key)| self.label(key, value).to_string())
        };
        match recipe.kind {
            RecipeKind::Crafting => {
                let parts: Vec<String> = [
                    recipe.craftsman.as_deref().and_then(|c| lookup(CRAFTSMAN_TYPE_KEYS, c)),
                    recipe.level.as_deref().and_then(|l| lookup(CRAFTSMAN_LEVEL_KEYS, l)),
                ]
                .into_iter()
                .flatten()
                .collect();
                (!parts.is_empty()).then(|| parts.join(" · "))
            }
            RecipeKind::Enchanting => recipe.level.as_deref().and_then(|l| lookup(ENCHANT_LEVEL_KEYS, l)),
            RecipeKind::Alchemy => None,
        }
    }

    /// Product categories that have recipes of `kind` (or any kind), sorted by label.
    pub fn categories(&self, kind: Option<RecipeKind>) -> Vec<String> {
        let mut found: Vec<String> = self
            .recipes
            .iter()
            .filter(|r| kind.is_none_or(|k| r.kind == k))
            .filter_map(|r| self.items.get(&r.product_id).map(|i| i.category.clone()))
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        found.sort_by_cached_key(|c| fold(self.category_label(c)));
        found
    }

    /// Recipes passing `options`, best search matches first (exact, then prefix, then contains;
    /// in either the UI language or English), otherwise in name order.
    pub fn search(&self, options: &WorkbenchOptions, inventory: &HashMap<String, u32>) -> Vec<&Recipe> {
        let query = fold(options.search.trim());
        let mut hits: Vec<(u8, &Recipe)> = self
            .recipes
            .iter()
            .filter(|r| options.kind.is_none_or(|k| r.kind == k))
            .filter(|r| {
                options.category.as_ref().is_none_or(|c| self.items.get(&r.product_id).is_some_and(|i| &i.category == c))
            })
            .filter(|r| !options.craftable_only || readiness(r, inventory).is_ready())
            .filter_map(|r| {
                if query.is_empty() {
                    return Some((0, r));
                }
                let (local, english) = self.search_names.get(&r.product_id)?;
                let rank = |name: &str| {
                    if name == query {
                        Some(0)
                    } else if name.starts_with(&query) {
                        Some(1)
                    } else if name.contains(&query) {
                        Some(2)
                    } else {
                        None
                    }
                };
                let best = [rank(local), rank(english)].into_iter().flatten().min()?;
                Some((best, r))
            })
            .collect();
        hits.sort_by_key(|(rank, _)| *rank);
        hits.into_iter().map(|(_, r)| r).collect()
    }
}

/// Lowercase, accents removed, for search and sorting. Letters with a stroke (which Unicode does
/// not decompose) lose it too, so "jaskolka" finds "Jaskółka".
pub fn fold(s: &str) -> String {
    s.nfkd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .map(|c| match c {
            'ł' => 'l',
            'ø' => 'o',
            'đ' => 'd',
            'ı' => 'i',
            c => c,
        })
        .collect()
}

/// Folded name without leading punctuation, so "(Name)" sorts under N.
fn sort_key(name: &str) -> String {
    fold(name.trim_start_matches(|c: char| !c.is_alphanumeric()))
}

/// How much of a recipe's primary ingredient set the inventory covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Readiness {
    pub covered: usize,
    pub total: usize,
}

impl Readiness {
    pub fn is_ready(self) -> bool {
        self.total > 0 && self.covered == self.total
    }
}

pub fn readiness(recipe: &Recipe, inventory: &HashMap<String, u32>) -> Readiness {
    let ingredients = recipe.ingredients(0);
    let covered = ingredients.iter().filter(|i| inventory.get(&i.item_id).copied().unwrap_or(0) >= i.qty).count();
    Readiness { covered, total: ingredients.len() }
}

/// Every localisation key the workbench shows as a label.
pub fn label_keys(categories: impl Iterator<Item = String>) -> Vec<String> {
    GAME_TEXT_KEYS
        .iter()
        .chain(CRAFTSMAN_TYPE_KEYS)
        .chain(CRAFTSMAN_LEVEL_KEYS)
        .chain(ENCHANT_LEVEL_KEYS)
        .map(|(_, key)| key.to_string())
        .chain(QUALITY_KEYS.iter().map(|k| k.to_string()))
        .chain(categories.map(|c| category_key(&c)))
        .collect()
}

/// A playthrough's plan with what the inventory holds.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    pub queue: Vec<QueueEntry>,
    pub collected: HashSet<String>,
    pub craft_self: HashSet<String>,
    pub inventory: HashMap<String, u32>,
}

impl Plan {
    pub fn have(&self, id: &str) -> u32 {
        self.inventory.get(id).copied().unwrap_or(0)
    }

    /// The plan's needs; collected items count as fully stocked.
    pub fn needs(&self, recipes: &HashMap<String, Recipe>) -> Vec<Need> {
        let have = |id: &str| if self.collected.contains(id) { u32::MAX } else { self.have(id) };
        compute_needs(&self.queue, recipes, &self.craft_self, &have)
    }

    /// Whether a need is met: collected by hand, crafted myself or carried in the inventory.
    pub fn is_met(&self, need: &Need) -> bool {
        self.collected.contains(&need.item_id)
            || need.crafts == Some(0)
            || (need.crafts.is_none() && self.have(&need.item_id) >= need.needed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use companion_db::Ingredient;

    fn item(id: &str, name: &str, english: &str, category: &str) -> Item {
        Item {
            id: id.into(),
            internal_name: id.into(),
            name: name.into(),
            english_name: english.into(),
            category: category.into(),
            quality: 1,
            expansion: None,
            has_image: false,
        }
    }

    fn recipe(id: &str, kind: RecipeKind, ings: &[(&str, u32)]) -> Recipe {
        Recipe {
            product_id: id.into(),
            kind,
            level: None,
            craftsman: None,
            yield_qty: 1,
            expansion: None,
            variants: vec![ings.iter().map(|(i, q)| Ingredient { item_id: i.to_string(), qty: *q }).collect()],
        }
    }

    fn catalog() -> Catalog {
        Catalog::new(
            vec![
                item("swallow", "Jaskółka", "Swallow", "potion"),
                item("swallow-2", "Wzmocniona Jaskółka", "Enhanced Swallow", "potion"),
                item("sword", "Miecz", "Sword", "steelsword"),
                item("celandine", "Glistnik", "Celandine", "alchemy_ingredient"),
            ],
            vec![
                recipe("swallow-2", RecipeKind::Alchemy, &[("swallow", 1), ("celandine", 3)]),
                recipe("swallow", RecipeKind::Alchemy, &[("celandine", 2)]),
                recipe("sword", RecipeKind::Crafting, &[("celandine", 9)]),
            ],
            HashMap::new(),
        )
    }

    fn ids(recipes: Vec<&Recipe>) -> Vec<&str> {
        recipes.into_iter().map(|r| r.product_id.as_str()).collect()
    }

    #[test]
    fn searches_both_languages_and_ranks_matches() {
        let c = catalog();
        let mut options = WorkbenchOptions::default();
        assert_eq!(ids(c.search(&options, &HashMap::new())), vec!["swallow", "sword", "swallow-2"]);
        options.search = "jaskolka".into();
        assert_eq!(ids(c.search(&options, &HashMap::new())), vec!["swallow", "swallow-2"]);
        options.search = "enhanced".into();
        assert_eq!(ids(c.search(&options, &HashMap::new())), vec!["swallow-2"]);
        options.search = String::new();
        options.kind = Some(RecipeKind::Crafting);
        assert_eq!(ids(c.search(&options, &HashMap::new())), vec!["sword"]);
        options.kind = None;
        options.category = Some("potion".into());
        options.craftable_only = true;
        let inventory = HashMap::from([("celandine".to_string(), 3)]);
        assert_eq!(ids(c.search(&options, &inventory)), vec!["swallow"]);
        assert_eq!(c.categories(Some(RecipeKind::Alchemy)), vec!["potion"]);
    }

    #[test]
    fn readiness_counts_covered_ingredients() {
        let c = catalog();
        let r = c.recipe("swallow-2").unwrap();
        let inv = HashMap::from([("celandine".to_string(), 3)]);
        assert_eq!(readiness(r, &inv), Readiness { covered: 1, total: 2 });
        assert!(!readiness(r, &inv).is_ready());
    }

    #[test]
    fn collected_items_are_met() {
        let c = catalog();
        let plan = Plan {
            queue: vec![QueueEntry { item_id: "swallow-2".into(), qty: 1, variant: 0 }],
            collected: HashSet::from(["swallow".to_string()]),
            craft_self: HashSet::new(),
            inventory: HashMap::from([("celandine".to_string(), 1)]),
        };
        let needs = plan.needs(&c.recipe_map());
        let met: Vec<(String, bool)> = needs.iter().map(|n| (n.item_id.clone(), plan.is_met(n))).collect();
        assert_eq!(met, vec![("swallow".to_string(), true), ("celandine".to_string(), false)]);
    }
}
