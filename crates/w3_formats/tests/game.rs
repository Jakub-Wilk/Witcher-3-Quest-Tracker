//! Tests against a real game install. Run with
//! `W3_GAME_DIR="G:\Steam\steamapps\common\The Witcher 3" cargo test -p w3_formats -- --ignored`.

use std::path::PathBuf;

use w3_formats::items::{Mode, RecipeKind};
use w3_formats::{Content, JournalQuestType, read_game_catalog, read_item_catalog};

fn game_dir() -> Option<PathBuf> {
    std::env::var_os("W3_GAME_DIR").map(PathBuf::from)
}

#[test]
#[ignore = "needs W3_GAME_DIR"]
fn reads_the_quest_catalog() {
    let Some(dir) = game_dir() else { return };
    let catalog = read_game_catalog(&dir).unwrap();
    for w in &catalog.warnings {
        eprintln!("warning: {w}");
    }
    assert!(catalog.quests.len() > 350, "{} quests", catalog.quests.len());
    assert!(catalog.languages.contains(&"en".to_string()));
    assert!(catalog.languages.contains(&"pl".to_string()));

    let find = |path: &str| catalog.quests.iter().find(|q| q.journal_path == path).unwrap();

    let dream = find("gameplay\\journal\\quests\\q001beggining.journal");
    assert_eq!(dream.base_name, "Q001 Dream");
    assert_eq!(dream.content, Content::Base);
    assert_eq!(dream.group, "Prologue");
    assert_eq!(dream.title("en"), Some("Kaer Morhen"));
    assert!(dream.title("pl").is_some());
    assert!(dream.descriptions.contains_key("en"));

    let griffin = find("gameplay\\journal\\quests\\q002griffin.journal");
    assert_eq!(griffin.recommended_level, Some(3));
    assert_eq!(griffin.title("en"), Some("The Beast of White Orchard"));

    let werewolf = find("gameplay\\journal\\quests\\sq104werewolf.journal");
    assert_eq!(werewolf.quest_type, JournalQuestType::Side);

    let bob = catalog.quests.iter().filter(|q| q.content == Content::BloodAndWine).count();
    let hos = catalog.quests.iter().filter(|q| q.content == Content::HeartsOfStone).count();
    assert!(bob > 50 && hos > 15, "bob {bob}, hos {hos}");
    assert!(catalog.quests.iter().all(|q| !q.group.is_empty()), "every quest has a group");
}

#[test]
#[ignore = "needs W3_GAME_DIR"]
fn reads_the_item_catalog() {
    let Some(dir) = game_dir() else { return };
    let catalog = read_item_catalog(&dir).unwrap();
    // The counts the Recipe Planner extracted from the same Remastered install.
    assert_eq!((catalog.base.items.len(), catalog.base.recipes.len()), (827, 629));
    assert_eq!((catalog.plus.items.len(), catalog.plus.recipes.len()), (838, 641));

    let swallow = &catalog.set(Mode::Base).recipes["swallow-2"];
    assert_eq!(swallow.kind, RecipeKind::Alchemy);
    let en = &catalog.texts["en"];
    assert_eq!(en[&catalog.base.items["swallow-2"].name_key], "Enhanced Swallow");
    assert!(catalog.texts["pl"].contains_key(&catalog.base.items["swallow-2"].name_key));
    assert!(en.contains_key("panel_title_alchemy"));
}
