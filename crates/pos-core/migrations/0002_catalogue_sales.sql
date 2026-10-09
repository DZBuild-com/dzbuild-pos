-- 0002 Catalogue, stock ledger, numbering, documents, payments, held carts and cash sessions
-- (milestone M1, spec §5.1). Never edit: add 0003_*.sql instead.
-- Tax rates are not seeded here: `catalog::ensure_defaults` copies them from the regulatory data
-- pack, so that no regulatory value is written twice.

-- Catalogue ---------------------------------------------------------------------------------

CREATE TABLE categories (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  parent_id   TEXT REFERENCES categories (id),
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  color       TEXT,
  sort_order  INTEGER NOT NULL DEFAULT 0,
  image       TEXT,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  CHECK (parent_id IS NULL OR parent_id <> id)
) STRICT;
CREATE INDEX idx_categories_parent_id ON categories (parent_id);

CREATE TABLE units (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  decimals    INTEGER NOT NULL DEFAULT 0 CHECK (decimals BETWEEN 0 AND 3),
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;

CREATE TABLE tax_rates (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code       TEXT NOT NULL UNIQUE,
  rate_bp    INTEGER NOT NULL CHECK (rate_bp BETWEEN 0 AND 10000),
  name_ar    TEXT NOT NULL DEFAULT '',
  name_fr    TEXT NOT NULL DEFAULT '',
  name_en    TEXT NOT NULL DEFAULT '',
  active     INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  valid_from TEXT CHECK (valid_from IS NULL OR valid_from GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  valid_to   TEXT CHECK (valid_to IS NULL OR valid_to GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  source     TEXT,
  created_at INTEGER NOT NULL,
  updated_at INTEGER NOT NULL
) STRICT;

CREATE TABLE price_levels (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  is_default  INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  CHECK (is_default = 0 OR archived_at IS NULL)
) STRICT;
CREATE UNIQUE INDEX idx_price_levels_single_default ON price_levels (is_default) WHERE is_default = 1;

CREATE TABLE products (
  id                   TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  reference            TEXT COLLATE NOCASE,
  name_ar              TEXT NOT NULL DEFAULT '',
  name_fr              TEXT NOT NULL DEFAULT '',
  name_en              TEXT NOT NULL DEFAULT '',
  search_text          TEXT NOT NULL DEFAULT '',
  category_id          TEXT REFERENCES categories (id),
  unit_id              TEXT NOT NULL REFERENCES units (id),
  tax_rate_id          TEXT NOT NULL REFERENCES tax_rates (id),
  kind                 TEXT NOT NULL DEFAULT 'simple'
                       CHECK (kind IN ('simple', 'variant_parent', 'variant', 'composite', 'service')),
  parent_id            TEXT REFERENCES products (id),
  sell_price           INTEGER NOT NULL DEFAULT 0 CHECK (sell_price >= 0),
  cost_price           INTEGER NOT NULL DEFAULT 0 CHECK (cost_price >= 0),
  min_stock            INTEGER NOT NULL DEFAULT 0 CHECK (min_stock >= 0),
  track_stock          INTEGER NOT NULL DEFAULT 1 CHECK (track_stock IN (0, 1)),
  allow_negative_stock INTEGER NOT NULL DEFAULT 0 CHECK (allow_negative_stock IN (0, 1)),
  is_weighed           INTEGER NOT NULL DEFAULT 0 CHECK (is_weighed IN (0, 1)),
  available_at_till    INTEGER NOT NULL DEFAULT 1 CHECK (available_at_till IN (0, 1)),
  plu                  INTEGER UNIQUE CHECK (plu IS NULL OR plu BETWEEN 1 AND 99999),
  image                TEXT,
  archived_at          INTEGER,
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL,
  CHECK (parent_id IS NULL OR parent_id <> id),
  CHECK ((kind = 'variant') = (parent_id IS NOT NULL)),
  CHECK (kind NOT IN ('service', 'variant_parent', 'composite') OR track_stock = 0)
) STRICT;
CREATE UNIQUE INDEX idx_products_reference ON products (reference) WHERE reference IS NOT NULL;
CREATE INDEX idx_products_category_id ON products (category_id);
CREATE INDEX idx_products_unit_id ON products (unit_id);
CREATE INDEX idx_products_tax_rate_id ON products (tax_rate_id);
CREATE INDEX idx_products_parent_id ON products (parent_id);

CREATE TABLE product_barcodes (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id TEXT NOT NULL REFERENCES products (id),
  barcode    TEXT NOT NULL UNIQUE,
  pack_qty   INTEGER NOT NULL DEFAULT 1000 CHECK (pack_qty > 0),
  label      TEXT,
  created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_product_barcodes_product_id ON product_barcodes (product_id);

CREATE TABLE product_prices (
  product_id     TEXT NOT NULL REFERENCES products (id),
  price_level_id TEXT NOT NULL REFERENCES price_levels (id),
  price          INTEGER NOT NULL CHECK (price >= 0),
  updated_at     INTEGER NOT NULL,
  PRIMARY KEY (product_id, price_level_id)
) STRICT;
CREATE INDEX idx_product_prices_price_level_id ON product_prices (price_level_id);

-- Components of a composite product (PRD-040): selling one kit takes `qty` of each component.
CREATE TABLE product_components (
  product_id   TEXT NOT NULL REFERENCES products (id),
  component_id TEXT NOT NULL REFERENCES products (id),
  qty          INTEGER NOT NULL CHECK (qty > 0),
  PRIMARY KEY (product_id, component_id),
  CHECK (product_id <> component_id)
) STRICT;
CREATE INDEX idx_product_components_component_id ON product_components (component_id);

-- Full-text index over the normalised names, reference and barcodes (spec §8). External
-- content: the text lives in `products`, the triggers below keep the index in step. The app
-- never runs VACUUM (it could renumber the rowids); `catalog::search::rebuild_index` repairs it.
CREATE VIRTUAL TABLE products_fts USING fts5 (
  search_text,
  reference,
  content = 'products',
  content_rowid = 'rowid',
  tokenize = 'unicode61 remove_diacritics 2',
  prefix = '2 3'
);

CREATE TRIGGER products_fts_after_insert AFTER INSERT ON products
BEGIN
  INSERT INTO products_fts (rowid, search_text, reference)
  VALUES (NEW.rowid, NEW.search_text, NEW.reference);
END;

CREATE TRIGGER products_fts_after_delete AFTER DELETE ON products
BEGIN
  INSERT INTO products_fts (products_fts, rowid, search_text, reference)
  VALUES ('delete', OLD.rowid, OLD.search_text, OLD.reference);
END;

CREATE TRIGGER products_fts_after_update AFTER UPDATE OF search_text, reference ON products
BEGIN
  INSERT INTO products_fts (products_fts, rowid, search_text, reference)
  VALUES ('delete', OLD.rowid, OLD.search_text, OLD.reference);
  INSERT INTO products_fts (rowid, search_text, reference)
  VALUES (NEW.rowid, NEW.search_text, NEW.reference);
END;

-- Stock ---------------------------------------------------------------------------------------

CREATE TABLE stock_locations (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  kind        TEXT NOT NULL DEFAULT 'storage' CHECK (kind IN ('storage', 'damaged')),
  is_default  INTEGER NOT NULL DEFAULT 0 CHECK (is_default IN (0, 1)),
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  CHECK (is_default = 0 OR (kind = 'storage' AND archived_at IS NULL))
) STRICT;
CREATE UNIQUE INDEX idx_stock_locations_single_default ON stock_locations (is_default) WHERE is_default = 1;

CREATE TABLE stock_levels (
  product_id  TEXT NOT NULL REFERENCES products (id),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  qty         INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (product_id, location_id)
) STRICT;
CREATE INDEX idx_stock_levels_location_id ON stock_levels (location_id);

CREATE TABLE stock_movements (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id  TEXT NOT NULL REFERENCES products (id),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  qty_delta   INTEGER NOT NULL CHECK (qty_delta <> 0),
  reason      TEXT NOT NULL CHECK (reason IN ('sale', 'return', 'purchase', 'adjustment', 'count',
                'transfer_in', 'transfer_out', 'loss', 'production', 'opening')),
  reason_code TEXT CHECK (reason_code IS NULL OR reason_code IN ('damage', 'loss', 'theft', 'expiry',
                'own_use', 'sample', 'correction')),
  ref_type    TEXT,
  ref_id      TEXT,
  unit_cost   INTEGER CHECK (unit_cost IS NULL OR unit_cost >= 0),
  created_at  INTEGER NOT NULL,
  user_id     TEXT,
  terminal_id TEXT,
  lot_id      TEXT,
  note        TEXT
) STRICT;
CREATE INDEX idx_stock_movements_product_location ON stock_movements (product_id, location_id);
CREATE INDEX idx_stock_movements_location_id ON stock_movements (location_id);
CREATE INDEX idx_stock_movements_ref ON stock_movements (ref_type, ref_id);
CREATE INDEX idx_stock_movements_created_at ON stock_movements (created_at);
CREATE INDEX idx_stock_movements_user_id ON stock_movements (user_id);
CREATE INDEX idx_stock_movements_terminal_id ON stock_movements (terminal_id);
CREATE INDEX idx_stock_movements_lot_id ON stock_movements (lot_id);

CREATE TRIGGER stock_movements_no_update BEFORE UPDATE ON stock_movements
BEGIN
  SELECT RAISE(ABORT, 'stock_movements is append-only');
END;

CREATE TRIGGER stock_movements_no_delete BEFORE DELETE ON stock_movements
BEGIN
  SELECT RAISE(ABORT, 'stock_movements is append-only');
END;

-- The level cache moves in the same statement as the ledger row, so no code path can forget it.
CREATE TRIGGER stock_movements_update_level AFTER INSERT ON stock_movements
BEGIN
  INSERT INTO stock_levels (product_id, location_id, qty, updated_at)
  VALUES (NEW.product_id, NEW.location_id, NEW.qty_delta, NEW.created_at)
  ON CONFLICT (product_id, location_id)
  DO UPDATE SET qty = qty + excluded.qty, updated_at = excluded.updated_at;
END;

-- Numbering (spec §5.2) -----------------------------------------------------------------------

CREATE TABLE counters (
  doc_type    TEXT NOT NULL,
  series      TEXT NOT NULL,
  fiscal_year INTEGER NOT NULL,
  last_number INTEGER NOT NULL CHECK (last_number >= 1),
  PRIMARY KEY (doc_type, series, fiscal_year)
) STRICT;

CREATE TRIGGER counters_start_at_one BEFORE INSERT ON counters
WHEN NEW.last_number <> 1
BEGIN
  SELECT RAISE(ABORT, 'a counter starts at 1');
END;

CREATE TRIGGER counters_step_by_one BEFORE UPDATE ON counters
WHEN NEW.last_number <> OLD.last_number + 1
  OR NEW.doc_type <> OLD.doc_type
  OR NEW.series <> OLD.series
  OR NEW.fiscal_year <> OLD.fiscal_year
BEGIN
  SELECT RAISE(ABORT, 'a counter only moves up by one');
END;

CREATE TRIGGER counters_no_delete BEFORE DELETE ON counters
BEGIN
  SELECT RAISE(ABORT, 'counters are never deleted');
END;

-- Cash sessions (spec §7.3) -------------------------------------------------------------------

CREATE TABLE cash_sessions (
  id            TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  terminal_id   TEXT NOT NULL,
  user_id       TEXT NOT NULL,
  business_date TEXT NOT NULL,
  opened_at     INTEGER NOT NULL,
  opening_float INTEGER NOT NULL CHECK (opening_float >= 0),
  closed_at     INTEGER,
  closed_by     TEXT,
  expected_cash INTEGER,
  counted_cash  INTEGER CHECK (counted_cash IS NULL OR counted_cash >= 0),
  variance      INTEGER,
  status        TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed')),
  z_number      TEXT UNIQUE,
  notes         TEXT,
  CHECK ((status = 'closed') = (closed_at IS NOT NULL AND closed_by IS NOT NULL
    AND expected_cash IS NOT NULL AND counted_cash IS NOT NULL AND variance IS NOT NULL
    AND z_number IS NOT NULL))
) STRICT;
CREATE UNIQUE INDEX idx_cash_sessions_one_open_per_terminal ON cash_sessions (terminal_id)
  WHERE status = 'open';
CREATE INDEX idx_cash_sessions_terminal_opened ON cash_sessions (terminal_id, opened_at);
CREATE INDEX idx_cash_sessions_user_id ON cash_sessions (user_id);
CREATE INDEX idx_cash_sessions_business_date ON cash_sessions (business_date);

CREATE TRIGGER cash_sessions_open_on_insert BEFORE INSERT ON cash_sessions
WHEN NEW.status <> 'open'
BEGIN
  SELECT RAISE(ABORT, 'a cash session starts open');
END;

CREATE TRIGGER cash_sessions_closed_are_final BEFORE UPDATE ON cash_sessions
WHEN OLD.status = 'closed'
BEGIN
  SELECT RAISE(ABORT, 'a closed cash session cannot be reopened or changed');
END;

CREATE TRIGGER cash_sessions_no_delete BEFORE DELETE ON cash_sessions
BEGIN
  SELECT RAISE(ABORT, 'cash sessions are never deleted');
END;

CREATE TABLE cash_movements (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  session_id TEXT NOT NULL REFERENCES cash_sessions (id),
  kind       TEXT NOT NULL CHECK (kind IN ('float', 'sale', 'refund', 'paid_in', 'paid_out',
               'expense', 'credit_payment', 'drop')),
  method     TEXT NOT NULL DEFAULT 'cash',
  amount     INTEGER NOT NULL,
  category   TEXT,
  ref_type   TEXT,
  ref_id     TEXT,
  note       TEXT,
  created_at INTEGER NOT NULL,
  user_id    TEXT NOT NULL,
  CHECK ((kind IN ('float', 'sale', 'paid_in', 'credit_payment') AND amount >= 0)
      OR (kind IN ('refund', 'paid_out', 'expense', 'drop') AND amount <= 0)),
  CHECK (kind <> 'expense' OR category IS NOT NULL)
) STRICT;
CREATE INDEX idx_cash_movements_session_id ON cash_movements (session_id);
CREATE INDEX idx_cash_movements_ref ON cash_movements (ref_type, ref_id);
CREATE INDEX idx_cash_movements_user_id ON cash_movements (user_id);
CREATE INDEX idx_cash_movements_created_at ON cash_movements (created_at);

CREATE TRIGGER cash_movements_session_open BEFORE INSERT ON cash_movements
WHEN (SELECT status FROM cash_sessions WHERE id = NEW.session_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'cash movements need an open cash session');
END;

CREATE TRIGGER cash_movements_no_update BEFORE UPDATE ON cash_movements
BEGIN
  SELECT RAISE(ABORT, 'cash_movements is append-only');
END;

CREATE TRIGGER cash_movements_no_delete BEFORE DELETE ON cash_movements
BEGIN
  SELECT RAISE(ABORT, 'cash_movements is append-only');
END;

CREATE TABLE cash_counts (
  session_id   TEXT NOT NULL REFERENCES cash_sessions (id),
  denomination INTEGER NOT NULL CHECK (denomination > 0),
  count        INTEGER NOT NULL CHECK (count >= 0),
  PRIMARY KEY (session_id, denomination)
) STRICT;

CREATE TRIGGER cash_counts_no_update BEFORE UPDATE ON cash_counts
BEGIN
  SELECT RAISE(ABORT, 'cash_counts is append-only');
END;

CREATE TRIGGER cash_counts_no_delete BEFORE DELETE ON cash_counts
BEGIN
  SELECT RAISE(ABORT, 'cash_counts is append-only');
END;

-- One closure record per Z (spec §7.3, AUD-004/005): the Z report as printed, the perpetual
-- grand total of the terminal, and the anchors (last seq and hash) of the audit journal and of
-- the terminal's document chains. Closures chain their own hashes per terminal.
CREATE TABLE cash_closures (
  session_id       TEXT PRIMARY KEY NOT NULL REFERENCES cash_sessions (id),
  terminal_id      TEXT NOT NULL,
  z_seq            INTEGER NOT NULL CHECK (z_seq >= 1),
  z_number         TEXT NOT NULL UNIQUE,
  closed_at        INTEGER NOT NULL,
  closed_by        TEXT NOT NULL,
  business_date    TEXT NOT NULL,
  report           TEXT NOT NULL CHECK (json_valid(report)),
  grand_total      INTEGER NOT NULL,
  audit_last_seq   INTEGER NOT NULL CHECK (audit_last_seq >= 0),
  audit_last_hash  TEXT NOT NULL CHECK (length(audit_last_hash) = 64),
  audit_intact     INTEGER NOT NULL CHECK (audit_intact IN (0, 1)),
  documents_anchor TEXT NOT NULL CHECK (json_valid(documents_anchor)),
  prev_hash        TEXT NOT NULL CHECK (length(prev_hash) = 64),
  hash             TEXT NOT NULL CHECK (length(hash) = 64),
  UNIQUE (terminal_id, z_seq)
) STRICT;
CREATE INDEX idx_cash_closures_business_date ON cash_closures (business_date);

CREATE TRIGGER cash_closures_no_update BEFORE UPDATE ON cash_closures
BEGIN
  SELECT RAISE(ABORT, 'cash_closures is append-only');
END;

CREATE TRIGGER cash_closures_no_delete BEFORE DELETE ON cash_closures
BEGIN
  SELECT RAISE(ABORT, 'cash_closures is append-only');
END;

-- Documents (spec §5.1, §5.3) -----------------------------------------------------------------

CREATE TABLE documents (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  doc_type        TEXT NOT NULL CHECK (doc_type IN ('ticket', 'invoice', 'proforma', 'quote',
                    'delivery_note', 'credit_note', 'return_note', 'customer_order',
                    'purchase_order', 'goods_receipt', 'supplier_invoice', 'transfer')),
  number          TEXT,
  series          TEXT NOT NULL DEFAULT '',
  fiscal_year     INTEGER,
  status          TEXT NOT NULL DEFAULT 'draft' CHECK (status IN ('draft', 'validated', 'cancelled')),
  business_date   TEXT NOT NULL,
  customer_id     TEXT,
  supplier_id     TEXT,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  cash_session_id TEXT REFERENCES cash_sessions (id),
  price_level_id  TEXT REFERENCES price_levels (id),
  lang            TEXT NOT NULL DEFAULT 'fr' CHECK (lang IN ('ar', 'fr', 'en')),
  currency        TEXT NOT NULL DEFAULT 'DZD' CHECK (currency = 'DZD'),
  tax_regime      TEXT NOT NULL DEFAULT 'reel' CHECK (tax_regime IN ('reel', 'ifu')),
  price_mode      TEXT NOT NULL DEFAULT 'ttc' CHECK (price_mode IN ('ttc', 'ht')),
  subtotal_ht     INTEGER NOT NULL DEFAULT 0,
  discount_total  INTEGER NOT NULL DEFAULT 0,
  tax_total       INTEGER NOT NULL DEFAULT 0,
  total_ttc       INTEGER NOT NULL DEFAULT 0,
  stamp_duty      INTEGER NOT NULL DEFAULT 0 CHECK (stamp_duty >= 0),
  stamp_mode      TEXT NOT NULL DEFAULT 'charge_customer'
                  CHECK (stamp_mode IN ('charge_customer', 'absorb', 'off')),
  rounding        INTEGER NOT NULL DEFAULT 0,
  total_due       INTEGER NOT NULL DEFAULT 0,
  paid_total      INTEGER NOT NULL DEFAULT 0,
  change_given    INTEGER NOT NULL DEFAULT 0 CHECK (change_given >= 0),
  credit_amount   INTEGER NOT NULL DEFAULT 0,
  note            TEXT,
  parent_id       TEXT REFERENCES documents (id),
  source_ref      TEXT,
  validated_at    INTEGER,
  chain_seq       INTEGER CHECK (chain_seq IS NULL OR chain_seq >= 1),
  prev_hash       TEXT CHECK (prev_hash IS NULL OR length(prev_hash) = 64),
  hash            TEXT CHECK (hash IS NULL OR length(hash) = 64),
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  CHECK (subtotal_ht + tax_total = total_ttc),
  CHECK (status <> 'validated' OR (number IS NOT NULL AND fiscal_year IS NOT NULL
    AND validated_at IS NOT NULL AND chain_seq IS NOT NULL AND prev_hash IS NOT NULL
    AND hash IS NOT NULL)),
  CHECK (status = 'validated' OR (number IS NULL AND chain_seq IS NULL AND hash IS NULL))
) STRICT;
CREATE UNIQUE INDEX idx_documents_type_number ON documents (doc_type, number) WHERE number IS NOT NULL;
CREATE UNIQUE INDEX idx_documents_chain ON documents (doc_type, series, chain_seq)
  WHERE chain_seq IS NOT NULL;
CREATE INDEX idx_documents_number ON documents (number);
CREATE INDEX idx_documents_business_date ON documents (business_date);
CREATE INDEX idx_documents_status ON documents (doc_type, status);
CREATE INDEX idx_documents_customer_id ON documents (customer_id);
CREATE INDEX idx_documents_supplier_id ON documents (supplier_id);
CREATE INDEX idx_documents_user_id ON documents (user_id);
CREATE INDEX idx_documents_terminal_id ON documents (terminal_id, validated_at);
CREATE INDEX idx_documents_cash_session_id ON documents (cash_session_id);
CREATE INDEX idx_documents_price_level_id ON documents (price_level_id);
CREATE INDEX idx_documents_parent_id ON documents (parent_id);
CREATE INDEX idx_documents_source_ref ON documents (source_ref);

CREATE TRIGGER documents_insert_as_draft BEFORE INSERT ON documents
WHEN NEW.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'documents are inserted as drafts, then validated');
END;

CREATE TRIGGER documents_final_once_validated BEFORE UPDATE ON documents
WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'validated and cancelled documents are immutable');
END;

CREATE TRIGGER documents_kept_once_validated BEFORE DELETE ON documents
WHEN OLD.status <> 'draft'
BEGIN
  SELECT RAISE(ABORT, 'validated and cancelled documents are never deleted');
END;

CREATE TABLE document_lines (
  id                  TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  document_id         TEXT NOT NULL REFERENCES documents (id),
  line_no             INTEGER NOT NULL CHECK (line_no >= 1),
  product_id          TEXT REFERENCES products (id),
  description         TEXT NOT NULL,
  qty                 INTEGER NOT NULL CHECK (qty <> 0),
  unit_code           TEXT NOT NULL DEFAULT '',
  unit_price          INTEGER NOT NULL CHECK (unit_price >= 0),
  price_overridden    INTEGER NOT NULL DEFAULT 0 CHECK (price_overridden IN (0, 1)),
  discount_bp         INTEGER NOT NULL DEFAULT 0 CHECK (discount_bp BETWEEN 0 AND 10000),
  discount_amount     INTEGER NOT NULL DEFAULT 0,
  tax_rate_bp         INTEGER NOT NULL CHECK (tax_rate_bp BETWEEN 0 AND 10000),
  total_ht            INTEGER NOT NULL,
  tax_amount          INTEGER NOT NULL,
  total_ttc           INTEGER NOT NULL,
  cost_price          INTEGER NOT NULL DEFAULT 0 CHECK (cost_price >= 0),
  lot_id              TEXT,
  serial_no           TEXT,
  parent_line_id      TEXT REFERENCES document_lines (id),
  origin_line_id      TEXT REFERENCES document_lines (id),
  restock_location_id TEXT REFERENCES stock_locations (id),
  reason_code         TEXT,
  note                TEXT,
  UNIQUE (document_id, line_no),
  CHECK (total_ht + tax_amount = total_ttc)
) STRICT;
CREATE INDEX idx_document_lines_product_id ON document_lines (product_id);
CREATE INDEX idx_document_lines_parent_line_id ON document_lines (parent_line_id);
CREATE INDEX idx_document_lines_origin_line_id ON document_lines (origin_line_id);
CREATE INDEX idx_document_lines_restock_location_id ON document_lines (restock_location_id);
CREATE INDEX idx_document_lines_lot_id ON document_lines (lot_id);

CREATE TRIGGER document_lines_draft_insert BEFORE INSERT ON document_lines
WHEN (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'lines are added to draft documents only');
END;

CREATE TRIGGER document_lines_draft_update BEFORE UPDATE ON document_lines
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
  OR (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'lines of a validated document are immutable');
END;

CREATE TRIGGER document_lines_draft_delete BEFORE DELETE ON document_lines
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'lines of a validated document are never deleted');
END;

CREATE TABLE document_taxes (
  document_id TEXT NOT NULL REFERENCES documents (id),
  rate_bp     INTEGER NOT NULL CHECK (rate_bp BETWEEN 0 AND 10000),
  base_ht     INTEGER NOT NULL,
  tax_amount  INTEGER NOT NULL,
  PRIMARY KEY (document_id, rate_bp)
) STRICT;

CREATE TRIGGER document_taxes_draft_insert BEFORE INSERT ON document_taxes
WHEN (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'taxes are added to draft documents only');
END;

CREATE TRIGGER document_taxes_draft_update BEFORE UPDATE ON document_taxes
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
  OR (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'taxes of a validated document are immutable');
END;

CREATE TRIGGER document_taxes_draft_delete BEFORE DELETE ON document_taxes
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'taxes of a validated document are never deleted');
END;

CREATE TABLE payments (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  document_id     TEXT NOT NULL REFERENCES documents (id),
  cash_session_id TEXT REFERENCES cash_sessions (id),
  method          TEXT NOT NULL CHECK (method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                    'cheque', 'transfer', 'credit', 'voucher', 'change')),
  amount          INTEGER NOT NULL CHECK (amount <> 0),
  reference       TEXT,
  cheque_bank     TEXT,
  cheque_date     TEXT,
  created_at      INTEGER NOT NULL,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  CHECK (method <> 'change' OR amount < 0),
  CHECK (method <> 'cheque' OR amount < 0
    OR (reference IS NOT NULL AND cheque_bank IS NOT NULL AND cheque_date IS NOT NULL)),
  CHECK (method <> 'transfer' OR amount < 0 OR reference IS NOT NULL)
) STRICT;
CREATE INDEX idx_payments_document_id ON payments (document_id);
CREATE INDEX idx_payments_session_method ON payments (cash_session_id, method);
CREATE INDEX idx_payments_user_id ON payments (user_id);
CREATE INDEX idx_payments_terminal_id ON payments (terminal_id);

CREATE TRIGGER payments_draft_insert BEFORE INSERT ON payments
WHEN (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'payments are added to draft documents only');
END;

CREATE TRIGGER payments_draft_update BEFORE UPDATE ON payments
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
  OR (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'payments of a validated document are immutable');
END;

CREATE TRIGGER payments_draft_delete BEFORE DELETE ON payments
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'payments of a validated document are never deleted');
END;

-- Held tickets (spec §7.4): never touch stock or cash; deleted when recalled.
CREATE TABLE held_carts (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  terminal_id TEXT NOT NULL,
  user_id     TEXT NOT NULL,
  label       TEXT NOT NULL DEFAULT '',
  payload     TEXT NOT NULL CHECK (json_valid(payload)),
  created_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_held_carts_terminal_id ON held_carts (terminal_id, created_at);
CREATE INDEX idx_held_carts_user_id ON held_carts (user_id);

-- Seeds: fixed ids (like the system roles of 0001) so that code and tests can name them.

INSERT INTO units (id, code, name_ar, name_fr, name_en, decimals, created_at, updated_at) VALUES
  ('00000000-0000-7000-8000-000000000101', 'pc', 'قطعة', 'Pièce', 'Piece', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000102', 'kg', 'كلغ', 'Kilogramme', 'Kilogram', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000103', 'g', 'غ', 'Gramme', 'Gram', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000104', 'l', 'لتر', 'Litre', 'Litre', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000105', 'm', 'متر', 'Mètre', 'Metre', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000106', 'm2', 'متر مربع', 'Mètre carré', 'Square metre', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000107', 'box', 'علبة', 'Boîte', 'Box', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000108', 'carton', 'كرتونة', 'Carton', 'Carton', 0, unixepoch() * 1000, unixepoch() * 1000);

INSERT INTO price_levels (id, code, name_ar, name_fr, name_en, is_default, sort_order, created_at, updated_at) VALUES
  ('00000000-0000-7000-8000-000000000201', 'retail', 'التجزئة', 'Détail', 'Retail', 1, 1, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000202', 'semi_wholesale', 'نصف الجملة', 'Demi-gros', 'Semi-wholesale', 0, 2, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000203', 'wholesale', 'الجملة', 'Gros', 'Wholesale', 0, 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000204', 'reseller', 'إعادة البيع', 'Revendeur', 'Reseller', 0, 4, unixepoch() * 1000, unixepoch() * 1000);

INSERT INTO stock_locations (id, code, name_ar, name_fr, name_en, kind, is_default, created_at, updated_at) VALUES
  ('00000000-0000-7000-8000-000000000301', 'SHOP', 'المحل', 'Magasin', 'Shop', 'storage', 1, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000302', 'DAMAGED', 'سلع تالفة', 'Marchandise abîmée', 'Damaged goods', 'damaged', 0, unixepoch() * 1000, unixepoch() * 1000);
