-- Migration 004: items, recipes and the recipe planner
--
-- Items and recipes come from the game files on sync; New Game+ has its own recipe set. Text
-- (item names and game-sourced UI labels) is stored per localisation key and language. Planner
-- state and the inventory imported from saves belong to a playthrough.

CREATE TABLE IF NOT EXISTS items (
    id              TEXT    PRIMARY KEY,
    internal_name   TEXT    NOT NULL,
    name_key        TEXT    NOT NULL,
    category        TEXT    NOT NULL,
    quality         INTEGER NOT NULL DEFAULT 1,
    expansion       TEXT    CHECK (expansion IN ('HeartsOfStone', 'BloodAndWine')),
    in_base         INTEGER NOT NULL DEFAULT 0,
    in_plus         INTEGER NOT NULL DEFAULT 0,
    synced_at       TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE INDEX IF NOT EXISTS items_internal_name ON items (internal_name);

CREATE TABLE IF NOT EXISTS game_texts (
    key             TEXT    NOT NULL,
    language        TEXT    NOT NULL,
    text            TEXT    NOT NULL,
    PRIMARY KEY (key, language)
);

CREATE TABLE IF NOT EXISTS item_images (
    item_id         TEXT    PRIMARY KEY REFERENCES items (id) ON DELETE CASCADE,
    image           BLOB    NOT NULL,
    content_type    TEXT    NOT NULL,
    wiki_file       TEXT    NOT NULL,
    sha1            TEXT    NOT NULL,
    fetched_at      TEXT    NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
);

CREATE TABLE IF NOT EXISTS recipes (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    mode            TEXT    NOT NULL CHECK (mode IN ('Base', 'Plus')),
    product_id      TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    kind            TEXT    NOT NULL CHECK (kind IN ('Crafting', 'Alchemy', 'Enchanting')),
    level           TEXT,
    craftsman       TEXT,
    yield_qty       INTEGER NOT NULL DEFAULT 1,
    expansion       TEXT    CHECK (expansion IN ('HeartsOfStone', 'BloodAndWine')),
    UNIQUE (mode, product_id)
);

-- Variant 0 is the primary ingredient set; 1 and up are alternatives.
CREATE TABLE IF NOT EXISTS recipe_ingredients (
    recipe_id       INTEGER NOT NULL REFERENCES recipes (id) ON DELETE CASCADE,
    variant         INTEGER NOT NULL,
    position        INTEGER NOT NULL,
    item_id         TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    qty             INTEGER NOT NULL,
    PRIMARY KEY (recipe_id, variant, item_id)
);

-- Crafts the user plans to make, by product.
CREATE TABLE IF NOT EXISTS planner_queue (
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs (id) ON DELETE CASCADE,
    item_id         TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    qty             INTEGER NOT NULL DEFAULT 1,
    variant         INTEGER NOT NULL DEFAULT 0,
    position        INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (playthrough_id, item_id)
);

-- Ingredients the user marked as gathered regardless of the inventory.
CREATE TABLE IF NOT EXISTS planner_collected (
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs (id) ON DELETE CASCADE,
    item_id         TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    PRIMARY KEY (playthrough_id, item_id)
);

-- Craftable ingredients the user will craft rather than gather.
CREATE TABLE IF NOT EXISTS planner_craft_self (
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs (id) ON DELETE CASCADE,
    item_id         TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    PRIMARY KEY (playthrough_id, item_id)
);

-- Geralt's inventory as of the save in playthroughs.inventory_save_key.
CREATE TABLE IF NOT EXISTS inventory (
    playthrough_id  INTEGER NOT NULL REFERENCES playthroughs (id) ON DELETE CASCADE,
    item_id         TEXT    NOT NULL REFERENCES items (id) ON DELETE CASCADE,
    qty             INTEGER NOT NULL,
    PRIMARY KEY (playthrough_id, item_id)
);

ALTER TABLE playthroughs ADD COLUMN inventory_save_key INTEGER;
