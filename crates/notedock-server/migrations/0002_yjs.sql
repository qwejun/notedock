-- Note bodies move into Yjs documents.
--
-- A note's body is no longer a JSON column that clients overwrite; it is the
-- result of applying every row here in order. That is what lets two devices edit
-- the same paragraph and converge without anyone resolving a conflict.
--
-- `notes.content_json` and `notes.content_text` stay, but as *derived* snapshots
-- the server materializes from the document. They are what the list preview, the
-- title rule and a future FTS index read, so those stay server-side and single-
-- sourced. `notes.rev` also stays, now meaning "metadata version" rather than
-- body version.
CREATE TABLE note_updates (
    id      INTEGER PRIMARY KEY AUTOINCREMENT,
    note_id TEXT    NOT NULL REFERENCES notes (id),
    -- A yrs v1 update. Many small rows during editing; compacted into one row
    -- once a note accumulates too many.
    data    BLOB    NOT NULL,
    at      TEXT    NOT NULL
);

CREATE INDEX note_updates_note ON note_updates (note_id, id);

-- The JSON snapshot goes. Nothing reads it: clients get the body from the
-- document, and previews and search read `content_text`. Leaving it would be
-- worse than removing it — the column is `NOT NULL` with no default, so any
-- insert that ignored it would fail.
--
-- Mapping a Yjs XML tree back to TipTap JSON server-side would mean a second,
-- subtly different renderer to maintain. Export, if it ever ships, belongs on the
-- client, where TipTap already lives.
ALTER TABLE notes DROP COLUMN content_json;
