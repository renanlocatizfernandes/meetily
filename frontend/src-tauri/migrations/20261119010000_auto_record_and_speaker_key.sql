-- Automatic meeting recording and stable speaker keys.
-- Existing transcript rows keep a NULL speaker_key and are left unchanged.

CREATE TABLE IF NOT EXISTS auto_record_settings (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);

INSERT INTO auto_record_settings (id, enabled)
VALUES ('1', 1)
ON CONFLICT(id) DO NOTHING;

ALTER TABLE transcripts ADD COLUMN speaker_key TEXT;

CREATE TABLE IF NOT EXISTS speaker_aliases (
    meeting_id TEXT NOT NULL,
    speaker_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker_key)
);
