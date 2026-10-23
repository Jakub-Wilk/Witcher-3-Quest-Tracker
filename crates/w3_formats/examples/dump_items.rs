//! Prints the game's items and recipes as tab-separated rows.
//!
//! `cargo run -p w3_formats --example dump_items -- "G:\Steam\steamapps\common\The Witcher 3"`

use std::path::PathBuf;

use w3_formats::items::Mode;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: dump_items <game dir>"));
    let catalog = w3_formats::read_item_catalog(&dir).unwrap_or_else(|e| panic!("{e}"));
    for w in &catalog.warnings {
        eprintln!("warning: {w}");
    }
    let en = &catalog.texts["en"];
    for (mode, tag) in [(Mode::Base, "base"), (Mode::Plus, "plus")] {
        let set = catalog.set(mode);
        eprintln!("{tag}: {} items, {} recipes", set.items.len(), set.recipes.len());
        for item in set.items.values() {
            let name = en.get(&item.name_key).map_or("", String::as_str);
            println!("{tag}\titem\t{}\t{}\t{}\t{}\t{:?}\t{name}", item.id, item.category, item.quality, item.name_key, item.expansion);
        }
        for r in set.recipes.values() {
            let ings: Vec<String> = r.ingredients.iter().map(|(id, q)| format!("{q}x{id}")).collect();
            println!(
                "{tag}\trecipe\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}",
                r.product,
                r.kind,
                r.level.as_deref().unwrap_or(""),
                r.craftsman.as_deref().unwrap_or(""),
                r.yield_qty,
                ings.join(","),
                r.alternatives.len()
            );
        }
    }
    eprintln!("languages: {}", catalog.texts.keys().cloned().collect::<Vec<_>>().join(" "));
}
