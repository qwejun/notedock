-- Notes are never hard-deleted: `deleted_at` turns a row into a tombstone so
-- clients pulling /sync can drop their local copy. `rev` is bumped by the
-- server on every accepted write and is what optimistic concurrency compares.
CREATE TABLE notes (
    id           TEXT    PRIMARY KEY NOT NULL,
    title        TEXT    NOT NULL DEFAULT '',
    content_json TEXT    NOT NULL,
    content_text TEXT    NOT NULL DEFAULT '',
    rev          INTEGER NOT NULL DEFAULT 1,
    created_at   TEXT    NOT NULL,
    updated_at   TEXT    NOT NULL,
    deleted_at   TEXT
);

CREATE INDEX notes_updated_at ON notes (updated_at DESC);

-- Global, monotonic change log. A client remembers the highest `seq` it has
-- seen and asks for everything after it; this is the only ordering clients
-- trust, because wall-clock timestamps from multiple devices cannot be.
CREATE TABLE note_changes (
    seq     INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id TEXT    NOT NULL REFERENCES notes (id),
    rev     INTEGER NOT NULL,
    at      TEXT    NOT NULL
);

CREATE INDEX note_changes_note_id ON note_changes (note_id);

-- Only the SHA-256 of a bearer token is stored, so a database leak cannot be
-- replayed against the API.
CREATE TABLE sessions (
    token_hash TEXT PRIMARY KEY NOT NULL,
    label      TEXT,
    created_at TEXT NOT NULL,
    expires_at TEXT NOT NULL
);

CREATE INDEX sessions_expires_at ON sessions (expires_at);
