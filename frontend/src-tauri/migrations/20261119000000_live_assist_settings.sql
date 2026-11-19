-- Live Assist preferences. Separate from the summary `settings` row so the
-- post-meeting model configuration is left untouched.
CREATE TABLE IF NOT EXISTS live_assist_settings (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    model TEXT NOT NULL DEFAULT 'llama3.2:latest',
    language TEXT NOT NULL DEFAULT 'pt-BR',
    frequency TEXT NOT NULL DEFAULT 'media',
    meeting_context TEXT NOT NULL DEFAULT ''
);

INSERT INTO live_assist_settings (id, enabled, model, language, frequency, meeting_context)
VALUES ('1', 1, 'llama3.2:latest', 'pt-BR', 'media', '')
ON CONFLICT(id) DO NOTHING;
