-- Feature switches for live assist extras. Defaults are on, except translation
-- which stays "auto" (only when English or Spanish is detected).

CREATE TABLE IF NOT EXISTS feature_toggles (
    id TEXT PRIMARY KEY NOT NULL,
    internet_search INTEGER NOT NULL DEFAULT 1,
    auto_tasks INTEGER NOT NULL DEFAULT 1,
    meeting_memory INTEGER NOT NULL DEFAULT 1,
    calendar_prep INTEGER NOT NULL DEFAULT 1,
    voice_profiles INTEGER NOT NULL DEFAULT 1,
    commitment_alerts INTEGER NOT NULL DEFAULT 1,
    followup_email INTEGER NOT NULL DEFAULT 1,
    meeting_search INTEGER NOT NULL DEFAULT 1,
    scoreboard INTEGER NOT NULL DEFAULT 1,
    translation TEXT NOT NULL DEFAULT 'auto'
);

INSERT INTO feature_toggles (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;

CREATE TABLE IF NOT EXISTS lookup_cache (
    term TEXT PRIMARY KEY NOT NULL,
    extract TEXT NOT NULL,
    url TEXT NOT NULL,
    source TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS voice_profiles (
    name TEXT PRIMARY KEY NOT NULL,
    embedding TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
