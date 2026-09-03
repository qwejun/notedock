-- Note bodies leave the local database.
--
-- The desktop webview now holds each note's body as a Yjs document, cached in
-- IndexedDB and synced over the note WebSocket. So this table keeps only what the
-- window needs in order to *list* notes while offline, and `dirty` narrows from
-- "the body has unsent edits" to "this note's creation or deletion has not
-- reached the server yet".
--
-- `base_rev` goes with it: there is no longer a body revision to compare, because
-- there is no longer a body write that could lose a race.
--
-- SQLite cannot drop a column that an index or a partial-index predicate names,
-- and the old `notes_dirty` index has `WHERE dirty = 1`. Rebuilding the table is
-- both simpler and the only way to be sure of the resulting schema.
CREATE TABLE notes_new (
    id           TEXT    PRIMARY KEY NOT NULL,
    title        TEXT    NOT NULL DEFAULT '',
    -- Leading plain text for the list, as the server derived it.
    preview      TEXT    NOT NULL DEFAULT '',
    rev          INTEGER NOT NULL DEFAULT 0,
    updated_at   TEXT    NOT NULL,
    deleted      INTEGER NOT NULL DEFAULT 0,
    dirty        INTEGER NOT NULL DEFAULT 0
);

-- Existing rows keep their identity and their derived text. Bodies are not
-- migrated: an installed client re-downloads them from the server's Yjs log the
-- first time each note is opened, which is also the only way to get a document
-- the collaborative editor can merge into.
INSERT INTO notes_new (id, title, preview, rev, updated_at, deleted, dirty)
SELECT id, title, substr(content_text, 1, 120), rev, updated_at, deleted, dirty
FROM notes;

DROP TABLE notes;
ALTER TABLE notes_new RENAME TO notes;

CREATE INDEX notes_updated_at ON notes (updated_at DESC);
CREATE INDEX notes_dirty ON notes (dirty) WHERE dirty = 1;
