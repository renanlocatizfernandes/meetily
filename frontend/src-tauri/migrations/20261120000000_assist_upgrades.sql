-- Live assist shortcut, automatic recording mode and the one-time privacy choice.

-- The global shortcut that opens the live assist panel is now a setting.
ALTER TABLE live_assist_settings
    ADD COLUMN shortcut TEXT NOT NULL DEFAULT 'CommandOrControl+Shift+L';

-- Automatic recording mode: 'off', 'ask' (confirm before recording) or 'auto'.
-- Existing installs move to 'ask'. A row that was already disabled stays off,
-- so `enabled = 0` and `mode = 'off'` keep meaning the same thing.
ALTER TABLE auto_record_settings
    ADD COLUMN mode TEXT NOT NULL DEFAULT 'ask';

UPDATE auto_record_settings SET mode = 'off' WHERE enabled = 0;

-- One-time privacy choice. Until it is 1, automatic recording behaves as 'off'
-- and the internet lookup as disabled, whatever the stored values say.
ALTER TABLE feature_toggles
    ADD COLUMN privacy_onboarding_done INTEGER NOT NULL DEFAULT 0;
