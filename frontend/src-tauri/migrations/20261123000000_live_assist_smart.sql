-- Live assist, proactive part: local alerts (question to the owner, decision,
-- task, possible contradiction) and the economy mode. Only adds a table.
-- Alerts cost nothing (text matching on this machine). `auto_answer` is the
-- only switch that makes a model call by itself and it starts off.
CREATE TABLE IF NOT EXISTS live_assist_smart_settings (
    id TEXT PRIMARY KEY NOT NULL,
    question_alerts INTEGER NOT NULL DEFAULT 1,
    decision_alerts INTEGER NOT NULL DEFAULT 1,
    task_alerts INTEGER NOT NULL DEFAULT 1,
    contradiction_alerts INTEGER NOT NULL DEFAULT 1,
    auto_answer INTEGER NOT NULL DEFAULT 0,
    economy TEXT NOT NULL DEFAULT 'auto'
);

INSERT INTO live_assist_smart_settings (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;
