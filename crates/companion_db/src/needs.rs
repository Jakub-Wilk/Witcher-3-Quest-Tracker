//! What a crafting plan takes: the ingredients of every planned craft, less what the plan itself
//! crafts, with craftable ingredients the user chose to craft expanded into their own
//! ingredients.

use std::collections::{HashMap, HashSet};

use crate::models::{QueueEntry, Recipe};

/// One item the plan consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    pub item_id: String,
    /// Total amount the plan consumes.
    pub demand: u32,
    /// How many units the queue itself crafts.
    pub queued: u32,
    /// What is left to gather or craft: `demand - queued`, always above zero.
    pub needed: u32,
    /// For an item crafted "myself": how many crafts cover what the inventory lacks. Its
    /// ingredients are part of the other needs. `None` for an item to gather.
    pub crafts: Option<u32>,
}

/// Items in this many nested craft-it-myself levels at most; deeper (cyclic) recipes stop there.
const MAX_DEPTH: usize = 32;

/// The needs of `queue`. `craft_self` items are crafted from their primary ingredients instead
/// of being gathered, as many times as `have` (what the user carries) falls short.
pub fn compute_needs(
    queue: &[QueueEntry],
    recipes: &HashMap<String, Recipe>,
    craft_self: &HashSet<String>,
    have: &dyn Fn(&str) -> u32,
) -> Vec<Need> {
    // Item order: first appearance, so the list is stable as quantities change.
    let mut order: Vec<String> = Vec::new();
    let mut add = |demand: &mut HashMap<String, u32>, id: &str, qty: u32| {
        let d = demand.entry(id.to_string()).or_insert_with(|| {
            order.push(id.to_string());
            0
        });
        *d += qty;
    };

    let mut base: HashMap<String, u32> = HashMap::new();
    for entry in queue {
        let Some(recipe) = recipes.get(&entry.item_id) else { continue };
        for ing in recipe.ingredients(entry.variant) {
            add(&mut base, &ing.item_id, ing.qty * entry.qty);
        }
    }
    let queued: HashMap<&str, u32> = queue
        .iter()
        .map(|e| (e.item_id.as_str(), e.qty * recipes.get(&e.item_id).map_or(1, |r| r.yield_qty.max(1))))
        .collect();
    let queued_of = |id: &str| queued.get(id).copied().unwrap_or(0);

    // Crafting an item adds to the demand of its ingredients, which may be crafted in turn:
    // iterate until the craft counts settle.
    let mut crafts: HashMap<String, u32> = HashMap::new();
    let mut demand = base.clone();
    for _ in 0..MAX_DEPTH {
        demand = base.clone();
        let mut ids: Vec<&String> = crafts.keys().collect();
        ids.sort();
        for id in ids {
            let recipe = &recipes[id];
            for ing in recipe.ingredients(0) {
                add(&mut demand, &ing.item_id, ing.qty * crafts[id]);
            }
        }
        let next: HashMap<String, u32> = demand
            .iter()
            .filter(|(id, _)| craft_self.contains(*id))
            .filter_map(|(id, &d)| {
                let recipe = recipes.get(id)?;
                let short = d.saturating_sub(queued_of(id)).saturating_sub(have(id));
                let n = short.div_ceil(recipe.yield_qty.max(1));
                (n > 0).then(|| (id.clone(), n))
            })
            .collect();
        if next == crafts {
            break;
        }
        crafts = next;
    }

    order
        .into_iter()
        .filter_map(|id| {
            let d = *demand.get(&id)?;
            let q = queued_of(&id);
            (d > q).then(|| Need {
                crafts: (craft_self.contains(&id) && recipes.contains_key(&id))
                    .then(|| crafts.get(&id).copied().unwrap_or(0)),
                item_id: id,
                demand: d,
                queued: q,
                needed: d - q,
            })
        })
        .collect()
}

/// How many of `id` the rest of the queue consumes (0 for an end product).
pub fn demand_for(id: &str, queue: &[QueueEntry], recipes: &HashMap<String, Recipe>) -> u32 {
    queue
        .iter()
        .filter_map(|e| Some((e, recipes.get(&e.item_id)?)))
        .flat_map(|(e, r)| r.ingredients(e.variant).iter().filter(|i| i.item_id == id).map(move |i| i.qty * e.qty))
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Ingredient, RecipeKind};

    fn recipe(id: &str, yield_qty: u32, variants: &[&[(&str, u32)]]) -> (String, Recipe) {
        let variants = variants
            .iter()
            .map(|v| v.iter().map(|(i, q)| Ingredient { item_id: i.to_string(), qty: *q }).collect())
            .collect();
        let r = Recipe {
            product_id: id.into(),
            kind: RecipeKind::Alchemy,
            level: None,
            craftsman: None,
            yield_qty,
            expansion: None,
            variants,
        };
        (id.to_string(), r)
    }

    fn swallows() -> HashMap<String, Recipe> {
        HashMap::from([
            recipe("enhanced-swallow", 1, &[&[("alcohest", 1), ("swallow", 1), ("celandine", 6)]]),
            recipe("swallow", 1, &[&[("dwarven-spirit", 1), ("celandine", 2)]]),
        ])
    }

    fn q(entries: &[(&str, u32)]) -> Vec<QueueEntry> {
        entries.iter().map(|(id, qty)| QueueEntry { item_id: id.to_string(), qty: *qty, variant: 0 }).collect()
    }

    fn needed(queue: &[QueueEntry], recipes: &HashMap<String, Recipe>) -> HashMap<String, u32> {
        needed_with(queue, recipes, &[], &[])
    }

    fn needed_with(
        queue: &[QueueEntry],
        recipes: &HashMap<String, Recipe>,
        craft_self: &[&str],
        have: &[(&str, u32)],
    ) -> HashMap<String, u32> {
        let craft_self = craft_self.iter().map(|s| s.to_string()).collect();
        let have: HashMap<&str, u32> = have.iter().copied().collect();
        compute_needs(queue, recipes, &craft_self, &|id| have.get(id).copied().unwrap_or(0))
            .into_iter()
            .map(|n| (n.item_id, n.needed))
            .collect()
    }

    fn map(entries: &[(&str, u32)]) -> HashMap<String, u32> {
        entries.iter().map(|(k, v)| (k.to_string(), *v)).collect()
    }

    #[test]
    fn lists_immediate_ingredients_of_queued_recipes() {
        assert_eq!(
            needed(&q(&[("enhanced-swallow", 1)]), &swallows()),
            map(&[("alcohest", 1), ("swallow", 1), ("celandine", 6)])
        );
        assert_eq!(
            needed(&q(&[("enhanced-swallow", 2)]), &swallows()),
            map(&[("alcohest", 2), ("swallow", 2), ("celandine", 12)])
        );
    }

    #[test]
    fn queueing_a_craftable_ingredient_replaces_it_with_its_own_ingredients() {
        assert_eq!(
            needed(&q(&[("enhanced-swallow", 1), ("swallow", 1)]), &swallows()),
            map(&[("alcohest", 1), ("celandine", 8), ("dwarven-spirit", 1)])
        );
        let partial = needed(&q(&[("enhanced-swallow", 3), ("swallow", 1)]), &swallows());
        assert_eq!(partial["swallow"], 2);
        assert_eq!(partial["celandine"], 20);
    }

    #[test]
    fn uses_the_selected_alternative_ingredient_set() {
        let recipes = HashMap::from([recipe("mutagen", 1, &[&[("a", 3)], &[("b", 1), ("c", 2)]])]);
        let mut queue = q(&[("mutagen", 1)]);
        assert_eq!(needed(&queue, &recipes), map(&[("a", 3)]));
        queue[0].variant = 1;
        assert_eq!(needed(&queue, &recipes), map(&[("b", 1), ("c", 2)]));
    }

    #[test]
    fn a_craft_covers_its_whole_yield() {
        let recipes = HashMap::from([
            recipe("crossbow", 1, &[&[("bolt", 30)]]),
            recipe("bolt", 20, &[&[("timber", 1)]]),
        ]);
        assert_eq!(needed(&q(&[("crossbow", 1), ("bolt", 1)]), &recipes), map(&[("bolt", 10), ("timber", 1)]));
        assert_eq!(needed(&q(&[("crossbow", 1), ("bolt", 2)]), &recipes), map(&[("timber", 2)]));
    }

    #[test]
    fn crafting_myself_expands_ingredients_recursively() {
        let recipes = HashMap::from([
            recipe("armor", 1, &[&[("hardened-leather", 3), ("thread", 1)]]),
            recipe("hardened-leather", 1, &[&[("leather", 2), ("oil", 1)]]),
            recipe("leather", 2, &[&[("hide", 1)]]),
        ]);
        let queue = q(&[("armor", 1)]);
        let needs = needed_with(&queue, &recipes, &["hardened-leather"], &[("hardened-leather", 1)]);
        // Carrying one hardened leather: two to craft, from 4 leather and 2 oil.
        assert_eq!(needs, map(&[("hardened-leather", 3), ("thread", 1), ("leather", 4), ("oil", 2)]));

        let craft = |craft_self: &[&str], have: &[(&str, u32)]| {
            let craft_self: HashSet<String> = craft_self.iter().map(|s| s.to_string()).collect();
            let have: HashMap<&str, u32> = have.iter().copied().collect();
            compute_needs(&queue, &recipes, &craft_self, &|id| have.get(id).copied().unwrap_or(0))
                .into_iter()
                .map(|n| (n.item_id, n.crafts))
                .collect::<HashMap<_, _>>()
        };
        let both = craft(&["hardened-leather", "leather"], &[]);
        assert_eq!(both["hardened-leather"], Some(3));
        // 6 leather at 2 per craft.
        assert_eq!(both["leather"], Some(3));
        assert_eq!(both["hide"], None);
        assert_eq!(both["thread"], None);
        // Enough in the bag: nothing to craft, and no ingredients needed for it.
        let stocked = craft(&["hardened-leather"], &[("hardened-leather", 5)]);
        assert_eq!(stocked["hardened-leather"], Some(0));
        assert!(!stocked.contains_key("leather"));
    }

    #[test]
    fn cyclic_recipes_terminate() {
        let recipes = HashMap::from([recipe("a", 1, &[&[("b", 1)]]), recipe("b", 1, &[&[("a", 1)]])]);
        let craft_self = HashSet::from(["a".to_string(), "b".to_string()]);
        let needs = compute_needs(&q(&[("a", 1)]), &recipes, &craft_self, &|_| 0);
        assert!(!needs.is_empty());
    }

    #[test]
    fn demand_for_counts_consumption_by_the_rest_of_the_queue() {
        let queue = q(&[("enhanced-swallow", 2), ("swallow", 1)]);
        assert_eq!(demand_for("swallow", &queue, &swallows()), 2);
        assert_eq!(demand_for("enhanced-swallow", &queue, &swallows()), 0);
    }
}
