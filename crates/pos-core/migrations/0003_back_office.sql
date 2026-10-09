-- 0003 Back office (milestone M2, spec §5.1 "M2 Back office"): customers and the kridi ledger,
-- suppliers and their ledger, allocations, expenses, stock counts, lots, serials, promotions,
-- price breaks, product extras, per-location minimums, transfers, cost history and import jobs,
-- plus the two cash-session guards 0002 left out (count sheets, closure records).
-- Never edit: add 0004_*.sql instead.

-- Customers and kridi --------------------------------------------------------------------------

CREATE TABLE customers (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code              TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name              TEXT NOT NULL CHECK (length(trim(name)) > 0),
  search_text       TEXT NOT NULL DEFAULT '',
  customer_type     TEXT NOT NULL DEFAULT 'individual'
                    CHECK (customer_type IN ('individual', 'reseller', 'garage', 'contractor',
                                             'restaurant', 'company', 'other')),
  phone             TEXT,
  phone2            TEXT,
  email             TEXT,
  address           TEXT,
  wilaya            TEXT,
  nif               TEXT,
  nis               TEXT,
  rc                TEXT,
  ai                TEXT,
  nin               TEXT,
  ids_verified_at   TEXT CHECK (ids_verified_at IS NULL
                                OR ids_verified_at GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  price_level_id    TEXT REFERENCES price_levels (id),
  discount_bp       INTEGER NOT NULL DEFAULT 0 CHECK (discount_bp BETWEEN 0 AND 10000),
  credit_limit      INTEGER CHECK (credit_limit IS NULL OR credit_limit >= 0),
  balance           INTEGER NOT NULL DEFAULT 0,
  lang              TEXT CHECK (lang IS NULL OR lang IN ('ar', 'fr', 'en')),
  notes             TEXT,
  tags              TEXT,
  is_blocked        INTEGER NOT NULL DEFAULT 0 CHECK (is_blocked IN (0, 1)),
  is_favourite      INTEGER NOT NULL DEFAULT 0 CHECK (is_favourite IN (0, 1)),
  marketing_consent INTEGER NOT NULL DEFAULT 0 CHECK (marketing_consent IN (0, 1)),
  archived_at       INTEGER,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  created_by        TEXT,
  terminal_id       TEXT
) STRICT;
CREATE UNIQUE INDEX idx_customers_phone ON customers (phone)
  WHERE phone IS NOT NULL AND archived_at IS NULL;
CREATE INDEX idx_customers_price_level_id ON customers (price_level_id);
CREATE INDEX idx_customers_search_text ON customers (search_text);
CREATE INDEX idx_customers_name ON customers (name COLLATE NOCASE);
CREATE INDEX idx_customers_debtors ON customers (balance) WHERE balance <> 0;

CREATE TRIGGER customers_no_delete BEFORE DELETE ON customers
BEGIN
  SELECT RAISE(ABORT, 'customers are archived, never deleted');
END;

CREATE TABLE customer_ledger (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  customer_id     TEXT NOT NULL REFERENCES customers (id),
  kind            TEXT NOT NULL CHECK (kind IN ('sale_credit', 'voucher_redeemed', 'payment',
                                                'refund', 'adjustment', 'opening')),
  amount          INTEGER NOT NULL CHECK (amount <> 0),
  balance_after   INTEGER NOT NULL,
  method          TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                          'cheque', 'transfer', 'credit', 'voucher')),
  reference       TEXT,
  cheque_bank     TEXT,
  cheque_date     TEXT,
  stamp_duty      INTEGER NOT NULL DEFAULT 0 CHECK (stamp_duty >= 0),
  stamp_charged   INTEGER NOT NULL DEFAULT 0 CHECK (stamp_charged BETWEEN 0 AND stamp_duty),
  receipt_number  TEXT UNIQUE,
  ref_type        TEXT,
  ref_id          TEXT,
  cash_session_id TEXT REFERENCES cash_sessions (id),
  business_date   TEXT NOT NULL,
  note            TEXT,
  created_at      INTEGER NOT NULL,
  user_id         TEXT,
  terminal_id     TEXT,
  CHECK ((kind IN ('sale_credit', 'voucher_redeemed') AND amount > 0)
      OR (kind IN ('payment', 'refund') AND amount < 0)
      OR kind IN ('adjustment', 'opening')),
  CHECK (kind = 'payment' OR (stamp_duty = 0 AND receipt_number IS NULL)),
  CHECK (kind <> 'payment' OR method IS NOT NULL)
) STRICT;
CREATE INDEX idx_customer_ledger_customer ON customer_ledger (customer_id, created_at);
CREATE INDEX idx_customer_ledger_ref ON customer_ledger (ref_type, ref_id);
CREATE INDEX idx_customer_ledger_cash_session_id ON customer_ledger (cash_session_id);
CREATE INDEX idx_customer_ledger_business_date ON customer_ledger (business_date);

CREATE TRIGGER customer_ledger_follows_the_cache BEFORE INSERT ON customer_ledger
WHEN NEW.balance_after IS NOT (SELECT balance FROM customers WHERE id = NEW.customer_id) + NEW.amount
BEGIN
  SELECT RAISE(ABORT, 'customer_ledger balance_after must follow the cached balance');
END;

CREATE TRIGGER customer_ledger_moves_the_balance AFTER INSERT ON customer_ledger
BEGIN
  UPDATE customers SET balance = NEW.balance_after, updated_at = NEW.created_at
  WHERE id = NEW.customer_id;
END;

CREATE TRIGGER customer_ledger_no_update BEFORE UPDATE ON customer_ledger
BEGIN
  SELECT RAISE(ABORT, 'customer_ledger is append-only');
END;

CREATE TRIGGER customer_ledger_no_delete BEFORE DELETE ON customer_ledger
BEGIN
  SELECT RAISE(ABORT, 'customer_ledger is append-only');
END;

-- Which versement (credit side) settled which debt (debit side), CRD-004/006.
CREATE TABLE customer_allocations (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  customer_id TEXT NOT NULL REFERENCES customers (id),
  debit_id    TEXT NOT NULL REFERENCES customer_ledger (id),
  credit_id   TEXT NOT NULL REFERENCES customer_ledger (id),
  amount      INTEGER NOT NULL CHECK (amount > 0),
  created_at  INTEGER NOT NULL,
  CHECK (debit_id <> credit_id)
) STRICT;
CREATE INDEX idx_customer_allocations_customer_id ON customer_allocations (customer_id);
CREATE INDEX idx_customer_allocations_debit_id ON customer_allocations (debit_id);
CREATE INDEX idx_customer_allocations_credit_id ON customer_allocations (credit_id);

CREATE TRIGGER customer_allocations_no_update BEFORE UPDATE ON customer_allocations
BEGIN
  SELECT RAISE(ABORT, 'customer_allocations is append-only');
END;

CREATE TRIGGER customer_allocations_no_delete BEFORE DELETE ON customer_allocations
BEGIN
  SELECT RAISE(ABORT, 'customer_allocations is append-only');
END;

-- Suppliers ------------------------------------------------------------------------------------

CREATE TABLE suppliers (
  id                 TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code               TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name               TEXT NOT NULL CHECK (length(trim(name)) > 0),
  search_text        TEXT NOT NULL DEFAULT '',
  phone              TEXT,
  phone2             TEXT,
  email              TEXT,
  address            TEXT,
  wilaya             TEXT,
  nif                TEXT,
  nis                TEXT,
  rc                 TEXT,
  ai                 TEXT,
  nin                TEXT,
  rib                TEXT,
  payment_terms_days INTEGER CHECK (payment_terms_days IS NULL OR payment_terms_days BETWEEN 0 AND 365),
  balance            INTEGER NOT NULL DEFAULT 0,
  notes              TEXT,
  archived_at        INTEGER,
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL,
  created_by         TEXT,
  terminal_id        TEXT
) STRICT;
CREATE INDEX idx_suppliers_search_text ON suppliers (search_text);
CREATE INDEX idx_suppliers_name ON suppliers (name COLLATE NOCASE);

CREATE TRIGGER suppliers_no_delete BEFORE DELETE ON suppliers
BEGIN
  SELECT RAISE(ABORT, 'suppliers are archived, never deleted');
END;

CREATE TABLE supplier_ledger (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  supplier_id     TEXT NOT NULL REFERENCES suppliers (id),
  kind            TEXT NOT NULL CHECK (kind IN ('purchase', 'payment', 'return', 'adjustment', 'opening')),
  amount          INTEGER NOT NULL CHECK (amount <> 0),
  balance_after   INTEGER NOT NULL,
  method          TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                          'cheque', 'transfer')),
  reference       TEXT,
  cheque_bank     TEXT,
  cheque_date     TEXT,
  stamp_duty      INTEGER NOT NULL DEFAULT 0 CHECK (stamp_duty >= 0),
  stamp_charged   INTEGER NOT NULL DEFAULT 0 CHECK (stamp_charged BETWEEN 0 AND stamp_duty),
  receipt_number  TEXT UNIQUE,
  ref_type        TEXT,
  ref_id          TEXT,
  cash_session_id TEXT REFERENCES cash_sessions (id),
  business_date   TEXT NOT NULL,
  note            TEXT,
  created_at      INTEGER NOT NULL,
  user_id         TEXT,
  terminal_id     TEXT,
  CHECK ((kind = 'purchase' AND amount > 0)
      OR (kind IN ('payment', 'return') AND amount < 0)
      OR kind IN ('adjustment', 'opening')),
  CHECK (kind <> 'payment' OR method IS NOT NULL)
) STRICT;
CREATE INDEX idx_supplier_ledger_supplier ON supplier_ledger (supplier_id, created_at);
CREATE INDEX idx_supplier_ledger_ref ON supplier_ledger (ref_type, ref_id);
CREATE INDEX idx_supplier_ledger_cash_session_id ON supplier_ledger (cash_session_id);
CREATE INDEX idx_supplier_ledger_business_date ON supplier_ledger (business_date);

CREATE TRIGGER supplier_ledger_follows_the_cache BEFORE INSERT ON supplier_ledger
WHEN NEW.balance_after IS NOT (SELECT balance FROM suppliers WHERE id = NEW.supplier_id) + NEW.amount
BEGIN
  SELECT RAISE(ABORT, 'supplier_ledger balance_after must follow the cached balance');
END;

CREATE TRIGGER supplier_ledger_moves_the_balance AFTER INSERT ON supplier_ledger
BEGIN
  UPDATE suppliers SET balance = NEW.balance_after, updated_at = NEW.created_at
  WHERE id = NEW.supplier_id;
END;

CREATE TRIGGER supplier_ledger_no_update BEFORE UPDATE ON supplier_ledger
BEGIN
  SELECT RAISE(ABORT, 'supplier_ledger is append-only');
END;

CREATE TRIGGER supplier_ledger_no_delete BEFORE DELETE ON supplier_ledger
BEGIN
  SELECT RAISE(ABORT, 'supplier_ledger is append-only');
END;

CREATE TABLE supplier_allocations (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  supplier_id TEXT NOT NULL REFERENCES suppliers (id),
  debit_id    TEXT NOT NULL REFERENCES supplier_ledger (id),
  credit_id   TEXT NOT NULL REFERENCES supplier_ledger (id),
  amount      INTEGER NOT NULL CHECK (amount > 0),
  created_at  INTEGER NOT NULL,
  CHECK (debit_id <> credit_id)
) STRICT;
CREATE INDEX idx_supplier_allocations_supplier_id ON supplier_allocations (supplier_id);
CREATE INDEX idx_supplier_allocations_debit_id ON supplier_allocations (debit_id);
CREATE INDEX idx_supplier_allocations_credit_id ON supplier_allocations (credit_id);

CREATE TRIGGER supplier_allocations_no_update BEFORE UPDATE ON supplier_allocations
BEGIN
  SELECT RAISE(ABORT, 'supplier_allocations is append-only');
END;

CREATE TRIGGER supplier_allocations_no_delete BEFORE DELETE ON supplier_allocations
BEGIN
  SELECT RAISE(ABORT, 'supplier_allocations is append-only');
END;

-- Expenses (FIN-001) ---------------------------------------------------------------------------

-- `cash_group` is the M1 `cash::ExpenseCategory` the drawer movement and the Z report use.
CREATE TABLE expense_categories (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL UNIQUE COLLATE NOCASE,
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  cash_group  TEXT NOT NULL CHECK (cash_group IN ('rent', 'electricity', 'water', 'wages', 'transport',
                                                  'supplies', 'maintenance', 'other')),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;

CREATE TRIGGER expense_categories_no_delete BEFORE DELETE ON expense_categories
BEGIN
  SELECT RAISE(ABORT, 'expense categories are archived, never deleted');
END;

CREATE TABLE expenses (
  id               TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  category_id      TEXT NOT NULL REFERENCES expense_categories (id),
  amount           INTEGER NOT NULL CHECK (amount <> 0),
  method           TEXT NOT NULL CHECK (method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                   'cheque', 'transfer')),
  from_till        INTEGER NOT NULL DEFAULT 0 CHECK (from_till IN (0, 1)),
  cash_session_id  TEXT REFERENCES cash_sessions (id),
  cash_movement_id TEXT REFERENCES cash_movements (id),
  reference        TEXT,
  supplier_id      TEXT REFERENCES suppliers (id),
  note             TEXT,
  reversal_of      TEXT UNIQUE REFERENCES expenses (id),
  business_date    TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  user_id          TEXT NOT NULL,
  terminal_id      TEXT,
  CHECK ((reversal_of IS NULL) = (amount > 0)),
  CHECK (from_till = 0 OR (method = 'cash' AND cash_session_id IS NOT NULL AND cash_movement_id IS NOT NULL))
) STRICT;
CREATE INDEX idx_expenses_category_id ON expenses (category_id);
CREATE INDEX idx_expenses_cash_session_id ON expenses (cash_session_id);
CREATE INDEX idx_expenses_cash_movement_id ON expenses (cash_movement_id);
CREATE INDEX idx_expenses_supplier_id ON expenses (supplier_id);
CREATE INDEX idx_expenses_business_date ON expenses (business_date);

CREATE TRIGGER expenses_no_update BEFORE UPDATE ON expenses
BEGIN
  SELECT RAISE(ABORT, 'expenses are append-only: reverse them');
END;

CREATE TRIGGER expenses_no_delete BEFORE DELETE ON expenses
BEGIN
  SELECT RAISE(ABORT, 'expenses are append-only: reverse them');
END;

-- Lots, serials (PRD-050/051/052/053, STK-004) --------------------------------------------------

CREATE TABLE lots (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id  TEXT NOT NULL REFERENCES products (id),
  lot_number  TEXT NOT NULL CHECK (length(trim(lot_number)) > 0),
  expiry_date TEXT CHECK (expiry_date IS NULL
                          OR expiry_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  supplier_id TEXT REFERENCES suppliers (id),
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  UNIQUE (product_id, lot_number)
) STRICT;
CREATE INDEX idx_lots_supplier_id ON lots (supplier_id);
CREATE INDEX idx_lots_expiry_date ON lots (expiry_date);

CREATE TRIGGER lots_no_delete BEFORE DELETE ON lots
BEGIN
  SELECT RAISE(ABORT, 'lots are never deleted');
END;

CREATE TABLE lot_levels (
  lot_id      TEXT NOT NULL REFERENCES lots (id),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  qty         INTEGER NOT NULL DEFAULT 0,
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (lot_id, location_id)
) STRICT;
CREATE INDEX idx_lot_levels_location_id ON lot_levels (location_id);

-- Like `stock_levels`: the lot cache moves in the statement that appends the movement.
CREATE TRIGGER stock_movements_update_lot_level AFTER INSERT ON stock_movements
WHEN NEW.lot_id IS NOT NULL
BEGIN
  INSERT INTO lot_levels (lot_id, location_id, qty, updated_at)
  VALUES (NEW.lot_id, NEW.location_id, NEW.qty_delta, NEW.created_at)
  ON CONFLICT (lot_id, location_id)
  DO UPDATE SET qty = qty + excluded.qty, updated_at = excluded.updated_at;
END;

CREATE TABLE serials (
  id                   TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id           TEXT NOT NULL REFERENCES products (id),
  serial_no            TEXT NOT NULL CHECK (length(trim(serial_no)) > 0),
  status               TEXT NOT NULL DEFAULT 'in_stock'
                       CHECK (status IN ('in_stock', 'sold', 'returned', 'returned_to_supplier')),
  location_id          TEXT REFERENCES stock_locations (id),
  received_document_id TEXT REFERENCES documents (id),
  sale_line_id         TEXT REFERENCES document_lines (id),
  created_at           INTEGER NOT NULL,
  updated_at           INTEGER NOT NULL,
  UNIQUE (product_id, serial_no),
  CHECK (status <> 'sold' OR sale_line_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_serials_serial_no ON serials (serial_no);
CREATE INDEX idx_serials_location_id ON serials (location_id);
CREATE INDEX idx_serials_received_document_id ON serials (received_document_id);
CREATE INDEX idx_serials_sale_line_id ON serials (sale_line_id);

CREATE TRIGGER serials_no_delete BEFORE DELETE ON serials
BEGIN
  SELECT RAISE(ABORT, 'serial numbers are never deleted');
END;

-- Stock counts (STK-009/010/011/012) ------------------------------------------------------------

CREATE TABLE stock_counts (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  scope       TEXT NOT NULL CHECK (scope IN ('full', 'category', 'products')),
  category_id TEXT REFERENCES categories (id),
  blind       INTEGER NOT NULL DEFAULT 0 CHECK (blind IN (0, 1)),
  status      TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'closed', 'cancelled')),
  note        TEXT,
  started_at  INTEGER NOT NULL,
  closed_at   INTEGER,
  user_id     TEXT NOT NULL,
  closed_by   TEXT,
  CHECK ((status = 'open') = (closed_at IS NULL)),
  CHECK (scope <> 'category' OR category_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_stock_counts_location_id ON stock_counts (location_id);
CREATE UNIQUE INDEX idx_stock_counts_one_open_per_location ON stock_counts (location_id)
  WHERE status = 'open';
CREATE INDEX idx_stock_counts_category_id ON stock_counts (category_id);

CREATE TRIGGER stock_counts_final BEFORE UPDATE ON stock_counts
WHEN OLD.status <> 'open'
BEGIN
  SELECT RAISE(ABORT, 'a closed or cancelled count is final');
END;

CREATE TRIGGER stock_counts_no_delete BEFORE DELETE ON stock_counts
BEGIN
  SELECT RAISE(ABORT, 'stock counts are never deleted');
END;

CREATE TABLE stock_count_lines (
  count_id     TEXT NOT NULL REFERENCES stock_counts (id),
  product_id   TEXT NOT NULL REFERENCES products (id),
  lot_id       TEXT REFERENCES lots (id),
  expected_qty INTEGER NOT NULL,
  counted_qty  INTEGER CHECK (counted_qty IS NULL OR counted_qty >= 0),
  unit_cost    INTEGER NOT NULL DEFAULT 0 CHECK (unit_cost >= 0),
  posted_delta INTEGER,
  counted_at   INTEGER,
  counted_by   TEXT,
  PRIMARY KEY (count_id, product_id)
) STRICT;
CREATE INDEX idx_stock_count_lines_product_id ON stock_count_lines (product_id);
CREATE INDEX idx_stock_count_lines_lot_id ON stock_count_lines (lot_id);

CREATE TRIGGER stock_count_lines_open_insert BEFORE INSERT ON stock_count_lines
WHEN (SELECT status FROM stock_counts WHERE id = NEW.count_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'lines are added to an open count only');
END;

CREATE TRIGGER stock_count_lines_open_update BEFORE UPDATE ON stock_count_lines
WHEN (SELECT status FROM stock_counts WHERE id = OLD.count_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'the lines of a closed count are final');
END;

CREATE TRIGGER stock_count_lines_open_delete BEFORE DELETE ON stock_count_lines
WHEN (SELECT status FROM stock_counts WHERE id = OLD.count_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'the lines of a closed count are final');
END;

-- Transfers (STK-005, DOC-009): the header of a `transfer` document (BT…).
CREATE TABLE stock_transfers (
  document_id      TEXT PRIMARY KEY NOT NULL REFERENCES documents (id),
  from_location_id TEXT NOT NULL REFERENCES stock_locations (id),
  to_location_id   TEXT NOT NULL REFERENCES stock_locations (id),
  driver_name      TEXT,
  vehicle          TEXT,
  CHECK (from_location_id <> to_location_id)
) STRICT;
CREATE INDEX idx_stock_transfers_from ON stock_transfers (from_location_id);
CREATE INDEX idx_stock_transfers_to ON stock_transfers (to_location_id);

CREATE TRIGGER stock_transfers_no_update BEFORE UPDATE ON stock_transfers
BEGIN
  SELECT RAISE(ABORT, 'stock_transfers is append-only');
END;

CREATE TRIGGER stock_transfers_no_delete BEFORE DELETE ON stock_transfers
BEGIN
  SELECT RAISE(ABORT, 'stock_transfers is append-only');
END;

-- Minimum per product and location (STK-014).
CREATE TABLE location_minimums (
  product_id  TEXT NOT NULL REFERENCES products (id),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  min_qty     INTEGER NOT NULL CHECK (min_qty >= 0),
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (product_id, location_id)
) STRICT;
CREATE INDEX idx_location_minimums_location_id ON location_minimums (location_id);

-- Every change of a product's CMUP (STK-016) with what caused it.
CREATE TABLE cost_history (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id  TEXT NOT NULL REFERENCES products (id),
  cost_before INTEGER NOT NULL CHECK (cost_before >= 0),
  cost_after  INTEGER NOT NULL CHECK (cost_after >= 0),
  source_type TEXT NOT NULL,
  source_id   TEXT,
  created_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_cost_history_product ON cost_history (product_id, created_at);

CREATE TRIGGER cost_history_no_update BEFORE UPDATE ON cost_history
BEGIN
  SELECT RAISE(ABORT, 'cost_history is append-only');
END;

CREATE TRIGGER cost_history_no_delete BEFORE DELETE ON cost_history
BEGIN
  SELECT RAISE(ABORT, 'cost_history is append-only');
END;

-- Product extras (PRD-005, 011, 051, 050, 061) and quantity breaks (PRC-004) -------------------

CREATE TABLE product_extras (
  product_id            TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  brand                 TEXT,
  supplier_ref          TEXT,
  alt_refs              TEXT,
  tags                  TEXT,
  attributes            TEXT NOT NULL DEFAULT '{}' CHECK (json_valid(attributes)),
  min_price             INTEGER CHECK (min_price IS NULL OR min_price >= 0),
  track_lots            INTEGER NOT NULL DEFAULT 0 CHECK (track_lots IN (0, 1)),
  track_serials         INTEGER NOT NULL DEFAULT 0 CHECK (track_serials IN (0, 1)),
  preferred_supplier_id TEXT REFERENCES suppliers (id),
  updated_at            INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_product_extras_brand ON product_extras (brand COLLATE NOCASE);
CREATE INDEX idx_product_extras_supplier_ref ON product_extras (supplier_ref);
CREATE INDEX idx_product_extras_preferred_supplier_id ON product_extras (preferred_supplier_id);

CREATE TABLE price_breaks (
  id             TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id     TEXT NOT NULL REFERENCES products (id),
  price_level_id TEXT NOT NULL REFERENCES price_levels (id),
  min_qty        INTEGER NOT NULL CHECK (min_qty > 0),
  price          INTEGER NOT NULL CHECK (price >= 0),
  updated_at     INTEGER NOT NULL,
  UNIQUE (product_id, price_level_id, min_qty)
) STRICT;
CREATE INDEX idx_price_breaks_price_level_id ON price_breaks (price_level_id);

-- Promotions and legal soldes (PRM-003, 021, 022, 024) --------------------------------------------

CREATE TABLE promotions (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  name_ar         TEXT NOT NULL DEFAULT '',
  name_fr         TEXT NOT NULL DEFAULT '',
  name_en         TEXT NOT NULL DEFAULT '',
  kind            TEXT NOT NULL CHECK (kind IN ('percent', 'amount', 'buy_x_get_y', 'bundle_price')),
  campaign        TEXT NOT NULL DEFAULT 'promotion'
                  CHECK (campaign IN ('promotion', 'solde', 'liquidation', 'deballage')),
  is_legal_soldes INTEGER GENERATED ALWAYS AS (campaign = 'solde') VIRTUAL,
  trigger_mode    TEXT NOT NULL DEFAULT 'auto' CHECK (trigger_mode IN ('auto', 'manual')),
  scope           TEXT NOT NULL DEFAULT '{"kind":"all"}' CHECK (json_valid(scope)),
  value           INTEGER NOT NULL DEFAULT 0 CHECK (value >= 0),
  buy_qty         INTEGER CHECK (buy_qty IS NULL OR buy_qty > 0),
  get_qty         INTEGER CHECK (get_qty IS NULL OR get_qty > 0),
  min_qty         INTEGER NOT NULL DEFAULT 0 CHECK (min_qty >= 0),
  priority        INTEGER NOT NULL DEFAULT 0 CHECK (priority BETWEEN -100 AND 100),
  starts_on       TEXT CHECK (starts_on IS NULL
                              OR starts_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  ends_on         TEXT CHECK (ends_on IS NULL
                              OR ends_on GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]'),
  active          INTEGER NOT NULL DEFAULT 1 CHECK (active IN (0, 1)),
  archived_at     INTEGER,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL,
  created_by      TEXT,
  CHECK (starts_on IS NULL OR ends_on IS NULL OR starts_on <= ends_on),
  CHECK (kind <> 'percent' OR value BETWEEN 1 AND 10000),
  CHECK (kind <> 'amount' OR value > 0),
  CHECK (kind <> 'buy_x_get_y' OR (buy_qty IS NOT NULL AND get_qty IS NOT NULL)),
  CHECK (kind <> 'bundle_price' OR (buy_qty IS NOT NULL AND value > 0)),
  CHECK (trigger_mode = 'auto' OR kind = 'percent')
) STRICT;
CREATE INDEX idx_promotions_dates ON promotions (starts_on, ends_on) WHERE archived_at IS NULL;

CREATE TRIGGER promotions_no_delete BEFORE DELETE ON promotions
BEGIN
  SELECT RAISE(ABORT, 'promotions are archived, never deleted');
END;

-- The price practised before the campaign and the stock put on sale (PRM-021/022 declaration).
CREATE TABLE promotion_products (
  promotion_id    TEXT NOT NULL REFERENCES promotions (id),
  product_id      TEXT NOT NULL REFERENCES products (id),
  reference_price INTEGER NOT NULL CHECK (reference_price >= 0),
  stock_at_start  INTEGER NOT NULL DEFAULT 0,
  created_at      INTEGER NOT NULL,
  PRIMARY KEY (promotion_id, product_id)
) STRICT;
CREATE INDEX idx_promotion_products_product_id ON promotion_products (product_id);

CREATE TRIGGER promotion_products_no_update BEFORE UPDATE ON promotion_products
BEGIN
  SELECT RAISE(ABORT, 'the reference prices of a campaign are evidence: never changed');
END;

CREATE TRIGGER promotion_products_no_delete BEFORE DELETE ON promotion_products
BEGIN
  SELECT RAISE(ABORT, 'the reference prices of a campaign are evidence: never deleted');
END;

-- Which automatic discount a sold line received (RPT-015, PRM-022).
CREATE TABLE sale_line_promotions (
  line_id      TEXT PRIMARY KEY NOT NULL REFERENCES document_lines (id),
  document_id  TEXT NOT NULL REFERENCES documents (id),
  promotion_id TEXT REFERENCES promotions (id),
  source       TEXT NOT NULL CHECK (source IN ('promotion', 'customer')),
  amount       INTEGER NOT NULL CHECK (amount > 0),
  CHECK ((source = 'promotion') = (promotion_id IS NOT NULL))
) STRICT;
CREATE INDEX idx_sale_line_promotions_document_id ON sale_line_promotions (document_id);
CREATE INDEX idx_sale_line_promotions_promotion_id ON sale_line_promotions (promotion_id);

CREATE TRIGGER sale_line_promotions_no_update BEFORE UPDATE ON sale_line_promotions
BEGIN
  SELECT RAISE(ABORT, 'sale_line_promotions is append-only');
END;

CREATE TRIGGER sale_line_promotions_no_delete BEFORE DELETE ON sale_line_promotions
BEGIN
  SELECT RAISE(ABORT, 'sale_line_promotions is append-only');
END;

-- Imports (IMX-004, 006, 007) --------------------------------------------------------------------

CREATE TABLE import_jobs (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  kind        TEXT NOT NULL CHECK (kind IN ('products', 'categories', 'customers', 'suppliers',
                                            'opening_stock', 'customer_balances',
                                            'supplier_balances', 'count_lines')),
  file_name   TEXT NOT NULL,
  policy      TEXT NOT NULL CHECK (policy IN ('create_only', 'upsert', 'sync')),
  status      TEXT NOT NULL CHECK (status IN ('running', 'done', 'failed', 'undone')),
  mapping     TEXT NOT NULL CHECK (json_valid(mapping)),
  total       INTEGER NOT NULL DEFAULT 0,
  created     INTEGER NOT NULL DEFAULT 0,
  updated     INTEGER NOT NULL DEFAULT 0,
  skipped     INTEGER NOT NULL DEFAULT 0,
  failed      INTEGER NOT NULL DEFAULT 0,
  next_row    INTEGER NOT NULL DEFAULT 0,
  errors      TEXT NOT NULL DEFAULT '[]' CHECK (json_valid(errors)),
  target_id   TEXT,
  created_at  INTEGER NOT NULL,
  finished_at INTEGER,
  user_id     TEXT,
  terminal_id TEXT
) STRICT;
CREATE INDEX idx_import_jobs_created_at ON import_jobs (created_at);

CREATE TRIGGER import_jobs_no_delete BEFORE DELETE ON import_jobs
BEGIN
  SELECT RAISE(ABORT, 'import jobs are never deleted');
END;

-- What each imported row did, with the previous values when it updated a record (undo).
CREATE TABLE import_job_rows (
  job_id    TEXT NOT NULL REFERENCES import_jobs (id),
  row_no    INTEGER NOT NULL CHECK (row_no >= 1),
  entity_id TEXT NOT NULL,
  action    TEXT NOT NULL CHECK (action IN ('created', 'updated', 'posted')),
  before    TEXT CHECK (before IS NULL OR json_valid(before)),
  PRIMARY KEY (job_id, row_no)
) STRICT;
CREATE INDEX idx_import_job_rows_entity_id ON import_job_rows (entity_id);

CREATE TRIGGER import_job_rows_no_update BEFORE UPDATE ON import_job_rows
BEGIN
  SELECT RAISE(ABORT, 'import_job_rows is append-only');
END;

CREATE TRIGGER import_job_rows_no_delete BEFORE DELETE ON import_job_rows
BEGIN
  SELECT RAISE(ABORT, 'import_job_rows is append-only');
END;

-- Cash session state (guards 0002 left out, found by the M1a review) ----------------------------

-- The count sheet is written by the closing transaction while the session is still open.
CREATE TRIGGER cash_counts_session_open BEFORE INSERT ON cash_counts
WHEN (SELECT status FROM cash_sessions WHERE id = NEW.session_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'cash counts are recorded while the session is open');
END;

-- A Z record files a closed session, under that session's own terminal.
CREATE TRIGGER cash_closures_closed_session BEFORE INSERT ON cash_closures
WHEN (SELECT status FROM cash_sessions WHERE id = NEW.session_id) IS NOT 'closed'
  OR (SELECT terminal_id FROM cash_sessions WHERE id = NEW.session_id) IS NOT NEW.terminal_id
BEGIN
  SELECT RAISE(ABORT, 'a closure records a closed session of its own terminal');
END;

-- Seeds: the eight expense groups of M1 as editable categories, fixed ids like 0002's seeds.

INSERT INTO expense_categories (id, code, name_ar, name_fr, name_en, cash_group, sort_order, created_at, updated_at) VALUES
  ('00000000-0000-7000-8000-000000000501', 'rent', 'الإيجار', 'Loyer', 'Rent', 'rent', 1, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000502', 'electricity', 'الكهرباء والغاز', 'Électricité et gaz', 'Electricity and gas', 'electricity', 2, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000503', 'water', 'الماء', 'Eau', 'Water', 'water', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000504', 'wages', 'الأجور', 'Salaires', 'Wages', 'wages', 4, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000505', 'transport', 'النقل', 'Transport', 'Transport', 'transport', 5, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000506', 'supplies', 'اللوازم', 'Fournitures', 'Supplies', 'supplies', 6, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000507', 'maintenance', 'الصيانة', 'Entretien', 'Maintenance', 'maintenance', 7, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000508', 'other', 'أخرى', 'Autres', 'Other', 'other', 8, unixepoch() * 1000, unixepoch() * 1000);
