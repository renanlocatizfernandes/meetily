-- Live assist context: the owner's persistent profile and glossary, and the
-- switch that lets the live prompt use notes from earlier meetings of the same
-- client. Kept apart from `live_assist_settings.meeting_context`, which is the
-- free text for one meeting. Only adds a table.
CREATE TABLE IF NOT EXISTS live_assist_context_settings (
    id TEXT PRIMARY KEY NOT NULL,
    profile_enabled INTEGER NOT NULL DEFAULT 1,
    about TEXT NOT NULL DEFAULT '',
    company TEXT NOT NULL DEFAULT '',
    offerings TEXT NOT NULL DEFAULT '',
    glossary TEXT NOT NULL DEFAULT '',
    use_past_meetings INTEGER NOT NULL DEFAULT 1
);

INSERT INTO live_assist_context_settings (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;
