# Witcher 3 Companion

A Windows desktop companion for *The Witcher 3: Wild Hunt* (Remastered): a quest journal that follows your saves, and a crafting workbench that plans recipes against Geralt's inventory. Built with Rust and Dioxus.

> [!WARNING]
> **AI-authored code.** The code in this repository was written largely by an AI model (Claude, by Anthropic), directed by a human maintainer. It may contain errors or unconventional choices; review it before reusing it elsewhere.

## Features

### Quests

- **Complete quest catalog** read directly from the game's own files, enriched with data from the [Witcher Wiki](https://witcher.fandom.com)
- **Save auto-tracking** – watches the save folder and updates quest statuses from your latest save
  - Detects separate in-game runs and links each one to a playthrough
  - Handles loading older saves (quests revert) and ignores stale files touched by cloud sync
- **Multiple playthroughs**, each with its own name and progress
- **Missable quest warnings** and a **cutoff panel** listing open quests an upcoming story point will lock out
- **Filters** by expansion, quest type, status and region (multi-select, persisted)
- **Sorting** by story order, level, name or region, plus text search
- **Wiki links** and journal descriptions for every quest

### Workbench

- **Every crafting diagram, alchemy formula and runeword/glyphword** read from the game files, with item pictures from the Witcher Wiki; New Game+ playthroughs use the New Game+ recipes
- **Recipe browser** with search (in your language or English), kind and category filters, and a "ready to make" filter
- **A plan per playthrough**: queue crafts, see what is still needed against the inventory read from your latest save, mark ingredients as gathered, and choose to craft intermediate ingredients yourself (their ingredients are added recursively)

### Everywhere

- **Automatic save import** – the save folder is watched; each new save updates quest progress and the inventory of its playthrough
- **18 languages** – the interface and the game's own names (quests, items, crafting labels) in every language the game ships, including right-to-left Arabic
- **One sync** reads quests, items and recipes from the game files, then cutoffs, prerequisites and item pictures from the wiki
- **Read-only** – game files and saves are never modified
- **Guided first-launch setup** with automatic Steam/GOG install and save folder detection

## Requirements

- Windows
- *The Witcher 3: Wild Hunt* 4.0+ (Remastered) installed

## Download

Prebuilt Windows binaries are on the [Releases](../../releases) page.

## Building

```sh
cargo run --release
```

App data (database and `settings.toml`) is stored in `%APPDATA%\witcher3_companion`. Data from the former Witcher 3 Quest Tracker (`%APPDATA%\witcher3_quest_tracker`) is copied over on first launch. Set `W3C_DATA_DIR` to use a different directory.

## Project layout

| Path                     | Contents                                                    |
| ------------------------ | ----------------------------------------------------------- |
| `src/`                  | Dioxus desktop app (UI, save tracking, sync, i18n)                       |
| `assets/`               | Stylesheets, bundled fonts, interface translations (`i18n/`), icons      |
| `crates/w3_formats`     | Readers for save games (journal, inventory), bundles, item XML, CR2W and strings |
| `crates/witcher_wiki`   | Witcher Wiki (MediaWiki API) client: quests and item pictures            |
| `crates/companion_db`   | SQLite storage: quests, items, recipes, pictures, playthroughs and plans |

## License

[GPL-3.0](LICENSE)

*The Witcher 3: Wild Hunt* is a trademark of CD PROJEKT S.A. This project is unofficial and not affiliated with or endorsed by CD PROJEKT RED. Wiki content, including item pictures, is from the Witcher Wiki on Fandom, licensed under CC BY-SA. The bundled fonts (Cinzel, IM Fell English, EB Garamond, Amiri) are under the SIL Open Font License; see `assets/fonts/`.
