-- Migration 003: playthroughs no longer record a difficulty
--
-- It was only ever a label. The updated_at trigger names the column, so it is recreated
-- around the drop.

DROP TRIGGER IF EXISTS playthroughs_updated_at;

ALTER TABLE playthroughs DROP COLUMN difficulty;

CREATE TRIGGER IF NOT EXISTS playthroughs_updated_at
    AFTER UPDATE OF name, is_new_game_plus, notes, lineage_root ON playthroughs FOR EACH ROW
BEGIN
    UPDATE playthroughs SET updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE id = OLD.id;
END;
