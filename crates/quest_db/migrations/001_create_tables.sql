-- Migration 001: Create core tables for Witcher 3 quest tracking
--
-- Tables:
--   quests              — static quest data, owned by sync (game files + wiki enrichment)
--   quest_texts         — quest titles and journal descriptions in every game language
--   quest_prerequisites — join table for many-to-many quest prerequisites (from the wiki)
--   playthroughs        — a game run, optionally linked to a save-game lineage
--   quest_progress      — per-playthrough quest state: manual status, status read from saves, notes
--   ignored_lineages    — save-game playthroughs the user chose not to track

CREATE TABLE IF NOT EXISTS quests (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    journal_path      TEXT NOT NULL UNIQUE,  -- depot path of the quest's .journal file: the key saves use
    journal_guid      TEXT NOT NULL,
    base_name         TEXT NOT NULL,         -- internal name, e.g. 'Q001 Dream'
    source            TEXT NOT NULL CHECK(source IN ('BaseGame','HeartsOfStone','BloodAndWine')),
    quest_type        TEXT NOT NULL CHECK(quest_type IN (
                          'MainQuest','SecondaryQuest','WitcherContract','TreasureHunt','ScavengerHunt'
                      )),
    region            TEXT NOT NULL CHECK(region IN (
                          'WhiteOrchard','Velen','Novigrad','Oxenfurt','Skellige',
                          'KaerMorhen','Vizima','Toussaint','Unknown'
                      )),
    recommended_level INTEGER,
    sort_order        INTEGER,
    -- Wiki enrichment; NULL when no wiki page matched the quest. Not unique: the game can split
    -- one quest over several journal files (e.g. epilogue variants) that share a page.
    wiki_page_id      INTEGER,
    wiki_title        TEXT,
    important_notes   TEXT,                  -- missable-quest warnings, newline separated
    is_unmarked       INTEGER NOT NULL DEFAULT 0 CHECK(is_unmarked IN (0,1)),
    cutoff_quest_id   INTEGER REFERENCES quests(id) ON DELETE SET NULL,
    synced_at         TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS idx_quests_sort_order ON quests(sort_order);
CREATE INDEX IF NOT EXISTS idx_quests_wiki_page ON quests(wiki_page_id);

CREATE TABLE IF NOT EXISTS quest_texts (
    quest_id    INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    language    TEXT NOT NULL,               -- game language code: 'en', 'pl', 'esmx', ...
    title       TEXT NOT NULL,
    description TEXT,
    PRIMARY KEY (quest_id, language)
);

CREATE TABLE IF NOT EXISTS quest_prerequisites (
    quest_id              INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    prerequisite_quest_id INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    PRIMARY KEY (quest_id, prerequisite_quest_id),
    CHECK (quest_id != prerequisite_quest_id)
);

CREATE INDEX IF NOT EXISTS idx_quest_prerequisites_prereq ON quest_prerequisites(prerequisite_quest_id);

CREATE TABLE IF NOT EXISTS playthroughs (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    name                TEXT NOT NULL,
    difficulty          TEXT NOT NULL CHECK(difficulty IN (
                            'JustTheStory','StoryAndSword','BloodAndBrokenBones','DeathMarch','Custom'
                        )),
    is_new_game_plus    INTEGER NOT NULL DEFAULT 0 CHECK(is_new_game_plus IN (0,1)),
    notes               TEXT,
    -- Save-game link: the packed time of the run's new-game event, shared by all its saves.
    lineage_root        INTEGER UNIQUE,
    game_playthrough_id TEXT,                -- saveInfo.playthroughId (4.0+), informational
    started_at          TEXT,                -- new-game time, local, from the save
    -- The newest save applied to this playthrough.
    head_save_key       INTEGER,
    head_save_file      TEXT,
    head_saved_at       TEXT,                -- local time from the save
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS quest_progress (
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs(id) ON DELETE CASCADE,
    quest_id        INTEGER NOT NULL REFERENCES quests(id)       ON DELETE CASCADE,
    -- Set by the user; overrides save_status while not NULL.
    manual_status   TEXT CHECK(manual_status IN ('NotStarted','InProgress','Completed','Failed')),
    -- Read from the playthrough's head save.
    save_status     TEXT CHECK(save_status IN ('NotStarted','InProgress','Completed','Failed')),
    notes           TEXT,
    completed_at    TEXT,   -- RFC-3339 UTC: when the effective status became Completed
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    PRIMARY KEY (playthrough_id, quest_id)
);

CREATE INDEX IF NOT EXISTS idx_quest_progress_quest ON quest_progress(quest_id);

CREATE TABLE IF NOT EXISTS ignored_lineages (
    lineage_root INTEGER PRIMARY KEY,
    ignored_at   TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

-- Scoped triggers (only fire on data columns, not updated_at itself — prevents recursion)
CREATE TRIGGER IF NOT EXISTS playthroughs_updated_at
    AFTER UPDATE OF name, difficulty, is_new_game_plus, notes, lineage_root ON playthroughs FOR EACH ROW
BEGIN
    UPDATE playthroughs SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS quest_progress_updated_at
    AFTER UPDATE OF manual_status, save_status, notes ON quest_progress FOR EACH ROW
BEGIN
    UPDATE quest_progress SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')
    WHERE playthrough_id = OLD.playthrough_id AND quest_id = OLD.quest_id;
END;
