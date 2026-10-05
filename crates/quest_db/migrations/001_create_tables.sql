-- Migration 001: Create core tables for Witcher 3 quest tracking
--
-- Four normalized tables:
--   playthroughs        — a named game run (difficulty, NG+ flag, notes)
--   quests              — static reference data for every quest in the game
--   quest_prerequisites — join table for many-to-many quest prerequisites
--   quest_progress      — per-playthrough progress tracking (status, timestamps, notes)

CREATE TABLE IF NOT EXISTS playthroughs (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    name              TEXT NOT NULL,
    difficulty        TEXT NOT NULL CHECK(difficulty IN (
                          'JustTheStory','StoryAndSword','BloodAndBrokenBones','DeathMarch','Custom'
                      )),
    is_new_game_plus  INTEGER NOT NULL DEFAULT 0 CHECK(is_new_game_plus IN (0,1)),
    notes             TEXT,
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS quests (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    name                  TEXT NOT NULL,
    source                TEXT NOT NULL CHECK(source IN ('BaseGame','HeartsOfStone','BloodAndWine')),
    quest_type            TEXT NOT NULL CHECK(quest_type IN (
                              'MainQuest','SecondaryQuest','WitcherContract','TreasureHunt','ScavengerHunt'
                          )),
    region                TEXT NOT NULL CHECK(region IN (
                              'WhiteOrchard','Velen','Novigrad','Skellige',
                              'KaerMorhen','Toussaint','OxenFurtSewers','Unknown'
                          )),
    recommended_level     INTEGER,
    sort_order            INTEGER,
    description           TEXT,
    is_unmarked           INTEGER NOT NULL DEFAULT 0 CHECK(is_unmarked IN (0,1)),
    cutoff_quest_id       INTEGER REFERENCES quests(id) ON DELETE SET NULL,
    created_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at            TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_quests_name_source ON quests(name, source);
CREATE INDEX IF NOT EXISTS idx_quests_quest_type ON quests(quest_type);
CREATE INDEX IF NOT EXISTS idx_quests_source     ON quests(source);
CREATE INDEX IF NOT EXISTS idx_quests_region     ON quests(region);

CREATE TABLE IF NOT EXISTS quest_prerequisites (
    quest_id              INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    prerequisite_quest_id INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE,
    PRIMARY KEY (quest_id, prerequisite_quest_id),
    CHECK (quest_id != prerequisite_quest_id)
);

CREATE INDEX IF NOT EXISTS idx_quest_prerequisites_prereq ON quest_prerequisites(prerequisite_quest_id);

CREATE TABLE IF NOT EXISTS quest_progress (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs(id) ON DELETE CASCADE,
    quest_id        INTEGER NOT NULL REFERENCES quests(id)       ON DELETE CASCADE,
    status          TEXT NOT NULL DEFAULT 'NotStarted'
                        CHECK(status IN ('NotStarted','InProgress','Completed','Failed')),
    notes           TEXT,
    started_at      TEXT,   -- RFC-3339 UTC
    completed_at    TEXT,   -- RFC-3339 UTC
    created_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    updated_at      TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
    UNIQUE(playthrough_id, quest_id)
);

CREATE INDEX IF NOT EXISTS idx_quest_progress_playthrough ON quest_progress(playthrough_id);
CREATE INDEX IF NOT EXISTS idx_quest_progress_status      ON quest_progress(status);
CREATE INDEX IF NOT EXISTS idx_quest_progress_quest       ON quest_progress(quest_id);

-- Scoped triggers (only fire on data columns, not updated_at itself — prevents recursion)
CREATE TRIGGER IF NOT EXISTS playthroughs_updated_at
    AFTER UPDATE OF name, difficulty, is_new_game_plus, notes ON playthroughs FOR EACH ROW
BEGIN
    UPDATE playthroughs SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS quests_updated_at
    AFTER UPDATE OF name, source, quest_type, region, recommended_level,
                   is_unmarked, sort_order, description,
                   cutoff_quest_id ON quests FOR EACH ROW
BEGIN
    UPDATE quests SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = OLD.id;
END;

CREATE TRIGGER IF NOT EXISTS quest_progress_updated_at
    AFTER UPDATE OF status, notes, started_at, completed_at ON quest_progress FOR EACH ROW
BEGIN
    UPDATE quest_progress SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = OLD.id;
END;
