-- Local mirror of the server's notes, plus the two columns that make offline
-- editing work: `dirty` marks a note whose local text has not reached the
-- server, and `base_rev` records which server revision that edit was made from
-- so the push can be checked for conflicts.
--
-- `rev = 0` means the note has never existed on the server — it was created
-- while offline and needs a POST rather than a PUT.
CREATE TABLE notes (
    id           TEXT    PRIMARY KEY NOT NULL,
    title        TEXT    NOT NULL DEFAULT '',
    content_json TEXT    NOT NULL,
    content_text TEXT    NOT NULL DEFAULT '',
    rev          INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT    NOT NULL,
    deleted      INTEGER NOT NULL DEFAULT 0,
    dirty        INTEGER NOT NULL DEFAULT 0,
    base_rev     INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX notes_updated_at ON notes (updated_at DESC);
-- The outbox is a query, not a table: everything still to push is `dirty = 1`.
CREATE INDEX notes_dirty ON notes (dirty) WHERE dirty = 1;

-- Small key/value bag. Holds the sync cursor; deliberately not the bearer token,
-- which lives in a separate file with tighter permissions.
CREATE TABLE state (
    k TEXT PRIMARY KEY NOT NULL,
    v TEXT NOT NULL
);
