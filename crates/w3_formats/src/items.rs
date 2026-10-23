//! Items and their crafting, alchemy and enchanting recipes as the game defines them.
//!
//! Definitions are XML files under `gameplay\items\`, `gameplay\items_plus\` (New Game+
//! replacements) and `gameplay\abilities\`, base game and DLC alike:
//! - `<item name localisation_key_name category>` with its abilities as `<a>` children;
//! - `<ability name>` with `<quality min>`, which gives an item its rarity;
//! - `<recipe cookedItem_name cookedItemQuantity>`: an alchemy formula;
//! - `<schematic craftedItem_name craftsmanType_name craftsmanLevel_name craftedItemQuantity>`:
//!   a crafting diagram;
//! - `<schematic name_name localisation_key_name level>`: a runeword or glyphword, which names its
//!   own product;
//! - each recipe lists `<ingredient item_name quantity>`.
//!
//! Item ids are [`slugify`]d internal names (`Swallow 2` → `swallow-2`), so a save's inventory
//! maps onto them directly.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

use quick_xml::events::{BytesStart, Event};
use unicode_normalization::UnicodeNormalization;

use crate::bundle::BundleSet;
use crate::error::Result;
use crate::strings;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RecipeKind {
    Crafting,
    Alchemy,
    Enchanting,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Expansion {
    HeartsOfStone,
    BloodAndWine,
}

/// The two item sets: New Game+ swaps some definitions for their `items_plus` versions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Mode {
    Base,
    Plus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Item {
    /// [`slugify`]d internal name, sometimes with a `-<kind>` suffix to keep it unique.
    pub id: String,
    pub internal_name: String,
    /// Localisation key of the name (lowercase).
    pub name_key: String,
    /// The game's item category, e.g. `crafting_ingredient`, `steelsword`; `enchantment` for
    /// runewords and glyphwords.
    pub category: String,
    /// Rarity, 1 (common) to 5 (set).
    pub quality: u8,
    pub expansion: Option<Expansion>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ingredient<'a> {
    pub id: &'a str,
    pub qty: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// Id of the product item; one recipe per product.
    pub product: String,
    pub kind: RecipeKind,
    /// Crafting: `Journeyman`, `Master`, `GrandMaster`, `ArchMaster`; enchanting: `1`-`3`.
    pub level: Option<String>,
    /// Crafting: `Smith`, `Armorer`, `Crafter`.
    pub craftsman: Option<String>,
    /// How many products one craft makes.
    pub yield_qty: u32,
    pub expansion: Option<Expansion>,
    /// (item id, quantity), the primary ingredient set.
    pub ingredients: Vec<(String, u32)>,
    /// Other ingredient sets for the same product (some mutagens, for example).
    pub alternatives: Vec<Vec<(String, u32)>>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemSet {
    pub items: BTreeMap<String, Item>,
    pub recipes: BTreeMap<String, Recipe>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemCatalog {
    pub base: ItemSet,
    pub plus: ItemSet,
    /// Language code → localisation key → text, for item names and the game-text UI labels
    /// ([`GAME_TEXT_KEYS`], craftsman, quality and category labels). Keys a language lacks are
    /// missing rather than filled in from English.
    pub texts: BTreeMap<String, HashMap<String, String>>,
    /// Non-fatal problems worth logging.
    pub warnings: Vec<String>,
}

impl ItemCatalog {
    pub fn set(&self, mode: Mode) -> &ItemSet {
        match mode {
            Mode::Base => &self.base,
            Mode::Plus => &self.plus,
        }
    }
}

/// UI labels the game itself has text for: (name, localisation key).
pub const GAME_TEXT_KEYS: &[(&str, &str)] = &[
    ("alchemy", "panel_title_alchemy"),
    ("crafting", "panel_title_crafting"),
    ("enchanting", "panel_title_enchanting"),
    ("runewright", "panel_map_enchanter_pin_name"),
    ("alchemyFormula", "item_category_alchemy_recipe"),
    ("craftingDiagram", "item_category_crafting_schematic"),
    ("required", "panel_crafting_ingredients_start"),
    ("type", "gui_panel_filter_item_type"),
    ("newGamePlus", "newgame_plus"),
    ("heartsOfStone", "dlc_hearts_of_stone"),
    ("bloodAndWine", "dlc_blood_and_wine"),
];

/// `craftsmanType_name` → localisation key (as `CraftsmanTypeToLocalizationKey` in the scripts).
pub const CRAFTSMAN_TYPE_KEYS: &[(&str, &str)] =
    &[("Smith", "map_location_blacksmith"), ("Armorer", "armorer"), ("Crafter", "map_location_craftman")];

/// `craftsmanLevel_name` → localisation key (the game's keys really are spelt `crating`).
pub const CRAFTSMAN_LEVEL_KEYS: &[(&str, &str)] = &[
    ("Journeyman", "panel_shop_crating_level_journeyman"),
    ("Master", "panel_shop_crating_level_master"),
    ("GrandMaster", "panel_shop_crating_level_grand_master"),
    ("ArchMaster", "panel_shop_crating_level_arch_master"),
];

/// Runeword/glyphword `level` → localisation key.
pub const ENCHANT_LEVEL_KEYS: &[(&str, &str)] = &[
    ("1", "panel_enchanting_filter_level_1_enchant"),
    ("2", "panel_enchanting_filter_level_2_enchant"),
    ("3", "panel_enchanting_filter_level_3_enchant"),
];

/// Item quality 1-5 → rarity label key.
pub const QUALITY_KEYS: &[&str] = &[
    "panel_inventory_item_rarity_type_common",
    "panel_inventory_item_rarity_type_masterwork",
    "panel_inventory_item_rarity_type_magic",
    "panel_inventory_item_rarity_type_relic",
    "panel_inventory_item_rarity_type_set",
];

/// Localisation key of an item category's label.
pub fn category_key(category: &str) -> String {
    format!("item_category_{category}")
}

/// Stable item id from an internal or English name: `Swallow 2` → `swallow-2`.
pub fn slugify(s: &str) -> String {
    let folded: String = s
        .nfkd()
        .filter(|c| !('\u{300}'..='\u{36f}').contains(c))
        .flat_map(char::to_lowercase)
        .filter(|c| *c != '\'' && *c != '\u{2019}')
        .collect();
    let mut out = String::with_capacity(folded.len());
    for c in folded.chars() {
        if c.is_ascii_lowercase() || c.is_ascii_digit() {
            out.push(c);
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

fn wanted_path(path: &str) -> bool {
    let p = path.to_ascii_lowercase();
    p.ends_with(".xml")
        && ["gameplay\\items\\", "gameplay\\items_plus\\", "gameplay\\abilities\\"]
            .iter()
            .any(|dir| p.contains(dir) && !p[p.find(dir).unwrap() + dir.len()..].contains('\\'))
}

fn expansion_of(path: &str) -> Option<Expansion> {
    if path.contains("dlc\\ep1\\") {
        Some(Expansion::HeartsOfStone)
    } else if path.contains("dlc\\bob\\") {
        Some(Expansion::BloodAndWine)
    } else {
        None
    }
}

/// Reads every item and recipe, with names in every language, from a game install (read-only).
pub fn read_item_catalog(game_dir: &Path) -> Result<ItemCatalog> {
    let bundles = BundleSet::open(game_dir, &wanted_path)?;
    let mut catalog = ItemCatalog::default();

    // Base-game files first so they win over DLC re-definitions of the same item.
    let mut paths: Vec<&str> = bundles.paths().collect();
    paths.sort_unstable_by_key(|p| (p.starts_with("dlc\\"), *p));

    let mut defs = Definitions::default();
    for path in paths {
        match bundles.read(path) {
            Ok(data) => defs.read_file(path, &data, &mut catalog.warnings),
            Err(e) => catalog.warnings.push(format!("{path}: {e}")),
        }
    }

    let language_files = strings::language_files(game_dir)?;
    let en_files = language_files.get("en").map(Vec::as_slice).unwrap_or_default();
    let candidate_keys: HashSet<String> = defs
        .items
        .values()
        .flat_map(|m| m.values().map(|d| d.key.clone()))
        .chain(defs.recipes.values().flatten().filter_map(|r| r.product_key.clone()))
        .collect();
    let english = strings::load_keys(en_files, &candidate_keys)?;

    let mut used: HashSet<String> = HashSet::new();
    let mut warnings = HashSet::new();
    catalog.base = defs.build(Mode::Base, &english, &mut used, &mut warnings);
    catalog.plus = defs.build(Mode::Plus, &english, &mut used, &mut warnings);
    catalog.warnings.extend(warnings);

    for set in [&catalog.base, &catalog.plus] {
        for item in set.items.values().filter(|i| i.category != "enchantment") {
            used.insert(category_key(&item.category));
        }
    }
    let label_keys = GAME_TEXT_KEYS
        .iter()
        .chain(CRAFTSMAN_TYPE_KEYS)
        .chain(CRAFTSMAN_LEVEL_KEYS)
        .chain(ENCHANT_LEVEL_KEYS)
        .map(|(_, key)| *key)
        .chain(QUALITY_KEYS.iter().copied());
    used.extend(label_keys.map(str::to_string));

    for (language, files) in &language_files {
        match strings::load_keys(files, &used) {
            Ok(texts) => {
                let cleaned = texts.into_iter().map(|(k, v)| (k, clean(&v, language))).collect();
                catalog.texts.insert(language.clone(), cleaned);
            }
            Err(e) => catalog.warnings.push(format!("{language} strings: {e}")),
        }
    }
    Ok(catalog)
}

#[derive(Debug, Clone)]
struct ItemDef {
    key: String,
    category: String,
    abilities: Vec<String>,
    expansion: Option<Expansion>,
}

#[derive(Debug, Clone)]
struct RawRecipe {
    kind: RecipeKind,
    product: String,
    /// Localisation key of the product name when the recipe names it itself (enchanting).
    product_key: Option<String>,
    level: Option<String>,
    craftsman: Option<String>,
    yield_qty: u32,
    ingredients: Vec<(String, u32)>,
    expansion: Option<Expansion>,
}

#[derive(Debug, Default)]
struct Definitions {
    /// Per mode: internal name → definition; the first definition wins.
    items: HashMap<ModeKey, HashMap<String, ItemDef>>,
    recipes: HashMap<ModeKey, Vec<RawRecipe>>,
    /// Ability name → highest `quality min`.
    ability_quality: HashMap<String, u8>,
}

type ModeKey = bool; // true = New Game+

/// A recipe being read: its start tag's attributes and the ingredients seen so far.
struct OpenRecipe {
    tag: &'static str,
    attrs: HashMap<String, String>,
    ingredients: Vec<(String, u32)>,
}

impl Definitions {
    fn read_file(&mut self, path: &str, data: &[u8], warnings: &mut Vec<String>) {
        let modes: &[ModeKey] = if path.contains("\\items_plus\\") {
            &[true]
        } else if path.contains("\\abilities\\") {
            &[]
        } else {
            &[false]
        };
        let expansion = expansion_of(path);

        let mut reader = quick_xml::Reader::from_reader(data.strip_prefix(b"\xef\xbb\xbf").unwrap_or(data));
        reader.config_mut().check_end_names = false;
        reader.config_mut().trim_text(true);
        let mut buf = Vec::new();

        let mut item: Option<(HashMap<String, String>, Vec<String>)> = None;
        // `<item>` elements nested in a definition (its `<variant>`s) only reference other items.
        let mut nested_items = 0;
        let mut in_a = false;
        let mut ability: Option<String> = None;
        let mut recipe: Option<OpenRecipe> = None;

        loop {
            let event = match reader.read_event_into(&mut buf) {
                Ok(event) => event,
                Err(e) => {
                    warnings.push(format!("{path}: {e}"));
                    break;
                }
            };
            match event {
                Event::Eof => break,
                Event::Start(ref e) | Event::Empty(ref e) => {
                    let empty = matches!(event, Event::Empty(_));
                    match e.name().as_ref() {
                        b"item" if item.is_some() => nested_items += usize::from(!empty),
                        b"item" => {
                            let attrs = attributes(e);
                            if empty {
                                self.add_item(modes, attrs, Vec::new(), expansion);
                            } else {
                                item = Some((attrs, Vec::new()));
                            }
                        }
                        b"a" if item.is_some() && !empty => in_a = true,
                        b"ability" if !empty => ability = attributes(e).remove("name"),
                        b"quality" => {
                            if let (Some(name), Some(min)) =
                                (&ability, attributes(e).get("min").and_then(|m| m.parse::<u8>().ok()))
                            {
                                let q = self.ability_quality.entry(name.clone()).or_default();
                                *q = (*q).max(min);
                            }
                        }
                        tag @ (b"recipe" | b"schematic") if !empty => {
                            let tag = if tag == b"recipe" { "recipe" } else { "schematic" };
                            recipe = Some(OpenRecipe { tag, attrs: attributes(e), ingredients: Vec::new() });
                        }
                        b"ingredient" => {
                            if let Some(r) = &mut recipe {
                                let a = attributes(e);
                                let qty = a.get("quantity").and_then(|q| q.parse::<u32>().ok()).unwrap_or(0);
                                if let Some(name) = a.get("item_name").filter(|n| !n.is_empty() && qty > 0) {
                                    r.ingredients.push((name.clone(), qty));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                Event::Text(ref t) if in_a => {
                    if let Some((_, abilities)) = &mut item {
                        abilities.push(String::from_utf8_lossy(t).trim().to_string());
                    }
                }
                Event::End(ref e) => match e.name().as_ref() {
                    b"item" if nested_items > 0 => nested_items -= 1,
                    b"item" => {
                        if let Some((attrs, abilities)) = item.take() {
                            self.add_item(modes, attrs, abilities, expansion);
                        }
                        in_a = false;
                    }
                    b"a" => in_a = false,
                    b"ability" => ability = None,
                    tag @ (b"recipe" | b"schematic") => {
                        if let Some(r) = recipe.take_if(|r| r.tag.as_bytes() == tag) {
                            if let Some(raw) = raw_recipe(r, expansion) {
                                for &mode in modes {
                                    self.recipes.entry(mode).or_default().push(raw.clone());
                                }
                            }
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            buf.clear();
        }
    }

    fn add_item(
        &mut self,
        modes: &[ModeKey],
        attrs: HashMap<String, String>,
        abilities: Vec<String>,
        expansion: Option<Expansion>,
    ) {
        let (Some(name), Some(key)) = (attrs.get("name"), attrs.get("localisation_key_name")) else { return };
        if name.is_empty() || key.is_empty() {
            return;
        }
        for &mode in modes {
            self.items.entry(mode).or_default().entry(name.clone()).or_insert_with(|| ItemDef {
                key: key.to_lowercase(),
                category: attrs.get("category").cloned().unwrap_or_default(),
                abilities: abilities.clone(),
                expansion,
            });
        }
    }

    fn def(&self, mode: Mode, internal: &str) -> Option<&ItemDef> {
        let get = |plus: bool| self.items.get(&plus).and_then(|m| m.get(internal));
        match mode {
            Mode::Base => get(false),
            Mode::Plus => get(true).or_else(|| get(false)),
        }
    }

    fn build(
        &self,
        mode: Mode,
        english: &HashMap<String, String>,
        used: &mut HashSet<String>,
        warnings: &mut HashSet<String>,
    ) -> ItemSet {
        let tag = match mode {
            Mode::Base => "base",
            Mode::Plus => "plus",
        };
        let mut set = ItemSet::default();
        let mut id_of: HashMap<String, String> = HashMap::new();

        let mut item_id = |set: &mut ItemSet,
                           internal: &str,
                           key: Option<&str>,
                           kind: Option<RecipeKind>,
                           expansion: Option<Expansion>|
         -> Option<String> {
            if let Some(id) = id_of.get(internal) {
                return Some(id.clone());
            }
            let def = self.def(mode, internal);
            let key = key.or(def.map(|d| d.key.as_str()));
            let Some(name_key) = key.filter(|k| english.contains_key(*k)) else {
                let why = key.map_or("no definition".to_string(), |k| format!("no text for {k}"));
                warnings.insert(format!("[{tag}] no name for \"{internal}\" ({why})"));
                return None;
            };
            let mut id = slugify(internal);
            if set.items.get(&id).is_some_and(|existing| existing.name_key != name_key) {
                let suffix = match kind {
                    Some(RecipeKind::Crafting) => "crafting",
                    Some(RecipeKind::Alchemy) => "alchemy",
                    Some(RecipeKind::Enchanting) => "enchanting",
                    None => "x",
                };
                id = format!("{id}-{suffix}");
            }
            let quality = def
                .map(|d| d.abilities.iter().filter_map(|a| self.ability_quality.get(a)).copied().max().unwrap_or(0))
                .unwrap_or(0)
                .clamp(1, 5);
            let category = if kind == Some(RecipeKind::Enchanting) {
                "enchantment".to_string()
            } else {
                def.map(|d| d.category.clone()).unwrap_or_default()
            };
            set.items.insert(
                id.clone(),
                Item {
                    id: id.clone(),
                    internal_name: internal.to_string(),
                    name_key: name_key.to_string(),
                    category,
                    quality,
                    expansion: def.and_then(|d| d.expansion).or(expansion),
                },
            );
            used.insert(name_key.to_string());
            id_of.insert(internal.to_string(), id.clone());
            Some(id)
        };

        let signature = |ings: &[(String, u32)]| {
            let mut parts: Vec<String> = ings.iter().map(|(id, qty)| format!("{qty}x{id}")).collect();
            parts.sort();
            parts.join(",")
        };

        let raws = self.recipes.get(&(mode == Mode::Plus)).map(Vec::as_slice).unwrap_or_default();
        let mut skipped = Vec::new();
        for r in raws {
            let Some(product) = item_id(&mut set, &r.product, r.product_key.as_deref(), Some(r.kind), r.expansion)
            else {
                continue;
            };
            let mut ingredients: Vec<(String, u32)> = Vec::new();
            let mut ok = true;
            for (name, qty) in &r.ingredients {
                let Some(id) = item_id(&mut set, name, None, None, None) else {
                    ok = false;
                    break;
                };
                match ingredients.iter_mut().find(|(i, _)| *i == id) {
                    Some((_, q)) => *q += qty,
                    None => ingredients.push((id, *qty)),
                }
            }
            if !ok {
                skipped.push(format!("[{tag}] skipped recipe for \"{}\": unnamed ingredient", r.product));
                continue;
            }
            match set.recipes.get_mut(&product) {
                None => {
                    set.recipes.insert(
                        product.clone(),
                        Recipe {
                            product,
                            kind: r.kind,
                            level: r.level.clone(),
                            craftsman: r.craftsman.clone(),
                            yield_qty: r.yield_qty,
                            expansion: r.expansion,
                            ingredients,
                            alternatives: Vec::new(),
                        },
                    );
                }
                Some(existing) => {
                    // Same product, different ingredient set: an alternative recipe.
                    let sig = signature(&ingredients);
                    let known = std::iter::once(&existing.ingredients)
                        .chain(&existing.alternatives)
                        .any(|ings| signature(ings) == sig);
                    if !known {
                        existing.alternatives.push(ingredients);
                    }
                }
            }
        }
        warnings.extend(skipped);
        set
    }
}

fn raw_recipe(r: OpenRecipe, expansion: Option<Expansion>) -> Option<RawRecipe> {
    if r.ingredients.is_empty() {
        return None;
    }
    let a = &r.attrs;
    let num = |name: &str| a.get(name).map_or(Some(1), |v| v.parse::<u32>().ok());
    let base = RawRecipe {
        kind: RecipeKind::Alchemy,
        product: String::new(),
        product_key: None,
        level: None,
        craftsman: None,
        yield_qty: 1,
        ingredients: r.ingredients,
        expansion,
    };
    if r.tag == "recipe" {
        let yield_qty = num("cookedItemQuantity").filter(|&q| q > 0)?;
        let product = a.get("cookedItem_name").filter(|p| !p.is_empty())?.clone();
        Some(RawRecipe { product, yield_qty, ..base })
    } else if let Some(product) = a.get("craftedItem_name").filter(|p| !p.is_empty()) {
        Some(RawRecipe {
            kind: RecipeKind::Crafting,
            product: product.clone(),
            level: a.get("craftsmanLevel_name").map(|l| l.replace(' ', "")),
            craftsman: a.get("craftsmanType_name").cloned(),
            yield_qty: num("craftedItemQuantity").unwrap_or(1),
            ..base
        })
    } else if let (Some(product), Some(key)) = (a.get("name_name"), a.get("localisation_key_name")) {
        // Runewright runewords/glyphwords: the schematic names its own product.
        Some(RawRecipe {
            kind: RecipeKind::Enchanting,
            product: product.clone(),
            product_key: Some(key.to_lowercase()),
            level: a.get("level").cloned(),
            ..base
        })
    } else {
        None
    }
}

/// A tag's attributes, values trimmed and unescaped. Malformed attributes are skipped.
fn attributes(e: &BytesStart<'_>) -> HashMap<String, String> {
    e.attributes()
        .with_checks(false)
        .flatten()
        .map(|a| {
            let key = String::from_utf8_lossy(a.key.as_ref()).into_owned();
            (key, unescape(&String::from_utf8_lossy(&a.value)).trim().to_string())
        })
        .collect()
}

fn unescape(s: &str) -> String {
    if !s.contains('&') {
        return s.to_string();
    }
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&apos;", "'").replace("&amp;", "&")
}

/// Strips the game's inline markup. Arabic is stored as presentation forms: NFKC restores the
/// letters, and the Persian yeh/heh it yields become the Arabic letters that look the same
/// (dotless yeh at the end of a word, dotted elsewhere).
pub fn clean(text: &str, language: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        plain.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('>') else {
            rest = &rest[start..];
            break;
        };
        let tag = rest[start + 1..start + end].trim_start_matches('/').to_ascii_lowercase();
        if tag.starts_with("br") {
            plain.push(' ');
        }
        rest = &rest[start + end + 1..];
    }
    plain.push_str(rest);
    let plain = plain.replace('\u{fffd}', " ");
    let collapsed = plain.split_whitespace().collect::<Vec<_>>().join(" ");
    if language != "ar" {
        return collapsed;
    }
    let chars: Vec<char> = collapsed.nfkc().collect();
    let arabic_letter = |c: char| ('\u{621}'..='\u{652}').contains(&c) || ('\u{66e}'..='\u{6d3}').contains(&c);
    chars
        .iter()
        .enumerate()
        .map(|(i, &c)| match c {
            '\u{6cc}' if chars.get(i + 1).is_some_and(|&n| arabic_letter(n)) => '\u{64a}',
            '\u{6cc}' => '\u{649}',
            '\u{6be}' => '\u{647}',
            c => c,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_match_the_planner() {
        assert_eq!(slugify("Swallow 2"), "swallow-2");
        assert_eq!(slugify("Dwimeritium plate"), "dwimeritium-plate");
        assert_eq!(slugify("Mutagen red"), "mutagen-red");
        assert_eq!(slugify("Witcher's Silver Sword"), "witchers-silver-sword");
        assert_eq!(slugify("  Aard_Glyph -- 1 "), "aard-glyph-1");
        // Like the planner: ł has no decomposition, so it splits the word.
        assert_eq!(slugify("Ćwiczenie Żółć"), "cwiczenie-zo-c");
    }

    #[test]
    fn selects_definition_files() {
        assert!(wanted_path("gameplay\\items\\def_item_crafting_recipes.xml"));
        assert!(wanted_path("dlc\\bob\\data\\gameplay\\items_plus\\def_item_alchemy.xml"));
        assert!(wanted_path("gameplay\\abilities\\items_abilities.xml"));
        assert!(!wanted_path("gameplay\\items\\sub\\x.xml"));
        assert!(!wanted_path("gameplay\\journal\\quests\\q001.journal"));
    }

    #[test]
    fn strips_markup_and_fixes_arabic() {
        assert_eq!(clean("<font color=\"red\">Swallow</font><br>  potion", "en"), "Swallow potion");
        // Presentation-form yeh (U+FEF1, NFKC → U+064A) and Persian yeh at the end of a word.
        assert_eq!(clean("\u{6cc}\u{628} \u{628}\u{6cc}", "ar"), "\u{64a}\u{628} \u{628}\u{649}");
    }

    const ITEMS: &str = r#"<?xml version="1.0"?>
<redxml><definitions>
  <!-- <item name="Commented out" localisation_key_name="nope"/> -->
  <abilities>
    <ability name="Swallow _Stats"><quality min="2" max="2"/></ability>
  </abilities>
  <items>
    <item name="Swallow 2" category="potion" localisation_key_name="item_name_swallow_2">
      <base_abilities><a>Swallow _Stats</a></base_abilities>
      <variants><variant category="potion"><item>Celandine</item></variant></variants>
    </item>
    <item name="Celandine" category="alchemy_ingredient" localisation_key_name="item_name_celandine" />
    <item name="Dwarven spirit" category="alchemy_ingredient" localisation_key_name="item_name_spirit"/>
    <item name="Celandine" category="ignored duplicate" localisation_key_name="item_name_other"/>
  </items>
  <alchemy_recipes>
    <recipe name_name="Recipe for Swallow 2" cookedItem_name="Swallow 2" cookedItemQuantity="1">
      <ingredients>
        <ingredient item_name="Celandine" quantity="2"/>
        <ingredient item_name="Celandine" quantity="1"/>
        <ingredient item_name="Dwarven spirit" quantity="1"/>
      </ingredients>
    </recipe>
    <recipe name_name="Alt" cookedItem_name="Swallow 2" cookedItemQuantity="1">
      <ingredients><ingredient item_name="Celandine" quantity="5"/></ingredients>
    </recipe>
    <recipe name_name="Same as primary" cookedItem_name="Swallow 2" cookedItemQuantity="1">
      <ingredients><ingredient item_name="Dwarven spirit" quantity="1"/><ingredient item_name="Celandine" quantity="3"/></ingredients>
    </recipe>
  </alchemy_recipes>
  <crafting_recipes>
    <schematic name_name="Bad" craftedItem_name="Swallow 2" craftsmanLevel_name="Grand Master" craftsmanType_name="Smith">
      <ingredients><ingredient item_name="Unknown thing" quantity="1"/></ingredients>
    </schematic>
  </crafting_recipes>
</definitions></redxml>"#;

    fn english() -> HashMap<String, String> {
        ["item_name_swallow_2", "item_name_celandine", "item_name_spirit"]
            .into_iter()
            .map(|k| (k.to_string(), k.to_string()))
            .collect()
    }

    #[test]
    fn reads_items_and_recipes() {
        let mut defs = Definitions::default();
        let mut warnings = Vec::new();
        defs.read_file("dlc\\ep1\\data\\gameplay\\items\\def.xml", ITEMS.as_bytes(), &mut warnings);
        assert!(warnings.is_empty(), "{warnings:?}");

        let mut used = HashSet::new();
        let mut build_warnings = HashSet::new();
        let set = defs.build(Mode::Base, &english(), &mut used, &mut build_warnings);
        assert_eq!(set.items.len(), 3);
        let swallow = &set.items["swallow-2"];
        assert_eq!(swallow.quality, 2);
        assert_eq!(swallow.category, "potion");
        assert_eq!(swallow.expansion, Some(Expansion::HeartsOfStone));
        assert_eq!(set.items["celandine"].name_key, "item_name_celandine");

        let recipe = &set.recipes["swallow-2"];
        assert_eq!(recipe.kind, RecipeKind::Alchemy);
        assert_eq!(recipe.ingredients, vec![("celandine".into(), 3), ("dwarven-spirit".into(), 1)]);
        assert_eq!(recipe.alternatives, vec![vec![("celandine".to_string(), 5)]]);
        assert_eq!(build_warnings.len(), 2, "{build_warnings:?}"); // unknown item + skipped recipe

        // Without an items_plus file, New Game+ falls back to nothing (only items_plus feeds it).
        assert!(defs.build(Mode::Plus, &english(), &mut used, &mut build_warnings).recipes.is_empty());
    }
}
