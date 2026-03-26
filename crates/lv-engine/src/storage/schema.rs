/// DuckDB schema SQL. Executed on database open to ensure tables exist.
pub const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS contexts (
    id              TEXT PRIMARY KEY,
    uri             TEXT NOT NULL,
    parent_uri      TEXT,
    level           INTEGER,
    is_leaf         BOOLEAN NOT NULL DEFAULT FALSE,
    context_type    TEXT NOT NULL,
    category        TEXT DEFAULT '',
    abstract_text   TEXT DEFAULT '',
    owner_account   TEXT NOT NULL DEFAULT 'default',
    owner_user      TEXT NOT NULL,
    owner_agent     TEXT,
    session_id      TEXT,
    active_count    INTEGER DEFAULT 0,
    meta            TEXT,
    vector          FLOAT[],
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (uri, level)
);

CREATE TABLE IF NOT EXISTS content (
    key             TEXT PRIMARY KEY,
    data            BLOB NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS sessions (
    session_id      TEXT PRIMARY KEY,
    owner_user      TEXT NOT NULL,
    owner_account   TEXT NOT NULL DEFAULT 'default',
    compression     TEXT,
    stats           TEXT,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS messages (
    id              INTEGER PRIMARY KEY,
    session_id      TEXT NOT NULL,
    role            TEXT NOT NULL,
    parts           TEXT NOT NULL,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS relations (
    id              TEXT PRIMARY KEY,
    reason          TEXT DEFAULT '',
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS relation_uris (
    relation_id     TEXT NOT NULL,
    uri             TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS usage_records (
    id              INTEGER PRIMARY KEY,
    session_id      TEXT NOT NULL,
    uri             TEXT NOT NULL,
    usage_type      TEXT NOT NULL,
    contribution    FLOAT DEFAULT 0.0,
    input           TEXT DEFAULT '',
    output          TEXT DEFAULT '',
    success         BOOLEAN DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE SEQUENCE IF NOT EXISTS messages_id_seq START 1;
CREATE SEQUENCE IF NOT EXISTS usage_records_id_seq START 1;
"#;
