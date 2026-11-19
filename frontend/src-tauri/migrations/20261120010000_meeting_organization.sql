-- Meeting organization: client, project, category and tags per meeting, plus
-- where the current title and classification came from.
--   title_source:          default (placeholder from recording) | auto | manual
--   classification_source: none | auto | manual
-- `subject` keeps the classifier's subject so an automatic title can be
-- rendered again when the client changes.

ALTER TABLE meetings ADD COLUMN client TEXT;
ALTER TABLE meetings ADD COLUMN project TEXT;
ALTER TABLE meetings ADD COLUMN category TEXT;
ALTER TABLE meetings ADD COLUMN tags TEXT;
ALTER TABLE meetings ADD COLUMN subject TEXT;
ALTER TABLE meetings ADD COLUMN title_source TEXT NOT NULL DEFAULT 'default';
ALTER TABLE meetings ADD COLUMN classification_source TEXT NOT NULL DEFAULT 'none';
ALTER TABLE meetings ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0;
ALTER TABLE meetings ADD COLUMN organized_at TEXT;

CREATE TABLE IF NOT EXISTS organization_settings (
    id TEXT PRIMARY KEY NOT NULL,
    auto_organize INTEGER NOT NULL DEFAULT 1,
    title_template TEXT NOT NULL DEFAULT '{cliente} · {assunto} · {data}',
    known_clients TEXT NOT NULL DEFAULT '[]',
    categories TEXT NOT NULL DEFAULT '["Cliente","Comercial","Interna","Suporte","Planejamento","Pessoal","Outros"]'
);

INSERT INTO organization_settings (id) VALUES ('1') ON CONFLICT(id) DO NOTHING;
