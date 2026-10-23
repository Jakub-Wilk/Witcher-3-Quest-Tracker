-- Migration 002: one quest row can stand for several journal files
--
-- The game sometimes splits one quest over several journal files: alternative epilogues, a
-- quest's second part, a separate intro. Such files are folded into one quest row; the row's
-- own journal_path is the main file and the others are listed here, so saves reporting any of
-- them update that row.

CREATE TABLE IF NOT EXISTS quest_journals (
    journal_path TEXT PRIMARY KEY,
    quest_id     INTEGER NOT NULL REFERENCES quests(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_quest_journals_quest ON quest_journals(quest_id);
