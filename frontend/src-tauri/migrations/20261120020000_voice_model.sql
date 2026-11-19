-- Switch for the optional neural voice model ("Quem está falando").
-- On by default; it only takes effect once the model file is downloaded.

CREATE TABLE IF NOT EXISTS voice_model_settings (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);

INSERT INTO voice_model_settings (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;
