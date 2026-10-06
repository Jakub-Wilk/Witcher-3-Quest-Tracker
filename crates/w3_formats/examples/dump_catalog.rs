//! Prints the game's quest catalog as tab-separated rows.
//!
//! `cargo run -p w3_formats --example dump_catalog -- "G:\Steam\steamapps\common\The Witcher 3"`

use std::path::PathBuf;

fn main() {
    let dir = PathBuf::from(std::env::args().nth(1).expect("usage: dump_catalog <game dir>"));
    let catalog = w3_formats::read_game_catalog(&dir).unwrap_or_else(|e| panic!("{e}"));
    for w in &catalog.warnings {
        eprintln!("warning: {w}");
    }
    eprintln!("{} quests, languages: {}", catalog.quests.len(), catalog.languages.join(" "));
    println!("journal_path\tbase_name\tgroup\ttype\tcontent\tworld\tlevel\ten\tpl");
    for q in &catalog.quests {
        println!(
            "{}\t{}\t{}\t{:?}\t{:?}\t{}\t{}\t{}\t{}",
            q.journal_path,
            q.base_name,
            q.group,
            q.quest_type,
            q.content,
            q.world.map_or(String::new(), |w| w.to_string()),
            q.recommended_level.map_or(String::new(), |l| l.to_string()),
            q.title("en").unwrap_or(""),
            q.title("pl").unwrap_or(""),
        );
    }
}
