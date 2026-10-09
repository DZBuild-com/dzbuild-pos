-- 0007 LAN (milestone M5, spec §4.6, §5.1 "M5 LAN / M7 cloud"). Never edit: add a new file.
-- Numbered after M7's 0006_dzbuild_link.sql. M4's 0005_hardware.sql lives on another branch and
-- is ordered before 0006 at the merge; this file stays last.
-- The change feed is shared with the DZBuild link (M7): changelog and sync_cursors are created by
-- 0006 with the DDL both features use, so they are not repeated here (the M5 plan's "M7 merged
-- before M5" case). M5's capture and guard triggers are not here either: they are generated at
-- run time from the schema (pos_core::sync::capture) when this PC becomes a main PC or a till, so
-- a single PC carries none and a later migration's columns are always captured. Nothing here
-- stores a token, a pairing code or a PIN in clear. The pre_pairing backup kind is file-only:
-- backups_log keeps its 0001 check and never logs it.

-- This PC in the LAN: one row, never replicated (a snapshot resets it), never deleted.
CREATE TABLE lan_local (
  id              INTEGER PRIMARY KEY CHECK (id = 1),
  role            TEXT NOT NULL DEFAULT 'standalone' CHECK (role IN ('standalone', 'host', 'client')),
  host_id         TEXT CHECK (host_id IS NULL OR length(host_id) = 36),
  host_name       TEXT,
  host_address    TEXT CHECK (host_address IS NULL OR length(host_address) BETWEEN 3 AND 80),
  applied_seq     INTEGER NOT NULL DEFAULT 0 CHECK (applied_seq >= 0),
  paired_at       INTEGER,
  last_contact_at INTEGER,
  skew_ms         INTEGER,
  host_schema     INTEGER,
  updated_at      INTEGER NOT NULL,
  CHECK (role <> 'client' OR (host_id IS NOT NULL AND host_address IS NOT NULL AND paired_at IS NOT NULL)),
  CHECK (role <> 'host' OR host_id IS NOT NULL)
) STRICT;
INSERT INTO lan_local (id, role, updated_at) VALUES (1, 'standalone', 0);

CREATE TRIGGER lan_local_kept BEFORE DELETE ON lan_local
BEGIN
  SELECT RAISE(ABORT, 'lan_local keeps its row');
END;

-- Till: one row per local business write, replayed to the main PC in local_seq order.
CREATE TABLE lan_outbox (
  local_seq       INTEGER PRIMARY KEY AUTOINCREMENT,
  id              TEXT NOT NULL UNIQUE CHECK (length(id) = 36),
  idempotency_key TEXT NOT NULL UNIQUE CHECK (length(idempotency_key) BETWEEN 1 AND 80
                    AND idempotency_key NOT GLOB '*[^A-Za-z0-9_:.-]*'),
  terminal_id     TEXT NOT NULL CHECK (length(terminal_id) = 36),
  kind            TEXT NOT NULL CHECK (length(kind) BETWEEN 1 AND 64),
  payload         TEXT NOT NULL CHECK (json_valid(payload)),
  rows            INTEGER NOT NULL CHECK (rows >= 0),
  status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'rejected')),
  attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  last_error      TEXT,
  host_id         TEXT CHECK (host_id IS NULL OR length(host_id) = 36),
  created_at      INTEGER NOT NULL,
  sent_at         INTEGER,
  CHECK ((status = 'sent') = (sent_at IS NOT NULL AND host_id IS NOT NULL))
) STRICT;
CREATE INDEX idx_lan_outbox_status ON lan_outbox (status, local_seq);
CREATE INDEX idx_lan_outbox_sent_at ON lan_outbox (sent_at);

CREATE TRIGGER lan_outbox_frozen
BEFORE UPDATE OF id, idempotency_key, terminal_id, kind, payload, rows, created_at ON lan_outbox
BEGIN
  SELECT RAISE(ABORT, 'an outbox item is frozen');
END;

CREATE TRIGGER lan_outbox_sent_is_final BEFORE UPDATE OF status ON lan_outbox
WHEN OLD.status = 'sent' AND NEW.status <> 'sent'
BEGIN
  SELECT RAISE(ABORT, 'a sent outbox item stays sent');
END;

CREATE TRIGGER lan_outbox_kept BEFORE DELETE ON lan_outbox
WHEN OLD.status <> 'sent'
BEGIN
  SELECT RAISE(ABORT, 'unsent outbox items are kept');
END;

-- Host: the tills paired with this main PC. Tokens only as SHA-256; revoked, never deleted.
CREATE TABLE lan_devices (
  terminal_id         TEXT PRIMARY KEY NOT NULL REFERENCES terminals (id),
  name                TEXT NOT NULL CHECK (length(trim(name)) BETWEEN 1 AND 40),
  token_hash          TEXT NOT NULL UNIQUE CHECK (length(token_hash) = 64
                        AND token_hash NOT GLOB '*[^0-9a-f]*'),
  paired_at           INTEGER NOT NULL,
  paired_by           TEXT,
  revoked_at          INTEGER,
  revoked_by          TEXT,
  last_seen_at        INTEGER,
  last_address        TEXT,
  app_version         TEXT,
  schema_version      INTEGER,
  outbox_pending      INTEGER NOT NULL DEFAULT 0 CHECK (outbox_pending >= 0),
  skew_ms             INTEGER,
  last_replayed_seq   INTEGER CHECK (last_replayed_seq IS NULL OR last_replayed_seq >= 1),
  journal_anchor_seq  INTEGER NOT NULL DEFAULT 0 CHECK (journal_anchor_seq >= 0),
  journal_anchor_hash TEXT NOT NULL
                      DEFAULT '0000000000000000000000000000000000000000000000000000000000000000'
                      CHECK (length(journal_anchor_hash) = 64),
  CHECK ((revoked_at IS NULL) = (revoked_by IS NULL))
) STRICT;

CREATE TRIGGER lan_devices_kept BEFORE DELETE ON lan_devices
BEGIN
  SELECT RAISE(ABORT, 'paired tills are revoked, never deleted');
END;

-- Host: every replayed item and the answer given (a retry gets the same answer).
CREATE TABLE lan_inbox (
  idempotency_key   TEXT PRIMARY KEY NOT NULL,
  terminal_id       TEXT NOT NULL CHECK (length(terminal_id) = 36),
  local_seq         INTEGER NOT NULL CHECK (local_seq >= 1),
  kind              TEXT NOT NULL,
  client_created_at INTEGER NOT NULL,
  received_at       INTEGER NOT NULL,
  skew_ms           INTEGER,
  result            TEXT NOT NULL CHECK (json_valid(result)),
  host_seq          INTEGER NOT NULL CHECK (host_seq >= 0)
) STRICT;
CREATE INDEX idx_lan_inbox_terminal ON lan_inbox (terminal_id, local_seq);

CREATE TRIGGER lan_inbox_no_update BEFORE UPDATE ON lan_inbox
BEGIN
  SELECT RAISE(ABORT, 'lan_inbox is append-only');
END;

CREATE TRIGGER lan_inbox_no_delete BEFORE DELETE ON lan_inbox
BEGIN
  SELECT RAISE(ABORT, 'lan_inbox is append-only');
END;

-- Host: each till's audit journal as the till wrote it (its own chain, checked on arrival),
-- chained again by the host in the order it received them (merge_prev/merge_hash).
CREATE TABLE lan_journal (
  merge_seq   INTEGER PRIMARY KEY AUTOINCREMENT,
  terminal_id TEXT NOT NULL CHECK (length(terminal_id) = 36),
  seq         INTEGER NOT NULL CHECK (seq >= 1),
  id          TEXT NOT NULL UNIQUE CHECK (length(id) = 36),
  at          INTEGER NOT NULL,
  user_id     TEXT,
  action      TEXT NOT NULL,
  entity_type TEXT NOT NULL,
  entity_id   TEXT,
  payload     TEXT NOT NULL CHECK (json_valid(payload)),
  prev_hash   TEXT NOT NULL CHECK (length(prev_hash) = 64),
  hash        TEXT NOT NULL CHECK (length(hash) = 64),
  intact      INTEGER NOT NULL CHECK (intact IN (0, 1)),
  received_at INTEGER NOT NULL,
  merge_prev  TEXT NOT NULL CHECK (length(merge_prev) = 64),
  merge_hash  TEXT NOT NULL CHECK (length(merge_hash) = 64),
  UNIQUE (terminal_id, seq)
) STRICT;
CREATE INDEX idx_lan_journal_action ON lan_journal (action);

CREATE TRIGGER lan_journal_no_update BEFORE UPDATE ON lan_journal
BEGIN
  SELECT RAISE(ABORT, 'lan_journal is append-only');
END;

CREATE TRIGGER lan_journal_no_delete BEFORE DELETE ON lan_journal
BEGIN
  SELECT RAISE(ABORT, 'lan_journal is append-only');
END;

-- Host: the last change-feed seq of each committed transaction, so a till applies whole host
-- transactions (pruned with the feed).
CREATE TABLE lan_commits (
  last_seq INTEGER PRIMARY KEY NOT NULL CHECK (last_seq >= 1)
) STRICT;

-- Host: what the owner must see (oversold, over the credit limit, a rebased chain, a broken
-- till journal, an offline Z, a skewed clock, a refused replay, a version to update).
CREATE TABLE lan_notices (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  kind            TEXT NOT NULL CHECK (kind IN ('oversold', 'credit_over_limit', 'store_credit_overdrawn',
                    'allocation_skipped', 'chain_rebased', 'journal_broken', 'offline_z', 'clock_skew',
                    'replay_rejected', 'version_mismatch', 'closure_unlinked')),
  terminal_id     TEXT,
  subject         TEXT NOT NULL CHECK (length(subject) BETWEEN 1 AND 128),
  details         TEXT NOT NULL CHECK (json_valid(details)),
  created_at      INTEGER NOT NULL,
  acknowledged_at INTEGER,
  acknowledged_by TEXT,
  CHECK ((acknowledged_at IS NULL) = (acknowledged_by IS NULL))
) STRICT;
CREATE UNIQUE INDEX idx_lan_notices_one_open ON lan_notices (kind, COALESCE(terminal_id, ''), subject)
  WHERE acknowledged_at IS NULL;
CREATE INDEX idx_lan_notices_created_at ON lan_notices (created_at);

CREATE TRIGGER lan_notices_only_acknowledged
BEFORE UPDATE OF id, kind, terminal_id, subject, details, created_at ON lan_notices
BEGIN
  SELECT RAISE(ABORT, 'a notice is only acknowledged');
END;

CREATE TRIGGER lan_notices_kept BEFORE DELETE ON lan_notices
BEGIN
  SELECT RAISE(ABORT, 'notices are kept');
END;
