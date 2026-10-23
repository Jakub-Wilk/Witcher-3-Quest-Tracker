//! The item half of a sync: items and recipes from the game files, pictures from the wiki.
//!
//! Wiki pictures are matched to items by the infobox `id` (the game's internal name), falling
//! back to the English name. A picture is only downloaded when the wiki's file or its SHA-1
//! differs from the stored one.

use std::collections::{BTreeMap, HashMap, HashSet};

use companion_db::{Connection, Expansion, Ingredient, ItemImage, NewItem, NewRecipe, RecipeKind, RecipeMode, items, recipes};
use w3_formats::ItemCatalog;
use w3_formats::items::{self as game_items, Mode, slugify};
use witcher_wiki::{ItemInfobox, WikiClient};

/// How many pictures are downloaded at once.
const PARALLEL_DOWNLOADS: usize = 8;

/// Picture work found by [`fetch_pictures`], per item id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Picture {
    /// The stored picture is current.
    Unchanged,
    New(ItemImage),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PictureProgress {
    ReadingPages { done: usize, total: usize },
    Downloading { done: usize, total: usize },
}

/// Summary of [`merge`].
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct ItemReport {
    pub items: usize,
    pub base_recipes: usize,
    pub plus_recipes: usize,
    /// Items that have a picture after the sync.
    pub pictures: usize,
    pub pictures_downloaded: usize,
    pub wiki_error: Option<String>,
}

/// Finds every item's wiki picture and downloads those that are new or changed. `known` is the
/// stored (wiki file, SHA-1) per item id.
pub async fn fetch_pictures(
    catalog: &ItemCatalog,
    known: &HashMap<String, (String, String)>,
    mut on_progress: impl FnMut(PictureProgress),
) -> Result<HashMap<String, Picture>, String> {
    let client = WikiClient::new().map_err(|e| e.to_string())?;
    let boxes = client
        .fetch_item_infoboxes(|done, total| on_progress(PictureProgress::ReadingPages { done, total }))
        .await
        .map_err(|e| e.to_string())?;
    if boxes.is_empty() {
        return Err("the wiki returned no item pages".into());
    }
    let file_of = match_pictures(&english_names(catalog), &boxes);

    let mut files: Vec<String> = file_of.values().cloned().collect::<HashSet<_>>().into_iter().collect();
    files.sort();
    let info = client.file_info(&files, |_, _| {}).await.map_err(|e| e.to_string())?;

    let mut out = HashMap::new();
    // File name -> the items that use it and need it downloaded.
    let mut downloads: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for (id, file) in &file_of {
        let Some(wiki) = info.get(file) else { continue };
        if known.get(id).is_some_and(|(f, sha1)| f == file && *sha1 == wiki.sha1) {
            out.insert(id.clone(), Picture::Unchanged);
        } else {
            downloads.entry(file).or_default().push(id);
        }
    }

    let total = downloads.len();
    let mut done = 0;
    let jobs: Vec<(&str, Vec<&str>)> = downloads.into_iter().collect();
    for batch in jobs.chunks(PARALLEL_DOWNLOADS) {
        let results = futures_util::future::join_all(batch.iter().map(|(file, _)| client.download(&info[*file].url))).await;
        for ((file, ids), result) in batch.iter().zip(results) {
            match result {
                Ok(bytes) => {
                    let wiki = &info[*file];
                    let image = ItemImage {
                        image: bytes,
                        content_type: wiki.mime.clone(),
                        wiki_file: file.to_string(),
                        sha1: wiki.sha1.clone(),
                    };
                    for id in ids {
                        out.insert(id.to_string(), Picture::New(image.clone()));
                    }
                }
                Err(e) => tracing::warn!("Could not download {file}: {e}"),
            }
        }
        done += batch.len();
        on_progress(PictureProgress::Downloading { done, total });
    }
    Ok(out)
}

/// Item id -> English name, over both item sets.
fn english_names(catalog: &ItemCatalog) -> BTreeMap<String, String> {
    let en = catalog.texts.get("en");
    catalog
        .base
        .items
        .values()
        .chain(catalog.plus.items.values())
        .map(|item| {
            let name = en.and_then(|t| t.get(&item.name_key)).cloned().unwrap_or_else(|| item.id.clone());
            (item.id.clone(), name)
        })
        .collect()
}

/// Item id -> wiki file: by infobox `id`, else by the item's English name against infobox names
/// and page titles. The first infobox to claim a slug wins.
fn match_pictures(items: &BTreeMap<String, String>, boxes: &[ItemInfobox]) -> HashMap<String, String> {
    let mut by_id: HashMap<String, &str> = HashMap::new();
    let mut by_name: HashMap<String, &str> = HashMap::new();
    for b in boxes {
        for id in &b.ids {
            by_id.entry(slugify(id)).or_insert(&b.file);
        }
        for name in [&b.name, &b.page_title] {
            if !name.is_empty() {
                by_name.entry(slugify(name)).or_insert(&b.file);
            }
        }
    }
    items
        .iter()
        .filter_map(|(id, name)| {
            let file = by_id.get(id).or_else(|| by_name.get(&slugify(name)))?;
            Some((id.clone(), file.to_string()))
        })
        .collect()
}

/// Stores items, game text, both recipe sets and new pictures (part of the sync transaction).
/// Items that left the game data are deleted, with the planner rows that referred to them.
pub fn merge(
    tx: &Connection,
    catalog: &ItemCatalog,
    pictures: &Result<HashMap<String, Picture>, String>,
) -> companion_db::Result<ItemReport> {
    let mut report = ItemReport { wiki_error: pictures.as_ref().err().cloned(), ..Default::default() };
    if catalog.base.items.is_empty() {
        return Ok(report);
    }

    let mut all: BTreeMap<&str, NewItem> = BTreeMap::new();
    for (mode, set) in [(Mode::Base, &catalog.base), (Mode::Plus, &catalog.plus)] {
        for item in set.items.values() {
            let entry = all.entry(&item.id).or_insert_with(|| NewItem {
                id: item.id.clone(),
                internal_name: item.internal_name.clone(),
                name_key: item.name_key.clone(),
                category: item.category.clone(),
                quality: item.quality,
                expansion: item.expansion.map(expansion),
                in_base: false,
                in_plus: false,
            });
            match mode {
                Mode::Base => entry.in_base = true,
                Mode::Plus => entry.in_plus = true,
            }
        }
    }
    for item in all.values() {
        items::upsert(tx, item)?;
    }
    items::delete_missing(tx, &all.keys().map(|id| id.to_string()).collect())?;
    report.items = all.len();

    for (language, texts) in &catalog.texts {
        items::set_game_texts(tx, language, texts)?;
    }
    items::retain_game_text_languages(tx, &catalog.texts.keys().cloned().collect::<Vec<_>>())?;

    let mut new_recipes = Vec::new();
    for (mode, set) in [(RecipeMode::Base, &catalog.base), (RecipeMode::Plus, &catalog.plus)] {
        for r in set.recipes.values() {
            let to_ingredients =
                |ings: &[(String, u32)]| ings.iter().map(|(id, qty)| Ingredient { item_id: id.clone(), qty: *qty }).collect();
            new_recipes.push(NewRecipe {
                mode,
                product_id: r.product.clone(),
                kind: match r.kind {
                    game_items::RecipeKind::Crafting => RecipeKind::Crafting,
                    game_items::RecipeKind::Alchemy => RecipeKind::Alchemy,
                    game_items::RecipeKind::Enchanting => RecipeKind::Enchanting,
                },
                level: r.level.clone(),
                craftsman: r.craftsman.clone(),
                yield_qty: r.yield_qty,
                expansion: r.expansion.map(expansion),
                variants: std::iter::once(&r.ingredients).chain(&r.alternatives).map(|v| to_ingredients(v)).collect(),
            });
        }
    }
    recipes::replace_all(tx, &new_recipes)?;
    report.base_recipes = catalog.base.recipes.len();
    report.plus_recipes = catalog.plus.recipes.len();

    if let Ok(pictures) = pictures {
        for (id, picture) in pictures {
            if let (Picture::New(image), true) = (picture, all.contains_key(id.as_str())) {
                items::set_image(tx, id, image)?;
                report.pictures_downloaded += 1;
            }
        }
    }
    report.pictures = items::image_hashes(tx)?.len();
    Ok(report)
}

fn expansion(e: game_items::Expansion) -> Expansion {
    match e {
        game_items::Expansion::HeartsOfStone => Expansion::HeartsOfStone,
        game_items::Expansion::BloodAndWine => Expansion::BloodAndWine,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use companion_db::open_in_memory;
    use w3_formats::items::{Item, ItemSet, Recipe};

    fn item(id: &str) -> (String, Item) {
        let it = Item {
            id: id.into(),
            internal_name: id.into(),
            name_key: format!("key_{id}"),
            category: "crafting_ingredient".into(),
            quality: 1,
            expansion: None,
        };
        (id.to_string(), it)
    }

    fn catalog() -> ItemCatalog {
        let items: BTreeMap<String, Item> =
            [item("swallow-2"), item("celandine")].into_iter().collect();
        let recipe = Recipe {
            product: "swallow-2".into(),
            kind: game_items::RecipeKind::Alchemy,
            level: None,
            craftsman: None,
            yield_qty: 1,
            expansion: None,
            ingredients: vec![("celandine".into(), 3)],
            alternatives: vec![vec![("celandine".into(), 5)]],
        };
        let set = ItemSet { items, recipes: BTreeMap::from([("swallow-2".to_string(), recipe)]) };
        let mut plus = set.clone();
        plus.items.extend([item("ngp-only")]);
        let texts = HashMap::from([
            ("key_swallow-2".to_string(), "Enhanced Swallow".to_string()),
            ("key_celandine".to_string(), "Celandine".to_string()),
        ]);
        ItemCatalog { base: set, plus, texts: BTreeMap::from([("en".to_string(), texts)]), warnings: Vec::new() }
    }

    fn infobox(ids: &[&str], name: &str, file: &str) -> ItemInfobox {
        ItemInfobox {
            page_title: name.into(),
            ids: ids.iter().map(|s| s.to_string()).collect(),
            name: name.into(),
            file: file.into(),
        }
    }

    #[test]
    fn matches_pictures_by_id_then_name() {
        let names = english_names(&catalog());
        let boxes = [
            infobox(&["Swallow 2"], "Swallow", "Swallow.png"),
            infobox(&[], "Celandine", "Celandine.png"),
            infobox(&["Celandine"], "Wrong", "Wrong.png"),
        ];
        let matched = match_pictures(&names, &boxes);
        assert_eq!(matched["swallow-2"], "Swallow.png");
        // The id match wins over the name match.
        assert_eq!(matched["celandine"], "Wrong.png");
        assert!(!matched.contains_key("ngp-only"));
    }

    #[test]
    fn merges_items_recipes_and_pictures() {
        let conn = open_in_memory().unwrap();
        let image = ItemImage { image: vec![1], content_type: "image/png".into(), wiki_file: "S.png".into(), sha1: "x".into() };
        let pictures = Ok(HashMap::from([
            ("swallow-2".to_string(), Picture::New(image)),
            ("celandine".to_string(), Picture::Unchanged),
        ]));
        let report = merge(&conn, &catalog(), &pictures).unwrap();
        assert_eq!((report.items, report.base_recipes, report.plus_recipes, report.pictures), (3, 1, 1, 1));

        let listed = items::list(&conn, "en").unwrap();
        let ngp = listed.iter().find(|i| i.id == "ngp-only").unwrap();
        assert_eq!(ngp.name, "ngp-only");
        let swallow = recipes::list(&conn, RecipeMode::Plus).unwrap();
        assert_eq!(swallow[0].variants.len(), 2);

        // A later sync without an item drops it; an empty catalog changes nothing.
        let mut smaller = catalog();
        smaller.plus.items.remove("ngp-only");
        merge(&conn, &smaller, &Err("offline".into())).unwrap();
        assert_eq!(items::count(&conn).unwrap(), 2);
        merge(&conn, &ItemCatalog::default(), &Err("offline".into())).unwrap();
        assert_eq!(items::count(&conn).unwrap(), 2);
        assert_eq!(items::image_hashes(&conn).unwrap().len(), 1);
    }
}
