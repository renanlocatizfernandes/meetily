-- Piloto automatico pos-reuniao. Desligado por padrao; cada etapa tem o seu
-- interruptor. O resultado de cada reuniao fica em autopilot_results: tarefas,
-- decisoes, pendencias, titulo e etiquetas sugeridos e o rascunho de e-mail
-- (so rascunho: nada e enviado).

CREATE TABLE IF NOT EXISTS autopilot_settings (
    id TEXT PRIMARY KEY NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 0,
    step_summary INTEGER NOT NULL DEFAULT 1,
    step_tasks INTEGER NOT NULL DEFAULT 1,
    step_title_tags INTEGER NOT NULL DEFAULT 1,
    step_email INTEGER NOT NULL DEFAULT 1,
    template_id TEXT NOT NULL DEFAULT 'standard_meeting'
);

INSERT INTO autopilot_settings (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;

CREATE TABLE IF NOT EXISTS autopilot_results (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    status TEXT NOT NULL,
    note TEXT,
    steps TEXT NOT NULL DEFAULT '{}',
    tasks TEXT NOT NULL DEFAULT '[]',
    decisions TEXT NOT NULL DEFAULT '[]',
    pending TEXT NOT NULL DEFAULT '[]',
    suggested_title TEXT,
    title_applied INTEGER NOT NULL DEFAULT 0,
    suggested_tags TEXT NOT NULL DEFAULT '[]',
    tags_applied INTEGER NOT NULL DEFAULT 0,
    email_subject TEXT,
    email_body TEXT,
    provider TEXT,
    model TEXT,
    leaves_machine INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
