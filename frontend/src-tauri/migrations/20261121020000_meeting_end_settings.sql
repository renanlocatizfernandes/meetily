-- Automatic detection of the end of a meeting.
-- mode: 'off', 'ask' (warn, then stop if nobody answers) or 'auto' (stop).
-- silence_minutes: minutes without speech on the microphone and the system
-- audio that count as the end. prompt_seconds: how long the warning waits.

CREATE TABLE IF NOT EXISTS meeting_end_settings (
    id TEXT PRIMARY KEY NOT NULL,
    mode TEXT NOT NULL DEFAULT 'ask',
    silence_minutes INTEGER NOT NULL DEFAULT 3,
    prompt_seconds INTEGER NOT NULL DEFAULT 60
);

INSERT INTO meeting_end_settings (id, mode, silence_minutes, prompt_seconds)
VALUES ('1', 'ask', 3, 60)
ON CONFLICT(id) DO NOTHING;
