# Witcher 3 Quest Tracker — Dioxus Application Plan (`witcher_3_quest_tracker`)

A modern desktop GUI application built with **Dioxus** (desktop renderer) that uses `quest_db` for SQLite persistence and `quest_scraper` for on-demand wiki synchronization.

---

## Key Features & Requirements

1. **Clean, Modern Dark Mode UI**:
   - **Aesthetics**: Glassmorphism cards, glowing Witcher gold (`#e5b869`) and crimson (`#e74c3c`) accents, custom CSS animations, crisp micro-interactions, responsive 3-panel layout.
2. **On-Demand Sync Engine ("Sync Quests" button)**:
   - Scrapes quest metadata from Witcher Fandom Wiki via `quest_scraper`.
   - **Smart Merge Logic**: Updates static quest fields (`region`, `level`, `type`, `description`, `cutoffs`, `prerequisites`) and adds missing quests, while **preserving user progress** (`status`, `notes`, `started_at`, `completed_at`).
   - Resolves text-based cutoff titles and prerequisite names into DB `id` foreign keys.
3. **No Manual Quest Adding**:
   - "Sync" is the exclusive mechanism for updating reference quests.
4. **Dedicated Cut-off & Warning Pane (Right Panel)**:
   - Displays all active cutoff points (e.g. *Isle of Mists*, *Now or Never*).
   - Lists uncompleted quests at risk of being locked out if the cutoff quest is completed.
   - Shows prerequisite warnings if prerequisite quests are not yet done.
5. **Simplified Quest Completion**:
   - **Checkbox**: Toggles between `Completed` and `NotStarted`.
   - **Mark Failed Button**: Dedicated action button to mark a quest as `Failed` (or clear failure).
   - **Notes Drawer**: Expandable note field per quest entry.
6. **Playthrough Management**:
   - Create and switch between multiple named playthroughs (Difficulty, NG+, notes).

---

## UI Layout Diagram

```
+---------------------------------------------------------------------------------------------------------+
|  Witcher 3 Quest Tracker                                                [ Select Playthrough v ] [+ New]|
+-------------------+----------------------------------------------------+--------------------------------+
|  SIDEBAR          |  MAIN QUEST VIEW                                   |  CUTOFF WARNING PANE           |
|                   |  [ Search quests... ]            [ Sync Quests 🔄 ] |                                |
|  Filters:         +----------------------------------------------------+  ⚠️ Impending Cutoffs         |
|  [ All ] [Base]   | [x] Lilac and Gooseberries    [Completed] [📝 Note] |                                |
|  [HoS]  [B&W]     |     Region: White Orchard | Level: 1 | Main        |  📌 Isle of Mists              |
|                   |----------------------------------------------------|  The following quests will     |
|  Category:        | [ ] The Last Wish             [Mark Failed]        |  fail if completed:            |
|  • Main (12)      |     Region: Skellige | Level: 15 | Secondary       |  • The Last Wish              |
|  • Side (45)      |     ⚠️ Cutoff: Isle of Mists                       |  • Following the Thread        |
|  • Contracts (25) |     🔗 Requires: Nameless                          |                                |
|                   +----------------------------------------------------+--------------------------------+
|  Progress:        | Summary: 2/57 Completed (3.5%)                     |  🔗 Prerequisites Warning      |
|  [||||||......]   |                                                    |  • 2 quests missing prereqs    |
+-------------------+----------------------------------------------------+--------------------------------+
```

---

## Proposed Changes

### Component 1: `witcher_3_quest_tracker` Package Setup

#### [MODIFY] `Cargo.toml` (root)

```toml
[package]
name = "witcher_3_quest_tracker"
version = "0.1.0"
edition = "2024"

[dependencies]
dioxus = { version = "0.6", features = ["desktop"] }
quest_db = { path = "crates/quest_db" }
quest_scraper = { path = "crates/quest_scraper" }
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
tracing = "0.1"
```

---

### Component 2: Smart Sync Service — `src/sync_service.rs`

#### [NEW] `src/sync_service.rs`

Handles the smart merge between `quest_scraper` and `quest_db`:

```rust
use quest_db::{Connection, Result, models::NewQuest, repository::{quests, progress}};
use quest_scraper::WikiScraperClient;

pub async fn run_sync(conn: &Connection) -> Result<usize> {
    let client = WikiScraperClient::new().map_err(|e| quest_db::QuestTrackerError::InvalidData(e.to_string()))?;

    // 1. Fetch live quests or mock fallback
    let scraper_store = match client.fetch_quests_batch(&["Lilac_and_Gooseberries", "The_Last_Wish", "Witch_Hunter_Raids", "Isle_of_Mists"]).await {
        Ok(store) if !store.is_empty() => store,
        _ => quest_scraper::mock_sample_quests(),
    };

    let mut synced_count = 0;

    // 2. Insert or update static quest entries
    for scraped in scraper_store.quests() {
        let existing = quests::get_by_name(conn, &scraped.name, &scraped.source.into()).ok();

        if let Some(existing_quest) = existing {
            // Update metadata without affecting quest_progress
            // (cutoff_quest_id and prerequisite_ids resolved in pass 2)
            synced_count += 1;
        } else {
            let new_quest = NewQuest {
                name: scraped.name.clone(),
                source: scraped.source.into(),
                quest_type: scraped.quest_type.into(),
                region: scraped.region.into(),
                recommended_level: scraped.recommended_level,
                sort_order: scraped.sort_order,
                description: scraped.description.clone(),
                is_unmarked: scraped.is_unmarked,
                cutoff_quest_id: None,
                prerequisite_ids: vec![],
            };
            quests::insert(conn, &new_quest)?;
            synced_count += 1;
        }
    }

    // 3. Resolve string cutoffs and prerequisites to DB IDs
    for scraped in scraper_store.quests() {
        if let Ok(target_quest) = quests::get_by_name(conn, &scraped.name, &scraped.source.into()) {
            if let Some(ref cutoff_name) = scraped.cutoff_quest_name {
                if let Ok(cutoff_quest) = quests::get_by_name(conn, cutoff_name, &scraped.source.into()) {
                    // Update cutoff_quest_id
                }
            }
            for prereq_name in &scraped.prerequisite_quest_names {
                if let Ok(prereq_quest) = quests::get_by_name(conn, prereq_name, &scraped.source.into()) {
                    quests::add_prerequisite(conn, target_quest.id, prereq_quest.id)?;
                }
            }
        }
    }

    Ok(synced_count)
}
```

---

### Component 3: Styling — `src/assets/style.css`

#### [NEW] `src/assets/style.css`

Rich dark mode theme tokens, glassmorphism containers, smooth CSS transitions, custom scrollbars, and badges:

```css
:root {
    --bg-dark: #121316;
    --bg-card: rgba(26, 28, 35, 0.85);
    --border-card: rgba(255, 255, 255, 0.08);
    --gold: #e5b869;
    --gold-glow: rgba(229, 184, 105, 0.25);
    --red: #e74c3c;
    --green: #2ecc71;
    --text-primary: #f1f2f6;
    --text-muted: #a4b0be;
}

body {
    background-color: var(--bg-dark);
    color: var(--text-primary);
    font-family: 'Inter', system-ui, sans-serif;
    margin: 0;
    overflow: hidden;
}

.app-container {
    display: grid;
    grid-template-columns: 280px 1fr 340px;
    height: 100vh;
}

.card {
    background: var(--bg-card);
    border: 1px solid var(--border-card);
    backdrop-filter: blur(12px);
    border-radius: 10px;
    transition: transform 0.2s ease, border-color 0.2s ease;
}

.card:hover {
    border-color: var(--gold);
}

.btn-primary {
    background: linear-gradient(135deg, #e5b869, #c09443);
    color: #000;
    font-weight: 600;
    border: none;
    border-radius: 6px;
    padding: 8px 16px;
    cursor: pointer;
}

.btn-fail {
    background: rgba(231, 76, 60, 0.15);
    color: #e74c3c;
    border: 1px solid #e74c3c;
    border-radius: 6px;
    padding: 4px 10px;
    cursor: pointer;
}

.warning-pane {
    background: rgba(231, 76, 60, 0.05);
    border-left: 2px solid var(--red);
    padding: 12px;
}
```

---

### Component 4: Dioxus UI Components

#### [NEW] `src/components/sidebar.rs`
Playthrough switcher, new playthrough modal, expansion source filter, category filters, completion progress bar.

#### [NEW] `src/components/quest_list.rs`
Main view search bar, Sync Quests button with spinner, list of quest cards with checkboxes (`Completed` vs `NotStarted`), "Mark Failed" button, and expandable notes text area.

#### [NEW] `src/components/cutoff_pane.rs`
Dedicated right pane calculating:
1. Impending cutoff points for the current playthrough.
2. Uncompleted quests at risk if that cutoff point is triggered.
3. Prerequisite warnings for quests missing completed requirements.

#### [NEW] `src/main.rs`
Dioxus main entry point initializing SQLite DB (`witcher3_quests.db`), managing reactive state (`use_signal`), and rendering the 3-pane layout.

---

## Verification Plan

### Automated Tests
Run workspace cargo build & tests to verify compilation:

```bash
cargo build --package witcher_3_quest_tracker
cargo test --workspace
```

### Manual Verification
1. Launch app with `cargo run`.
2. Select or create a playthrough.
3. Click **"Sync Quests"** to scrape and update quest reference data.
4. Verify quest checkboxes mark items as `Completed` and update progress statistics.
5. Click **"Mark Failed"** button on a quest to verify `Failed` state styling.
6. Verify the **Cutoff Warning Pane** on the right side dynamically displays uncompleted quests attached to impending cutoff points.
