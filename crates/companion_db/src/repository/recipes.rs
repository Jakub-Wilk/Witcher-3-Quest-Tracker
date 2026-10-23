use rusqlite::{Connection, params};

use crate::error::Result;
use crate::models::{Ingredient, NewRecipe, Recipe, RecipeMode};

/// Replaces every recipe with `recipes` (they are derived from the game files in full).
pub fn replace_all(conn: &Connection, recipes: &[NewRecipe]) -> Result<()> {
    conn.execute("DELETE FROM recipes", [])?;
    let mut insert = conn.prepare(
        "INSERT INTO recipes (mode, product_id, kind, level, craftsman, yield_qty, expansion)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
    )?;
    let mut insert_ingredient = conn.prepare(
        "INSERT INTO recipe_ingredients (recipe_id, variant, position, item_id, qty)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for r in recipes {
        insert.execute(params![r.mode, r.product_id, r.kind, r.level, r.craftsman, r.yield_qty, r.expansion])?;
        let id = conn.last_insert_rowid();
        for (variant, ingredients) in r.variants.iter().enumerate() {
            for (position, ing) in ingredients.iter().enumerate() {
                insert_ingredient.execute(params![id, variant as i64, position as i64, ing.item_id, ing.qty])?;
            }
        }
    }
    Ok(())
}

pub fn count(conn: &Connection, mode: RecipeMode) -> Result<i64> {
    Ok(conn.query_row("SELECT COUNT(*) FROM recipes WHERE mode = ?1", params![mode], |r| r.get(0))?)
}

/// Every recipe of one set, ordered by product id.
pub fn list(conn: &Connection, mode: RecipeMode) -> Result<Vec<Recipe>> {
    let mut stmt = conn.prepare(
        "SELECT r.id, r.product_id, r.kind, r.level, r.craftsman, r.yield_qty, r.expansion,
                ri.variant, ri.item_id, ri.qty
         FROM recipes r JOIN recipe_ingredients ri ON ri.recipe_id = r.id
         WHERE r.mode = ?1
         ORDER BY r.product_id, ri.variant, ri.position",
    )?;
    let mut rows = stmt.query(params![mode])?;
    let mut out: Vec<Recipe> = Vec::new();
    let mut current_id = None;
    while let Some(row) = rows.next()? {
        let id: i64 = row.get(0)?;
        if current_id != Some(id) {
            current_id = Some(id);
            out.push(Recipe {
                product_id: row.get(1)?,
                kind: row.get(2)?,
                level: row.get(3)?,
                craftsman: row.get(4)?,
                yield_qty: row.get(5)?,
                expansion: row.get(6)?,
                variants: Vec::new(),
            });
        }
        let recipe = out.last_mut().expect("pushed above");
        let variant = row.get::<_, u32>(7)? as usize;
        if recipe.variants.len() <= variant {
            recipe.variants.resize(variant + 1, Vec::new());
        }
        recipe.variants[variant].push(Ingredient { item_id: row.get(8)?, qty: row.get(9)? });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::open_in_memory;
    use crate::models::RecipeKind;
    use crate::repository::items;

    fn ing(id: &str, qty: u32) -> Ingredient {
        Ingredient { item_id: id.into(), qty }
    }

    #[test]
    fn round_trips_recipes_per_mode() {
        let conn = open_in_memory().unwrap();
        for id in ["swallow-2", "celandine", "dwarven-spirit"] {
            items::upsert(&conn, &items::tests::sample(id)).unwrap();
        }
        let swallow = |mode| NewRecipe {
            mode,
            product_id: "swallow-2".into(),
            kind: RecipeKind::Alchemy,
            level: None,
            craftsman: None,
            yield_qty: 1,
            expansion: None,
            variants: vec![vec![ing("celandine", 3), ing("dwarven-spirit", 1)], vec![ing("celandine", 5)]],
        };
        replace_all(&conn, &[swallow(RecipeMode::Base), swallow(RecipeMode::Plus)]).unwrap();
        assert_eq!(count(&conn, RecipeMode::Plus).unwrap(), 1);

        let base = list(&conn, RecipeMode::Base).unwrap();
        assert_eq!(base.len(), 1);
        assert_eq!(base[0].variants, swallow(RecipeMode::Base).variants);
        assert_eq!(base[0].ingredients(7), base[0].ingredients(0));

        // Replacing drops what is not in the new set.
        replace_all(&conn, &[swallow(RecipeMode::Base)]).unwrap();
        assert_eq!(count(&conn, RecipeMode::Plus).unwrap(), 0);
    }
}
