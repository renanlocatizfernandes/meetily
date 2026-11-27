-- Tarefas persistentes, participantes, clientes com apelidos e domínios,
-- e a conexão com o Obsidian. Só acrescenta tabelas.

CREATE TABLE IF NOT EXISTS tasks (
    id TEXT PRIMARY KEY,
    meeting_id TEXT,
    text TEXT NOT NULL,
    owner TEXT,
    -- Prazo como foi dito ("até sexta") e, quando dá para calcular, a data.
    due TEXT,
    due_date TEXT,
    -- mine: você deve fazer. theirs: outra pessoa deve devolver. unassigned: sem dono claro.
    direction TEXT NOT NULL DEFAULT 'unassigned',
    status TEXT NOT NULL DEFAULT 'open',
    source TEXT NOT NULL DEFAULT 'auto',
    edited INTEGER NOT NULL DEFAULT 0,
    evidence TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    done_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_tasks_meeting ON tasks (meeting_id);
CREATE INDEX IF NOT EXISTS idx_tasks_status ON tasks (status);

CREATE TABLE IF NOT EXISTS task_runs (
    meeting_id TEXT PRIMARY KEY,
    -- model | autopilot | rules | empty
    origin TEXT NOT NULL,
    generated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS meeting_participants (
    id TEXT PRIMARY KEY,
    meeting_id TEXT NOT NULL,
    name TEXT NOT NULL,
    email TEXT,
    organization TEXT,
    -- calendar | teams | file | manual | speech
    source TEXT NOT NULL DEFAULT 'manual',
    speaker_key TEXT,
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_participants_meeting_name
    ON meeting_participants (meeting_id, name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS clients (
    name TEXT PRIMARY KEY COLLATE NOCASE,
    aliases TEXT NOT NULL DEFAULT '[]',
    domains TEXT NOT NULL DEFAULT '[]',
    notes TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS workspace_settings (
    id TEXT PRIMARY KEY,
    user_name TEXT NOT NULL DEFAULT '',
    user_aliases TEXT NOT NULL DEFAULT '[]',
    auto_tasks INTEGER NOT NULL DEFAULT 1,
    auto_participants INTEGER NOT NULL DEFAULT 1,
    obsidian_vault TEXT NOT NULL DEFAULT '',
    obsidian_folder TEXT NOT NULL DEFAULT 'Reuniões',
    obsidian_auto INTEGER NOT NULL DEFAULT 0,
    obsidian_transcript INTEGER NOT NULL DEFAULT 1,
    obsidian_links INTEGER NOT NULL DEFAULT 1
);
INSERT OR IGNORE INTO workspace_settings (id) VALUES ('1');

CREATE TABLE IF NOT EXISTS obsidian_notes (
    meeting_id TEXT PRIMARY KEY,
    path TEXT NOT NULL,
    exported_at TEXT NOT NULL
);
