-- Which neural voice model is in use ("Quem está falando"). The default is
-- the model that existed before this column: CAM++.

ALTER TABLE voice_model_settings ADD COLUMN model_id TEXT NOT NULL DEFAULT 'campplus-zh-en';
