-- 0006 DZBuild link (milestone M7, spec §4.7, §5.1 "M5 LAN / M7 cloud"). Never edit: add a new file.
-- The change feed is shared with the LAN sync (M5): triggers write it only while a reader is
-- registered in sync_cursors (kind 'changelog'), so a shop with no link and no LAN writes nothing.
-- M5 reuses the changelog and sync_cursors DDL below byte for byte (its readers register as
-- 'changelog:lan:<terminal>'); it adds triggers for its own tables in its own migration.
-- Payloads carry routing fields only; readers load the current row by row_id.
-- No token, secret or password is ever stored here (tokens: Windows Credential Manager).

-- Change feed ------------------------------------------------------------------------------

CREATE TABLE changelog (
  seq        INTEGER PRIMARY KEY AUTOINCREMENT,
  at         INTEGER NOT NULL,
  table_name TEXT NOT NULL CHECK (length(table_name) BETWEEN 1 AND 64),
  row_id     TEXT NOT NULL CHECK (length(row_id) BETWEEN 1 AND 128),
  op         TEXT NOT NULL CHECK (op IN ('insert', 'update', 'delete')),
  payload    TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload))
) STRICT;
CREATE INDEX idx_changelog_table ON changelog (table_name, seq);

CREATE TRIGGER changelog_append_only BEFORE UPDATE ON changelog
BEGIN
  SELECT RAISE(ABORT, 'the change feed is append-only');
END;

CREATE TABLE sync_cursors (
  name       TEXT PRIMARY KEY NOT NULL CHECK (length(name) BETWEEN 1 AND 64),
  kind       TEXT NOT NULL CHECK (kind IN ('changelog', 'remote')),
  last_seq   INTEGER CHECK (last_seq IS NULL OR last_seq >= 0),
  cursor     TEXT,
  updated_at INTEGER NOT NULL,
  CHECK (kind <> 'changelog' OR last_seq IS NOT NULL)
) STRICT;
CREATE INDEX idx_sync_cursors_kind ON sync_cursors (kind);

CREATE TRIGGER changelog_products_insert AFTER INSERT ON products
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'products', NEW.id, 'insert');
END;

CREATE TRIGGER changelog_products_update AFTER UPDATE ON products
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'products', NEW.id, 'update');
END;

CREATE TRIGGER changelog_barcodes_insert AFTER INSERT ON product_barcodes
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'product_barcodes', NEW.id, 'insert',
          json_object('product_id', NEW.product_id));
END;

CREATE TRIGGER changelog_barcodes_update AFTER UPDATE ON product_barcodes
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'product_barcodes', NEW.id, 'update',
          json_object('product_id', NEW.product_id));
END;

CREATE TRIGGER changelog_barcodes_delete AFTER DELETE ON product_barcodes
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'product_barcodes', OLD.id, 'delete',
          json_object('product_id', OLD.product_id));
END;

CREATE TRIGGER changelog_categories_update AFTER UPDATE ON categories
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'categories', NEW.id, 'update');
END;

CREATE TRIGGER changelog_stock_movements_insert AFTER INSERT ON stock_movements
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'stock_movements', NEW.id, 'insert',
          json_object('product_id', NEW.product_id, 'reason', NEW.reason,
                      'ref_type', NEW.ref_type, 'ref_id', NEW.ref_id));
END;

CREATE TRIGGER changelog_documents_validated AFTER UPDATE OF status ON documents
WHEN OLD.status = 'draft' AND NEW.status = 'validated'
  AND EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'documents', NEW.id, 'update',
          json_object('doc_type', NEW.doc_type, 'parent_id', NEW.parent_id));
END;

CREATE TRIGGER changelog_cash_closures_insert AFTER INSERT ON cash_closures
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op, payload)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'cash_closures', NEW.session_id, 'insert',
          json_object('terminal_id', NEW.terminal_id));
END;

-- DZBuild link -----------------------------------------------------------------------------

CREATE TABLE cloud_links (
  id                  TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  account_sub         TEXT NOT NULL CHECK (length(account_sub) BETWEEN 1 AND 128),
  account_name        TEXT,
  store_id            TEXT NOT NULL CHECK (length(store_id) BETWEEN 1 AND 64),
  store_name          TEXT NOT NULL,
  store_language      TEXT NOT NULL DEFAULT 'ar' CHECK (store_language IN ('ar', 'fr')),
  store_plan          TEXT,
  public_url          TEXT,
  location_id         TEXT,
  device_id           TEXT,
  scopes              TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(scopes) AND json_type(scopes) = 'array'),
  stock_deduction     TEXT CHECK (stock_deduction IS NULL
                        OR stock_deduction IN ('on_confirm', 'on_create', 'on_delivery')),
  active_products_cap INTEGER CHECK (active_products_cap IS NULL OR active_products_cap >= 0),
  status              TEXT NOT NULL CHECK (status IN ('linked', 'relink_required', 'revoked', 'unlinked')),
  linked_at           INTEGER NOT NULL,
  last_sync_at        INTEGER,
  unlinked_at         INTEGER,
  active              INTEGER GENERATED ALWAYS AS (CASE WHEN status <> 'unlinked' THEN 1 END) VIRTUAL,
  CHECK ((status = 'unlinked') = (unlinked_at IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX idx_cloud_links_one_active ON cloud_links (active);
CREATE INDEX idx_cloud_links_store_id ON cloud_links (store_id);

CREATE TRIGGER cloud_links_kept BEFORE DELETE ON cloud_links
BEGIN
  SELECT RAISE(ABORT, 'links are kept for the record');
END;

CREATE TABLE cloud_map (
  store_id     TEXT NOT NULL,
  entity_type  TEXT NOT NULL CHECK (entity_type IN ('product', 'category', 'sale', 'refund', 'closure',
                                                    'media', 'location', 'device')),
  local_id     TEXT NOT NULL,
  remote_id    TEXT NOT NULL,
  last_payload TEXT CHECK (last_payload IS NULL OR json_valid(last_payload)),
  last_hash    TEXT CHECK (last_hash IS NULL OR length(last_hash) = 64),
  updated_at   INTEGER NOT NULL,
  PRIMARY KEY (store_id, entity_type, local_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_cloud_map_remote ON cloud_map (store_id, entity_type, remote_id);

CREATE TABLE sync_outbox (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  target          TEXT NOT NULL CHECK (target IN ('host', 'cloud')),
  store_id        TEXT,
  kind            TEXT NOT NULL CHECK (length(kind) BETWEEN 1 AND 40),
  entity_id       TEXT NOT NULL,
  payload         TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(payload)),
  idempotency_key TEXT NOT NULL UNIQUE CHECK (length(idempotency_key) BETWEEN 1 AND 64
                    AND idempotency_key NOT GLOB '*[^A-Za-z0-9_:.-]*'),
  status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'sent', 'failed', 'skipped')),
  attempts        INTEGER NOT NULL DEFAULT 0 CHECK (attempts >= 0),
  next_attempt_at INTEGER NOT NULL DEFAULT 0,
  last_error      TEXT,
  error_details   TEXT CHECK (error_details IS NULL OR json_valid(error_details)),
  remote_id       TEXT,
  depends_on      TEXT REFERENCES sync_outbox (id) ON DELETE SET NULL,
  created_at      INTEGER NOT NULL,
  sent_at         INTEGER,
  CHECK ((status = 'sent') = (sent_at IS NOT NULL)),
  CHECK (target <> 'cloud' OR store_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_sync_outbox_due ON sync_outbox (target, status, next_attempt_at);
CREATE INDEX idx_sync_outbox_entity ON sync_outbox (target, kind, entity_id);
CREATE INDEX idx_sync_outbox_depends_on ON sync_outbox (depends_on);
CREATE INDEX idx_sync_outbox_sent_at ON sync_outbox (sent_at);
CREATE UNIQUE INDEX idx_sync_outbox_coalesce ON sync_outbox (target, kind, entity_id)
  WHERE status = 'pending' AND attempts = 0 AND kind IN ('product_upsert', 'media_upload');

CREATE TRIGGER sync_outbox_open_kept BEFORE DELETE ON sync_outbox
WHEN OLD.status IN ('pending', 'failed')
BEGIN
  SELECT RAISE(ABORT, 'pending and failed outbox items are kept');
END;

CREATE TRIGGER sync_outbox_key_frozen BEFORE UPDATE OF idempotency_key ON sync_outbox
WHEN OLD.status = 'pending' AND OLD.attempts > 0
BEGIN
  SELECT RAISE(ABORT, 'an attempted item keeps its idempotency key');
END;

CREATE TRIGGER sync_outbox_payload_frozen BEFORE UPDATE OF payload ON sync_outbox
WHEN OLD.status = 'pending' AND OLD.attempts > 0
BEGIN
  SELECT RAISE(ABORT, 'an attempted item keeps its payload');
END;

-- depends_on: an item waits for another one (a refund for its sale) until that one is sent. A
-- sent item has met the dependency, so pruning it 30 days on releases its dependants (ON DELETE
-- SET NULL: the prune never fails on a kept refund). A skipped item never reaches DZBuild, so its
-- open dependants are skipped with it: none waits forever, none is sent once the skipped item is
-- pruned. (recursive_triggers is on: the skip reaches dependants of dependants.)
CREATE TRIGGER sync_outbox_skip_dependants AFTER UPDATE OF status ON sync_outbox
WHEN NEW.status = 'skipped' AND OLD.status <> 'skipped'
BEGIN
  UPDATE sync_outbox SET status = 'skipped', last_error = 'parent_skipped'
  WHERE depends_on = NEW.id AND status IN ('pending', 'failed');
END;

CREATE TABLE cloud_stock (
  store_id        TEXT NOT NULL,
  product_id      TEXT NOT NULL REFERENCES products (id),
  confirmed       INTEGER,
  set_in_flight   INTEGER,
  in_flight       INTEGER NOT NULL DEFAULT 0,
  reset_requested INTEGER NOT NULL DEFAULT 0 CHECK (reset_requested IN (0, 1)),
  updated_at      INTEGER NOT NULL,
  PRIMARY KEY (store_id, product_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_cloud_stock_product_id ON cloud_stock (product_id);

CREATE TABLE cloud_publish (
  product_id    TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  publish       INTEGER NOT NULL CHECK (publish IN (0, 1)),
  online_status TEXT NOT NULL DEFAULT 'active' CHECK (online_status IN ('active', 'draft')),
  updated_at    INTEGER NOT NULL
) STRICT;

CREATE TRIGGER changelog_cloud_publish_insert AFTER INSERT ON cloud_publish
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'cloud_publish', NEW.product_id, 'insert');
END;

CREATE TRIGGER changelog_cloud_publish_update AFTER UPDATE ON cloud_publish
WHEN EXISTS (SELECT 1 FROM sync_cursors WHERE kind = 'changelog')
BEGIN
  INSERT INTO changelog (at, table_name, row_id, op)
  VALUES (CAST(unixepoch('subsec') * 1000 AS INTEGER), 'cloud_publish', NEW.product_id, 'update');
END;

CREATE TABLE cloud_inbox (
  store_id    TEXT NOT NULL,
  event_id    TEXT NOT NULL,
  event_type  TEXT NOT NULL,
  received_at INTEGER NOT NULL,
  outcome     TEXT NOT NULL CHECK (outcome IN ('applied', 'ignored', 'echo', 'failed')),
  PRIMARY KEY (store_id, event_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_cloud_inbox_received_at ON cloud_inbox (received_at);

CREATE TABLE cloud_endpoints (
  store_id TEXT NOT NULL,
  endpoint TEXT NOT NULL CHECK (length(endpoint) BETWEEN 1 AND 40),
  state    TEXT NOT NULL CHECK (state IN ('ready', 'not_ready')),
  since    INTEGER NOT NULL,
  retry_at INTEGER,
  PRIMARY KEY (store_id, endpoint)
) STRICT, WITHOUT ROWID;

CREATE TABLE cloud_notices (
  id           TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  kind         TEXT NOT NULL CHECK (kind IN ('not_ready', 'plan_cap', 'conflict', 'relink', 'revoked',
                                             'failed_items', 'order', 'stock', 'backup')),
  code         TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 64),
  subject      TEXT NOT NULL DEFAULT '',
  details      TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(details)),
  count        INTEGER NOT NULL DEFAULT 1 CHECK (count >= 1),
  first_at     INTEGER NOT NULL,
  last_at      INTEGER NOT NULL,
  dismissed_at INTEGER
) STRICT;
CREATE UNIQUE INDEX idx_cloud_notices_open ON cloud_notices (kind, code, subject)
  WHERE dismissed_at IS NULL;

-- Online orders (a local mirror of the store's orders, with the till's claim) --------------

CREATE TABLE online_orders (
  id                      TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  store_id                TEXT NOT NULL,
  remote_id               TEXT NOT NULL,
  order_number            TEXT NOT NULL,
  remote_status           TEXT NOT NULL CHECK (remote_status IN ('pending', 'confirmed', 'processing',
                            'shipped', 'delivered', 'cancelled', 'returned')),
  payment_method          TEXT NOT NULL,
  delivery_type           TEXT NOT NULL CHECK (delivery_type IN ('home', 'desk', 'pickup', 'digital')),
  customer_name           TEXT,
  customer_phone          TEXT,
  wilaya_id               INTEGER CHECK (wilaya_id IS NULL OR wilaya_id BETWEEN 1 AND 69),
  commune                 TEXT,
  address                 TEXT,
  customer_notes          TEXT,
  subtotal                INTEGER NOT NULL,
  shipping_cost           INTEGER NOT NULL CHECK (shipping_cost >= 0),
  discount                INTEGER NOT NULL CHECK (discount >= 0),
  total                   INTEGER NOT NULL,
  local_state             TEXT NOT NULL CHECK (local_state IN ('new', 'claimed', 'prepared', 'fulfilled',
                            'refused', 'closed')),
  claimed_by_terminal     TEXT,
  claimed_by_user         TEXT,
  claimed_at              INTEGER,
  claim_confirmed         INTEGER NOT NULL DEFAULT 0 CHECK (claim_confirmed IN (0, 1)),
  local_document_id       TEXT REFERENCES documents (id),
  refuse_reason           TEXT,
  remote_created_at       INTEGER NOT NULL,
  remote_updated_at       INTEGER NOT NULL,
  seen_at                 INTEGER,
  closed_at               INTEGER,
  personal_data_purged_at INTEGER,
  created_at              INTEGER NOT NULL,
  updated_at              INTEGER NOT NULL,
  UNIQUE (store_id, remote_id)
) STRICT;
CREATE INDEX idx_online_orders_state ON online_orders (local_state, remote_created_at);
CREATE INDEX idx_online_orders_document ON online_orders (local_document_id);
CREATE INDEX idx_online_orders_closed_at ON online_orders (closed_at);

CREATE TRIGGER online_orders_kept BEFORE DELETE ON online_orders
BEGIN
  SELECT RAISE(ABORT, 'online orders are kept; personal data is purged instead');
END;

CREATE TABLE online_order_lines (
  order_id          TEXT NOT NULL REFERENCES online_orders (id),
  line_no           INTEGER NOT NULL CHECK (line_no >= 1),
  remote_product_id TEXT,
  product_id        TEXT REFERENCES products (id),
  product_name      TEXT NOT NULL,
  variant_name      TEXT,
  sku               TEXT,
  unit_price        INTEGER NOT NULL CHECK (unit_price >= 0),
  qty               INTEGER NOT NULL CHECK (qty > 0),
  total             INTEGER NOT NULL,
  PRIMARY KEY (order_id, line_no)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_online_order_lines_product_id ON online_order_lines (product_id);

-- DZBuild blacklist flags: salted phone hashes only (loi 18-07) -----------------------------

CREATE TABLE remote_customer_flags (
  store_id           TEXT NOT NULL,
  phone_hash         TEXT NOT NULL CHECK (length(phone_hash) = 64),
  remote_customer_id TEXT NOT NULL,
  is_banned          INTEGER NOT NULL CHECK (is_banned IN (0, 1)),
  fraud_score        INTEGER CHECK (fraud_score IS NULL OR fraud_score BETWEEN 0 AND 100),
  updated_at         INTEGER NOT NULL,
  PRIMARY KEY (store_id, phone_hash)
) STRICT, WITHOUT ROWID;

-- Zero-knowledge cloud backups --------------------------------------------------------------

CREATE TABLE cloud_backup_keys (
  id               TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  recipient        TEXT NOT NULL UNIQUE CHECK (recipient GLOB 'age1*'),
  wrapped_identity BLOB NOT NULL,
  created_at       INTEGER NOT NULL,
  retired_at       INTEGER,
  active           INTEGER GENERATED ALWAYS AS (CASE WHEN retired_at IS NULL THEN 1 END) VIRTUAL
) STRICT;
CREATE UNIQUE INDEX idx_cloud_backup_keys_active ON cloud_backup_keys (active);

CREATE TRIGGER cloud_backup_keys_frozen BEFORE UPDATE OF id, recipient, wrapped_identity, created_at
  ON cloud_backup_keys
BEGIN
  SELECT RAISE(ABORT, 'a backup key never changes; create a new one');
END;

CREATE TRIGGER cloud_backup_keys_kept BEFORE DELETE ON cloud_backup_keys
BEGIN
  SELECT RAISE(ABORT, 'backup keys are kept: old backups need them');
END;

CREATE TABLE cloud_backups (
  id           TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  store_id     TEXT NOT NULL,
  key_id       TEXT NOT NULL REFERENCES cloud_backup_keys (id),
  remote_id    TEXT,
  status       TEXT NOT NULL CHECK (status IN ('uploading', 'complete', 'failed')),
  file_name    TEXT NOT NULL,
  size         INTEGER NOT NULL CHECK (size > 0),
  sha256       TEXT NOT NULL CHECK (length(sha256) = 64),
  part_size    INTEGER NOT NULL CHECK (part_size > 0),
  parts_total  INTEGER NOT NULL CHECK (parts_total >= 1),
  parts_done   INTEGER NOT NULL DEFAULT 0 CHECK (parts_done BETWEEN 0 AND parts_total),
  part_etags   TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(part_etags) AND json_type(part_etags) = 'array'),
  last_error   TEXT,
  created_at   INTEGER NOT NULL,
  completed_at INTEGER,
  CHECK ((status = 'complete') = (completed_at IS NOT NULL))
) STRICT;
CREATE INDEX idx_cloud_backups_key_id ON cloud_backups (key_id);
CREATE INDEX idx_cloud_backups_created_at ON cloud_backups (created_at);
