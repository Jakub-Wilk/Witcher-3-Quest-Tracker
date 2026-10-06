# Witcher 3 Quest Tracker

A Windows desktop app for tracking quest progress in *The Witcher 3: Wild Hunt* (Remastered / next-gen), built with Rust and Dioxus.

> [!WARNING]
> **AI-authored code.** The code in this repository was written largely by an AI model (Claude, by Anthropic), directed by a human maintainer. It may contain errors or unconventional choices; review it before reusing it elsewhere.

## Features

- **Complete quest catalog** read directly from the game's own files, enriched with data from the [Witcher Wiki](https://witcher.fandom.com)
- **Save auto-tracking** – watches the save folder and updates quest statuses from your latest save
  - Detects separate in-game runs and links each one to a playthrough
  - Handles loading older saves (quests revert) and ignores stale files touched by cloud sync
- **Multiple playthroughs**, each with its own name and progress
- **Missable quest warnings** and a **cutoff panel** listing open quests an upcoming story point will lock out
- **Filters** by expansion, quest type, status and region (multi-select, persisted)
- **Sorting** by story order, level, name or region, plus text search
- **Localized** quest titles and journal descriptions in the game's languages
- **Wiki links** and journal descriptions for every quest
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

App data (database and `settings.toml`) is stored in the OS data directory. Set `W3QT_DATA_DIR` to use a different one.

## Project layout

| Path                     | Contents                                                    |
| ------------------------ | ----------------------------------------------------------- |
| `src/`                 | Dioxus desktop app (UI, save tracking, sync)                |
| `crates/w3_formats`    | Readers for save games, bundles, CR2W and localized strings |
| `crates/quest_scraper` | Witcher Wiki (MediaWiki API) scraper                        |
| `crates/quest_db`      | SQLite storage for quests, playthroughs and progress        |

## License

[GPL-3.0](LICENSE)

*The Witcher 3: Wild Hunt* is a trademark of CD PROJEKT S.A. This project is unofficial and not affiliated with or endorsed by CD PROJEKT RED. Wiki content is from the Witcher Wiki on Fandom, licensed under CC BY-SA.
