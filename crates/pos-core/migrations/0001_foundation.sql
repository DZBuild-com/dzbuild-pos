-- 0001 Foundation (milestone M0): app meta, settings, company, terminals, roles, users,
-- audit journal and backups log (spec §5.1). Never edit: add 0002_*.sql instead.

CREATE TABLE app_meta (
  key   TEXT PRIMARY KEY NOT NULL,
  value TEXT NOT NULL
) STRICT;

CREATE TABLE settings (
  key        TEXT PRIMARY KEY NOT NULL,
  value      TEXT NOT NULL CHECK (json_valid(value)),
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE company (
  id                INTEGER PRIMARY KEY CHECK (id = 1),
  name_ar           TEXT NOT NULL DEFAULT '',
  name_fr           TEXT NOT NULL DEFAULT '',
  name_en           TEXT NOT NULL DEFAULT '',
  legal_form        TEXT,
  capital           INTEGER CHECK (capital IS NULL OR capital >= 0),
  address           TEXT,
  wilaya            TEXT,
  phone             TEXT,
  email             TEXT,
  rc                TEXT,
  nif               TEXT, -- 15 or 18 digits: checked in Rust (company::validate, data pack nif_lengths)
  nis               TEXT,
  ai                TEXT,
  rib               TEXT,
  tax_regime        TEXT NOT NULL DEFAULT 'reel' CHECK (tax_regime IN ('reel', 'ifu')),
  price_mode        TEXT NOT NULL DEFAULT 'ttc' CHECK (price_mode IN ('ttc', 'ht')),
  logo              BLOB,
  receipt_header_ar TEXT,
  receipt_header_fr TEXT,
  receipt_header_en TEXT,
  receipt_footer_ar TEXT,
  receipt_footer_fr TEXT,
  receipt_footer_en TEXT,
  default_doc_lang  TEXT NOT NULL DEFAULT 'fr' CHECK (default_doc_lang IN ('ar', 'fr', 'en')),
  updated_at        INTEGER NOT NULL
) STRICT;

CREATE TABLE terminals (
  id           TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code         TEXT NOT NULL UNIQUE CHECK (length(code) BETWEEN 1 AND 8),
  name         TEXT NOT NULL,
  role         TEXT NOT NULL DEFAULT 'standalone' CHECK (role IN ('standalone', 'host', 'client')),
  last_seen_at INTEGER,
  paired_at    INTEGER,
  created_at   INTEGER NOT NULL
) STRICT;

CREATE TABLE roles (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT UNIQUE,
  name_ar     TEXT NOT NULL,
  name_fr     TEXT NOT NULL,
  name_en     TEXT NOT NULL,
  is_system   INTEGER NOT NULL DEFAULT 0 CHECK (is_system IN (0, 1)),
  permissions TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(permissions) AND json_type(permissions) = 'array'),
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE users (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  username          TEXT NOT NULL UNIQUE COLLATE NOCASE,
  display_name      TEXT NOT NULL,
  pin_hash          TEXT NOT NULL,
  role_id           TEXT NOT NULL REFERENCES roles (id),
  lang              TEXT NOT NULL DEFAULT 'fr' CHECK (lang IN ('ar', 'fr', 'en')),
  active            INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  discount_limit_bp INTEGER NOT NULL DEFAULT 0 CHECK (discount_limit_bp BETWEEN 0 AND 10000),
  failed_attempts   INTEGER NOT NULL DEFAULT 0,
  locked_until      INTEGER,
  last_login_at     INTEGER,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_users_role_id ON users (role_id);

CREATE TABLE audit_log (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  id          TEXT NOT NULL UNIQUE CHECK (length(id) = 36),
  at          INTEGER NOT NULL,
  user_id     TEXT,
  terminal_id TEXT,
  action      TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id   TEXT,
  payload     TEXT NOT NULL CHECK (json_valid(payload)),
  prev_hash   TEXT NOT NULL CHECK (length(prev_hash) = 64),
  hash        TEXT NOT NULL CHECK (length(hash) = 64)
) STRICT;
CREATE INDEX idx_audit_log_at ON audit_log (at);
CREATE INDEX idx_audit_log_user_id ON audit_log (user_id);
CREATE INDEX idx_audit_log_terminal_id ON audit_log (terminal_id);
CREATE INDEX idx_audit_log_entity ON audit_log (entity_type, entity_id);
CREATE INDEX idx_audit_log_action ON audit_log (action);

CREATE TRIGGER audit_log_no_update BEFORE UPDATE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TRIGGER audit_log_no_delete BEFORE DELETE ON audit_log
BEGIN
  SELECT RAISE(ABORT, 'audit_log is append-only');
END;

CREATE TABLE backups_log (
  id   TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  at   INTEGER NOT NULL,
  path TEXT NOT NULL,
  size INTEGER NOT NULL CHECK (size >= 0),
  kind TEXT NOT NULL CHECK (kind IN ('auto', 'manual', 'pre_migration', 'pre_restore')),
  ok   INTEGER NOT NULL CHECK (ok IN (0, 1))
) STRICT;
CREATE INDEX idx_backups_log_at ON backups_log (at);
