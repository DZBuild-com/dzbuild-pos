-- M8 vertical packs (master spec §23–§30, mvp-backlog). Forward-only; never edit once committed.
-- Money INTEGER centimes, quantities INTEGER milli-units, precious metal weights INTEGER mg,
-- fineness INTEGER per mille. Every ledger is append-only; balances are sums of their ledger.
-- LAN intent of every table (the M5 sync catalogue classifies it): `shared` is host data every
-- till mirrors; `till(col)` a row owned by the till its `col` names; `parent(fk -> table)` a row
-- owned like its parent row; `host-only` a row written and kept on the main PC.

-- Units the templates use (PRD-031) ------------------------------------------------------------
INSERT OR IGNORE INTO units (id, code, name_ar, name_fr, name_en, decimals, created_at, updated_at) VALUES
  ('00000000-0000-7000-8000-000000000109', 'sachet', 'كيس صغير', 'Sachet', 'Sachet', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000110', 'm3', 'متر مكعب', 'Mètre cube', 'Cubic metre', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000111', 'bag', 'كيس', 'Sac', 'Bag', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000112', 'pallet', 'منصة نقالة', 'Palette', 'Pallet', 0, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000113', 't', 'طن', 'Tonne', 'Tonne', 3, unixepoch() * 1000, unixepoch() * 1000),
  ('00000000-0000-7000-8000-000000000114', 'lot', 'مجموعة', 'Lot', 'Set', 0, unixepoch() * 1000, unixepoch() * 1000);

-- Open-price products: department keys and top-ups, priced when sold (SAL-019, VRT-002) --------
-- LAN: shared.
CREATE TABLE open_price_products (
  product_id TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  created_at INTEGER NOT NULL
) STRICT;

-- LAN: shared.
CREATE TABLE department_keys (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id  TEXT NOT NULL UNIQUE REFERENCES open_price_products (product_id),
  color       TEXT CHECK (color IS NULL OR (length(color) = 7 AND substr(color, 1, 1) = '#')),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;

CREATE TRIGGER department_keys_no_delete BEFORE DELETE ON department_keys
BEGIN
  SELECT RAISE(ABORT, 'department keys are archived, never deleted');
END;

-- Regulated staples: the product's code in the data pack's price ceilings (PRC-012) ------------
-- LAN: shared.
CREATE TABLE product_regulations (
  product_id TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  code       TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 40),
  updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_product_regulations_code ON product_regulations (code);

-- Restaurant (RST-001…009) -------------------------------------------------------------------------
-- LAN: shared.
CREATE TABLE dining_rooms (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 12),
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  grid_cols   INTEGER NOT NULL CHECK (grid_cols BETWEEN 1 AND 12),
  grid_rows   INTEGER NOT NULL CHECK (grid_rows BETWEEN 1 AND 12),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX idx_dining_rooms_code ON dining_rooms (code) WHERE archived_at IS NULL;

CREATE TRIGGER dining_rooms_no_delete BEFORE DELETE ON dining_rooms
BEGIN
  SELECT RAISE(ABORT, 'dining rooms are archived, never deleted');
END;

-- LAN: shared.
CREATE TABLE dining_tables (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  room_id     TEXT NOT NULL REFERENCES dining_rooms (id),
  code        TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 8),
  seats       INTEGER NOT NULL DEFAULT 4 CHECK (seats BETWEEN 1 AND 40),
  shape       TEXT NOT NULL DEFAULT 'square' CHECK (shape IN ('square', 'round', 'rect')),
  grid_col    INTEGER NOT NULL CHECK (grid_col >= 0),
  grid_row    INTEGER NOT NULL CHECK (grid_row >= 0),
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_dining_tables_room_id ON dining_tables (room_id);
CREATE UNIQUE INDEX idx_dining_tables_cell ON dining_tables (room_id, grid_col, grid_row)
  WHERE archived_at IS NULL;
CREATE UNIQUE INDEX idx_dining_tables_code ON dining_tables (code COLLATE NOCASE)
  WHERE archived_at IS NULL;

CREATE TRIGGER dining_tables_no_delete BEFORE DELETE ON dining_tables
BEGIN
  SELECT RAISE(ABORT, 'dining tables are archived, never deleted');
END;

-- LAN: till(terminal_id).
CREATE TABLE dining_tickets (
  id               TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  mode             TEXT NOT NULL CHECK (mode IN ('dine_in', 'takeaway', 'delivery')),
  order_no         INTEGER CHECK (order_no IS NULL OR order_no >= 1),
  business_date    TEXT NOT NULL,
  table_id         TEXT REFERENCES dining_tables (id),
  label            TEXT CHECK (label IS NULL OR length(label) BETWEEN 1 AND 40),
  covers           INTEGER NOT NULL DEFAULT 0 CHECK (covers BETWEEN 0 AND 200),
  waiter_id        TEXT NOT NULL REFERENCES users (id),
  customer_id      TEXT REFERENCES customers (id),
  delivery_address TEXT CHECK (delivery_address IS NULL OR length(delivery_address) <= 200),
  price_level_id   TEXT REFERENCES price_levels (id),
  lang             TEXT NOT NULL DEFAULT 'fr' CHECK (lang IN ('ar', 'fr', 'en')),
  note             TEXT,
  status           TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'billed', 'paid', 'void')),
  total_ttc        INTEGER NOT NULL DEFAULT 0,
  parent_id        TEXT REFERENCES dining_tickets (id),
  document_id      TEXT REFERENCES documents (id),
  opened_at        INTEGER NOT NULL,
  billed_at        INTEGER,
  closed_at        INTEGER,
  terminal_id      TEXT NOT NULL,
  user_id          TEXT NOT NULL,
  created_at       INTEGER NOT NULL,
  updated_at       INTEGER NOT NULL,
  CHECK ((mode = 'dine_in') = (order_no IS NULL)),
  CHECK (mode <> 'dine_in' OR table_id IS NOT NULL OR label IS NOT NULL),
  CHECK (status <> 'paid' OR document_id IS NOT NULL),
  CHECK (status IN ('open', 'billed') OR closed_at IS NOT NULL)
) STRICT;
CREATE INDEX idx_dining_tickets_table_id ON dining_tickets (table_id);
CREATE INDEX idx_dining_tickets_waiter_id ON dining_tickets (waiter_id);
CREATE INDEX idx_dining_tickets_customer_id ON dining_tickets (customer_id);
CREATE INDEX idx_dining_tickets_price_level_id ON dining_tickets (price_level_id);
CREATE INDEX idx_dining_tickets_parent_id ON dining_tickets (parent_id);
CREATE UNIQUE INDEX idx_dining_tickets_document_id ON dining_tickets (document_id)
  WHERE document_id IS NOT NULL;
CREATE INDEX idx_dining_tickets_live ON dining_tickets (status) WHERE status IN ('open', 'billed');
CREATE UNIQUE INDEX idx_dining_tickets_order_no ON dining_tickets (business_date, terminal_id, order_no)
  WHERE order_no IS NOT NULL;

CREATE TRIGGER dining_tickets_final BEFORE UPDATE ON dining_tickets
WHEN OLD.status IN ('paid', 'void')
BEGIN
  SELECT RAISE(ABORT, 'a paid or void table ticket is final');
END;

CREATE TRIGGER dining_tickets_no_delete BEFORE DELETE ON dining_tickets
BEGIN
  SELECT RAISE(ABORT, 'table tickets are voided, never deleted');
END;

-- LAN: shared.
CREATE TABLE modifier_groups (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  min_select  INTEGER NOT NULL DEFAULT 0 CHECK (min_select BETWEEN 0 AND 10),
  max_select  INTEGER NOT NULL DEFAULT 0 CHECK (max_select BETWEEN 0 AND 10),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL,
  CHECK (max_select = 0 OR max_select >= min_select)
) STRICT;

CREATE TRIGGER modifier_groups_no_delete BEFORE DELETE ON modifier_groups
BEGIN
  SELECT RAISE(ABORT, 'modifier groups are archived, never deleted');
END;

-- LAN: shared.
CREATE TABLE modifier_options (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  group_id    TEXT NOT NULL REFERENCES modifier_groups (id),
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  -- A priced option is sold as its own line of this product (stock and TVA follow it).
  product_id  TEXT REFERENCES products (id),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_modifier_options_group_id ON modifier_options (group_id);
CREATE INDEX idx_modifier_options_product_id ON modifier_options (product_id);

CREATE TRIGGER modifier_options_no_delete BEFORE DELETE ON modifier_options
BEGIN
  SELECT RAISE(ABORT, 'modifier options are archived, never deleted');
END;

-- LAN: shared.
CREATE TABLE product_modifier_groups (
  product_id TEXT NOT NULL REFERENCES products (id),
  group_id   TEXT NOT NULL REFERENCES modifier_groups (id),
  sort_order INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (product_id, group_id)
) STRICT;
CREATE INDEX idx_product_modifier_groups_group_id ON product_modifier_groups (group_id);

-- LAN: parent(ticket_id -> dining_tickets).
CREATE TABLE dining_lines (
  id                 TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  ticket_id          TEXT NOT NULL REFERENCES dining_tickets (id),
  position           INTEGER NOT NULL CHECK (position >= 1),
  product_id         TEXT REFERENCES products (id),
  -- The M1a `CartLine` as JSON: the till's pricing prices the ticket exactly like a sale.
  cart_line          TEXT NOT NULL CHECK (json_valid(cart_line)),
  qty                INTEGER NOT NULL CHECK (qty > 0),
  sent_qty           INTEGER NOT NULL DEFAULT 0 CHECK (sent_qty >= 0),
  seat               INTEGER CHECK (seat IS NULL OR seat BETWEEN 1 AND 99),
  course             INTEGER CHECK (course IS NULL OR course BETWEEN 1 AND 5),
  parent_line_id     TEXT REFERENCES dining_lines (id),
  modifier_option_id TEXT REFERENCES modifier_options (id),
  status             TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'voided')),
  void_reason        TEXT CHECK (void_reason IS NULL
                       OR void_reason IN ('mistake', 'customer_changed', 'quality', 'other')),
  comp_reason        TEXT CHECK (comp_reason IS NULL
                       OR comp_reason IN ('offered', 'staff', 'quality', 'other')),
  user_id            TEXT NOT NULL,
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL,
  CHECK (sent_qty <= qty),
  CHECK (status = 'active' OR void_reason IS NOT NULL)
) STRICT;
CREATE INDEX idx_dining_lines_ticket_id ON dining_lines (ticket_id, position);
CREATE INDEX idx_dining_lines_product_id ON dining_lines (product_id);
CREATE INDEX idx_dining_lines_parent_line_id ON dining_lines (parent_line_id);
CREATE INDEX idx_dining_lines_modifier_option_id ON dining_lines (modifier_option_id);

CREATE TRIGGER dining_lines_final_insert BEFORE INSERT ON dining_lines
WHEN (SELECT status FROM dining_tickets WHERE id = NEW.ticket_id) IN ('paid', 'void')
BEGIN
  SELECT RAISE(ABORT, 'a paid or void table ticket is final');
END;

CREATE TRIGGER dining_lines_final_update BEFORE UPDATE ON dining_lines
WHEN (SELECT status FROM dining_tickets WHERE id = OLD.ticket_id) IN ('paid', 'void')
BEGIN
  SELECT RAISE(ABORT, 'a paid or void table ticket is final');
END;

CREATE TRIGGER dining_lines_no_delete BEFORE DELETE ON dining_lines
BEGIN
  SELECT RAISE(ABORT, 'table ticket lines are voided, never deleted');
END;

-- LAN: shared.
CREATE TABLE kitchen_stations (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code        TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 12),
  name_ar     TEXT NOT NULL DEFAULT '',
  name_fr     TEXT NOT NULL DEFAULT '',
  name_en     TEXT NOT NULL DEFAULT '',
  sort_order  INTEGER NOT NULL DEFAULT 0,
  archived_at INTEGER,
  created_at  INTEGER NOT NULL,
  updated_at  INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX idx_kitchen_stations_code ON kitchen_stations (code) WHERE archived_at IS NULL;

CREATE TRIGGER kitchen_stations_no_delete BEFORE DELETE ON kitchen_stations
BEGIN
  SELECT RAISE(ABORT, 'kitchen stations are archived, never deleted');
END;

-- LAN: shared.
CREATE TABLE category_stations (
  category_id TEXT PRIMARY KEY NOT NULL REFERENCES categories (id),
  station_id  TEXT NOT NULL REFERENCES kitchen_stations (id),
  updated_at  INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_category_stations_station_id ON category_stations (station_id);

-- LAN: till(terminal_id).
CREATE TABLE kitchen_tickets (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  ticket_id   TEXT NOT NULL REFERENCES dining_tickets (id),
  -- NULL: no station for these categories, printed on the till's receipt printer.
  station_id  TEXT REFERENCES kitchen_stations (id),
  seq         INTEGER NOT NULL CHECK (seq >= 1),
  kind        TEXT NOT NULL CHECK (kind IN ('order', 'cancel')),
  lines       TEXT NOT NULL CHECK (json_valid(lines) AND json_type(lines) = 'array'
                                   AND json_array_length(lines) > 0),
  printed_at  INTEGER,
  created_at  INTEGER NOT NULL,
  user_id     TEXT NOT NULL,
  terminal_id TEXT NOT NULL,
  UNIQUE (ticket_id, seq)
) STRICT;
CREATE INDEX idx_kitchen_tickets_station_id ON kitchen_tickets (station_id);
CREATE INDEX idx_kitchen_tickets_pending ON kitchen_tickets (terminal_id) WHERE printed_at IS NULL;

-- Only `printed_at` moves, once and alone: every other column is pinned, the owning till included.
CREATE TRIGGER kitchen_tickets_printed_once BEFORE UPDATE ON kitchen_tickets
WHEN OLD.printed_at IS NOT NULL OR NEW.printed_at IS NULL
  OR NEW.lines IS NOT OLD.lines OR NEW.kind IS NOT OLD.kind OR NEW.seq IS NOT OLD.seq
  OR NEW.station_id IS NOT OLD.station_id OR NEW.ticket_id IS NOT OLD.ticket_id
  OR NEW.id IS NOT OLD.id OR NEW.created_at IS NOT OLD.created_at OR NEW.user_id IS NOT OLD.user_id
  OR NEW.terminal_id IS NOT OLD.terminal_id
BEGIN
  SELECT RAISE(ABORT, 'kitchen tickets are append-only; printed_at is set once');
END;

CREATE TRIGGER kitchen_tickets_no_delete BEFORE DELETE ON kitchen_tickets
BEGIN
  SELECT RAISE(ABORT, 'kitchen tickets are append-only');
END;

-- LAN: shared.
CREATE TABLE daily_menu (
  business_date TEXT NOT NULL,
  product_id    TEXT NOT NULL REFERENCES products (id),
  sort_order    INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (business_date, product_id)
) STRICT;
CREATE INDEX idx_daily_menu_product_id ON daily_menu (product_id);

-- Pharmacy (VRT-030, PRD-053, SAL-011 DCI, PDP-009) -------------------------------------------------
-- LAN: shared.
CREATE TABLE pharmacy_products (
  product_id            TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  dci                   TEXT CHECK (dci IS NULL OR length(dci) BETWEEN 1 AND 120),
  dci_key               TEXT,
  controlled            INTEGER NOT NULL DEFAULT 0 CHECK (controlled IN (0, 1)),
  schedule              TEXT CHECK (schedule IS NULL OR length(schedule) BETWEEN 1 AND 40),
  prescription_required INTEGER NOT NULL DEFAULT 0 CHECK (prescription_required IN (0, 1)),
  updated_at            INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_pharmacy_products_dci_key ON pharmacy_products (dci_key);

-- LAN: shared.
CREATE TABLE chifa_prices (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code            TEXT NOT NULL UNIQUE CHECK (length(code) BETWEEN 1 AND 40),
  name            TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 160),
  product_id      TEXT REFERENCES products (id),
  reference_price INTEGER NOT NULL CHECK (reference_price >= 0),
  reimbursable    INTEGER NOT NULL DEFAULT 1 CHECK (reimbursable IN (0, 1)),
  source_file     TEXT,
  created_at      INTEGER NOT NULL,
  updated_at      INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_chifa_prices_product_id ON chifa_prices (product_id);

-- The only table of the product that holds health data (loi 18-07 art. 18): written at the sale
-- of a controlled medicine, read with `pharmacy.register`, never updated nor deleted.
-- LAN: till(terminal_id), reaches the main PC with its sale, never mirrored to other tills.
CREATE TABLE controlled_register (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  year              INTEGER NOT NULL,
  seq               INTEGER NOT NULL CHECK (seq >= 1),
  business_date     TEXT NOT NULL,
  document_id       TEXT NOT NULL REFERENCES documents (id),
  line_id           TEXT NOT NULL REFERENCES document_lines (id),
  product_id        TEXT NOT NULL REFERENCES products (id),
  qty               INTEGER NOT NULL CHECK (qty > 0),
  prescriber_name   TEXT NOT NULL CHECK (length(prescriber_name) BETWEEN 1 AND 120),
  prescriber_ref    TEXT CHECK (prescriber_ref IS NULL OR length(prescriber_ref) <= 40),
  prescription_no   TEXT CHECK (prescription_no IS NULL OR length(prescription_no) <= 40),
  prescription_date TEXT NOT NULL,
  patient_name      TEXT NOT NULL CHECK (length(patient_name) BETWEEN 1 AND 120),
  patient_ref       TEXT CHECK (patient_ref IS NULL OR length(patient_ref) <= 40),
  created_at        INTEGER NOT NULL,
  user_id           TEXT NOT NULL,
  terminal_id       TEXT NOT NULL,
  UNIQUE (year, seq)
) STRICT;
CREATE INDEX idx_controlled_register_document_id ON controlled_register (document_id);
CREATE INDEX idx_controlled_register_line_id ON controlled_register (line_id);
CREATE INDEX idx_controlled_register_product_id ON controlled_register (product_id);
CREATE INDEX idx_controlled_register_business_date ON controlled_register (business_date);

CREATE TRIGGER controlled_register_no_update BEFORE UPDATE ON controlled_register
BEGIN
  SELECT RAISE(ABORT, 'controlled_register is append-only');
END;

CREATE TRIGGER controlled_register_no_delete BEFORE DELETE ON controlled_register
BEGIN
  SELECT RAISE(ABORT, 'controlled_register is append-only');
END;

-- Butcher: carcass cutting yield (VRT-011) --------------------------------------------------------
-- LAN: shared.
CREATE TABLE yield_templates (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  source_product_id TEXT NOT NULL REFERENCES products (id),
  name              TEXT NOT NULL CHECK (length(name) BETWEEN 1 AND 80),
  archived_at       INTEGER,
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_yield_templates_source ON yield_templates (source_product_id);

CREATE TRIGGER yield_templates_no_delete BEFORE DELETE ON yield_templates
BEGIN
  SELECT RAISE(ABORT, 'yield templates are archived, never deleted');
END;

-- LAN: shared.
CREATE TABLE yield_template_cuts (
  template_id TEXT NOT NULL REFERENCES yield_templates (id),
  product_id  TEXT NOT NULL REFERENCES products (id),
  expected_bp INTEGER NOT NULL CHECK (expected_bp BETWEEN 1 AND 10000),
  sort_order  INTEGER NOT NULL DEFAULT 0,
  PRIMARY KEY (template_id, product_id)
) STRICT;
CREATE INDEX idx_yield_template_cuts_product_id ON yield_template_cuts (product_id);

-- LAN: till(terminal_id).
CREATE TABLE yield_runs (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  number            TEXT NOT NULL UNIQUE,
  template_id       TEXT REFERENCES yield_templates (id),
  source_product_id TEXT NOT NULL REFERENCES products (id),
  location_id       TEXT NOT NULL REFERENCES stock_locations (id),
  input_qty         INTEGER NOT NULL CHECK (input_qty > 0),
  input_cost        INTEGER NOT NULL CHECK (input_cost >= 0),
  method            TEXT NOT NULL CHECK (method IN ('value', 'weight')),
  output_qty        INTEGER NOT NULL CHECK (output_qty > 0),
  loss_qty          INTEGER NOT NULL CHECK (loss_qty >= 0),
  business_date     TEXT NOT NULL,
  note              TEXT,
  created_at        INTEGER NOT NULL,
  user_id           TEXT NOT NULL,
  terminal_id       TEXT NOT NULL,
  CHECK (output_qty + loss_qty = input_qty)
) STRICT;
CREATE INDEX idx_yield_runs_template_id ON yield_runs (template_id);
CREATE INDEX idx_yield_runs_source ON yield_runs (source_product_id);
CREATE INDEX idx_yield_runs_location_id ON yield_runs (location_id);
CREATE INDEX idx_yield_runs_business_date ON yield_runs (business_date);

-- LAN: parent(run_id -> yield_runs).
CREATE TABLE yield_run_cuts (
  run_id       TEXT NOT NULL REFERENCES yield_runs (id),
  product_id   TEXT NOT NULL REFERENCES products (id),
  qty          INTEGER NOT NULL CHECK (qty > 0),
  expected_qty INTEGER NOT NULL CHECK (expected_qty >= 0),
  cost         INTEGER NOT NULL CHECK (cost >= 0),
  unit_cost    INTEGER NOT NULL CHECK (unit_cost >= 0),
  movement_id  TEXT NOT NULL REFERENCES stock_movements (id),
  PRIMARY KEY (run_id, product_id)
) STRICT;
CREATE INDEX idx_yield_run_cuts_product_id ON yield_run_cuts (product_id);
CREATE INDEX idx_yield_run_cuts_movement_id ON yield_run_cuts (movement_id);

CREATE TRIGGER yield_runs_no_update BEFORE UPDATE ON yield_runs
BEGIN
  SELECT RAISE(ABORT, 'yield_runs is append-only');
END;
CREATE TRIGGER yield_runs_no_delete BEFORE DELETE ON yield_runs
BEGIN
  SELECT RAISE(ABORT, 'yield_runs is append-only');
END;
CREATE TRIGGER yield_run_cuts_no_update BEFORE UPDATE ON yield_run_cuts
BEGIN
  SELECT RAISE(ABORT, 'yield_run_cuts is append-only');
END;
CREATE TRIGGER yield_run_cuts_no_delete BEFORE DELETE ON yield_run_cuts
BEGIN
  SELECT RAISE(ABORT, 'yield_run_cuts is append-only');
END;

-- Flexy float per operator (VRT-002) -------------------------------------------------------------
-- LAN: till(terminal_id); the float of an operator is the sum over every till.
CREATE TABLE flexy_float (
  id               TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  operator         TEXT NOT NULL CHECK (length(operator) BETWEEN 1 AND 20),
  kind             TEXT NOT NULL CHECK (kind IN ('purchase', 'sale', 'reversal', 'adjustment')),
  amount           INTEGER NOT NULL CHECK (amount <> 0),
  cost             INTEGER NOT NULL DEFAULT 0 CHECK (cost >= 0),
  document_id      TEXT REFERENCES documents (id),
  line_id          TEXT REFERENCES document_lines (id),
  phone_tail       TEXT CHECK (phone_tail IS NULL OR length(phone_tail) = 4),
  cash_movement_id TEXT REFERENCES cash_movements (id),
  business_date    TEXT NOT NULL,
  note             TEXT,
  created_at       INTEGER NOT NULL,
  user_id          TEXT NOT NULL,
  terminal_id      TEXT NOT NULL,
  CHECK ((kind = 'purchase' AND amount > 0) OR (kind = 'sale' AND amount < 0)
      OR (kind = 'reversal' AND amount > 0) OR kind = 'adjustment')
) STRICT;
CREATE INDEX idx_flexy_float_operator ON flexy_float (operator, created_at);
CREATE INDEX idx_flexy_float_document_id ON flexy_float (document_id);
CREATE INDEX idx_flexy_float_line_id ON flexy_float (line_id);
CREATE INDEX idx_flexy_float_cash_movement_id ON flexy_float (cash_movement_id);

CREATE TRIGGER flexy_float_never_negative BEFORE INSERT ON flexy_float
WHEN NEW.amount < 0
  AND (SELECT COALESCE(SUM(amount), 0) FROM flexy_float WHERE operator = NEW.operator) + NEW.amount < 0
BEGIN
  SELECT RAISE(ABORT, 'flexy float below zero');
END;
CREATE TRIGGER flexy_float_no_update BEFORE UPDATE ON flexy_float
BEGIN
  SELECT RAISE(ABORT, 'flexy_float is append-only');
END;
CREATE TRIGGER flexy_float_no_delete BEFORE DELETE ON flexy_float
BEGIN
  SELECT RAISE(ABORT, 'flexy_float is append-only');
END;

-- Service tickets: repairs, pressing, tailoring (ORD-005, ORD-006) ---------------------------------
-- LAN: till(terminal_id), the till that took the item in.
CREATE TABLE service_tickets (
  id            TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  number        TEXT NOT NULL UNIQUE,
  kind          TEXT NOT NULL CHECK (kind IN ('repair', 'pressing', 'tailoring', 'other')),
  customer_id   TEXT NOT NULL REFERENCES customers (id),
  item          TEXT NOT NULL CHECK (length(item) BETWEEN 1 AND 120),
  brand         TEXT CHECK (brand IS NULL OR length(brand) <= 60),
  model         TEXT CHECK (model IS NULL OR length(model) <= 60),
  serial_no     TEXT CHECK (serial_no IS NULL OR length(serial_no) <= 40),
  accessories   TEXT CHECK (accessories IS NULL OR length(accessories) <= 200),
  condition     TEXT CHECK (condition IS NULL OR length(condition) <= 300),
  fault         TEXT NOT NULL CHECK (length(fault) BETWEEN 1 AND 500),
  estimate      INTEGER CHECK (estimate IS NULL OR estimate >= 0),
  promised_on   TEXT,
  technician_id TEXT REFERENCES users (id),
  status        TEXT NOT NULL DEFAULT 'received' CHECK (status IN ('received', 'diagnosing',
                  'waiting_parts', 'in_progress', 'ready', 'delivered', 'returned_unrepaired', 'cancelled')),
  ready_at      INTEGER,
  closed_at     INTEGER,
  document_id   TEXT REFERENCES documents (id),
  lang          TEXT NOT NULL DEFAULT 'fr' CHECK (lang IN ('ar', 'fr', 'en')),
  business_date TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  updated_at    INTEGER NOT NULL,
  user_id       TEXT NOT NULL,
  terminal_id   TEXT NOT NULL,
  CHECK (status <> 'delivered' OR document_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_service_tickets_customer_id ON service_tickets (customer_id);
CREATE INDEX idx_service_tickets_technician_id ON service_tickets (technician_id);
CREATE INDEX idx_service_tickets_document_id ON service_tickets (document_id);
CREATE INDEX idx_service_tickets_status ON service_tickets (status, promised_on);
CREATE INDEX idx_service_tickets_serial_no ON service_tickets (serial_no);

CREATE TRIGGER service_tickets_final BEFORE UPDATE ON service_tickets
WHEN OLD.status IN ('delivered', 'returned_unrepaired', 'cancelled')
BEGIN
  SELECT RAISE(ABORT, 'a closed service ticket is final');
END;
CREATE TRIGGER service_tickets_no_delete BEFORE DELETE ON service_tickets
BEGIN
  SELECT RAISE(ABORT, 'service tickets are cancelled, never deleted');
END;

-- LAN: parent(ticket_id -> service_tickets).
CREATE TABLE service_ticket_events (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  ticket_id  TEXT NOT NULL REFERENCES service_tickets (id),
  status     TEXT NOT NULL,
  note       TEXT,
  created_at INTEGER NOT NULL,
  user_id    TEXT NOT NULL
) STRICT;
CREATE INDEX idx_service_ticket_events_ticket_id ON service_ticket_events (ticket_id, created_at);
CREATE TRIGGER service_ticket_events_no_update BEFORE UPDATE ON service_ticket_events
BEGIN
  SELECT RAISE(ABORT, 'service_ticket_events is append-only');
END;
CREATE TRIGGER service_ticket_events_no_delete BEFORE DELETE ON service_ticket_events
BEGIN
  SELECT RAISE(ABORT, 'service_ticket_events is append-only');
END;

-- The parts and labour planned on a ticket, sold at pickup; replaced as a set until then.
-- LAN: parent(ticket_id -> service_tickets).
CREATE TABLE service_ticket_items (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  ticket_id   TEXT NOT NULL REFERENCES service_tickets (id),
  position    INTEGER NOT NULL CHECK (position >= 1),
  product_id  TEXT NOT NULL REFERENCES products (id),
  qty         INTEGER NOT NULL CHECK (qty > 0),
  unit_price  INTEGER CHECK (unit_price IS NULL OR unit_price >= 0),
  created_at  INTEGER NOT NULL,
  user_id     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_service_ticket_items_ticket_id ON service_ticket_items (ticket_id, position);
CREATE INDEX idx_service_ticket_items_product_id ON service_ticket_items (product_id);
CREATE TRIGGER service_ticket_items_frozen_insert BEFORE INSERT ON service_ticket_items
WHEN (SELECT status FROM service_tickets WHERE id = NEW.ticket_id)
     IN ('delivered', 'returned_unrepaired', 'cancelled')
BEGIN
  SELECT RAISE(ABORT, 'a closed service ticket is final');
END;
CREATE TRIGGER service_ticket_items_frozen_delete BEFORE DELETE ON service_ticket_items
WHEN (SELECT status FROM service_tickets WHERE id = OLD.ticket_id)
     IN ('delivered', 'returned_unrepaired', 'cancelled')
BEGIN
  SELECT RAISE(ABORT, 'a closed service ticket is final');
END;
CREATE TRIGGER service_ticket_items_no_update BEFORE UPDATE ON service_ticket_items
BEGIN
  SELECT RAISE(ABORT, 'service ticket items are replaced, never updated');
END;

-- Deposits (arrhes) held for a customer order or a service ticket (CRD-007, ORD-003) -------------
-- `owner_id` names a `documents` row (customer_order) or a `service_tickets` row: no FK, the
-- service checks it. The deposit's own receipt `AC…` carries the droit de timbre of its cash.
-- LAN: till(terminal_id); the amount held for an owner is the sum over every till.
CREATE TABLE deposits (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  owner_type      TEXT NOT NULL CHECK (owner_type IN ('customer_order', 'service_ticket')),
  owner_id        TEXT NOT NULL CHECK (length(owner_id) = 36),
  customer_id     TEXT REFERENCES customers (id),
  kind            TEXT NOT NULL CHECK (kind IN ('deposit', 'applied', 'refund')),
  amount          INTEGER NOT NULL CHECK (amount <> 0),
  method          TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                          'cheque', 'transfer', 'voucher')),
  reference       TEXT,
  cheque_bank     TEXT,
  cheque_date     TEXT,
  stamp_duty      INTEGER NOT NULL DEFAULT 0 CHECK (stamp_duty >= 0),
  stamp_charged   INTEGER NOT NULL DEFAULT 0 CHECK (stamp_charged BETWEEN 0 AND stamp_duty),
  receipt_number  TEXT UNIQUE,
  cash_session_id TEXT REFERENCES cash_sessions (id),
  document_id     TEXT REFERENCES documents (id),
  business_date   TEXT NOT NULL,
  note            TEXT,
  created_at      INTEGER NOT NULL,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  CHECK ((kind = 'deposit' AND amount > 0 AND method IS NOT NULL AND receipt_number IS NOT NULL)
      OR (kind IN ('applied', 'refund') AND amount < 0)),
  CHECK (kind <> 'applied' OR document_id IS NOT NULL),
  CHECK (kind = 'deposit' OR (stamp_duty = 0 AND receipt_number IS NULL))
) STRICT;
CREATE INDEX idx_deposits_owner ON deposits (owner_type, owner_id);
CREATE INDEX idx_deposits_customer_id ON deposits (customer_id);
CREATE INDEX idx_deposits_cash_session_id ON deposits (cash_session_id);
CREATE INDEX idx_deposits_document_id ON deposits (document_id);

CREATE TRIGGER deposits_never_negative BEFORE INSERT ON deposits
WHEN NEW.amount < 0
  AND (SELECT COALESCE(SUM(amount), 0) FROM deposits
       WHERE owner_type = NEW.owner_type AND owner_id = NEW.owner_id) + NEW.amount < 0
BEGIN
  SELECT RAISE(ABORT, 'deposit held below zero');
END;
CREATE TRIGGER deposits_no_update BEFORE UPDATE ON deposits
BEGIN
  SELECT RAISE(ABORT, 'deposits is append-only');
END;
CREATE TRIGGER deposits_no_delete BEFORE DELETE ON deposits
BEGIN
  SELECT RAISE(ABORT, 'deposits is append-only');
END;

-- Instalment sales (TMP-001…009): the contract is a validated kridi sale; payments are M2
-- versements settling its `sale_credit` entry. Guarantees are neutral documents, never cheques.
-- LAN: till(terminal_id), with its contract sale.
CREATE TABLE instalment_plans (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  number          TEXT NOT NULL UNIQUE,
  customer_id     TEXT NOT NULL REFERENCES customers (id),
  document_id     TEXT NOT NULL UNIQUE REFERENCES documents (id),
  credit_entry_id TEXT NOT NULL UNIQUE REFERENCES customer_ledger (id),
  total           INTEGER NOT NULL CHECK (total > 0),
  down_payment    INTEGER NOT NULL CHECK (down_payment >= 0),
  financed        INTEGER NOT NULL CHECK (financed > 0),
  count           INTEGER NOT NULL CHECK (count BETWEEN 1 AND 60),
  first_due_on    TEXT NOT NULL,
  note            TEXT,
  business_date   TEXT NOT NULL,
  created_at      INTEGER NOT NULL,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  CHECK (down_payment + financed = total)
) STRICT;
CREATE INDEX idx_instalment_plans_customer_id ON instalment_plans (customer_id);
CREATE TRIGGER instalment_plans_no_update BEFORE UPDATE ON instalment_plans
BEGIN
  SELECT RAISE(ABORT, 'a signed instalment contract is immutable');
END;
CREATE TRIGGER instalment_plans_no_delete BEFORE DELETE ON instalment_plans
BEGIN
  SELECT RAISE(ABORT, 'a signed instalment contract is immutable');
END;

-- LAN: parent(plan_id -> instalment_plans).
CREATE TABLE instalment_dues (
  plan_id TEXT NOT NULL REFERENCES instalment_plans (id),
  seq     INTEGER NOT NULL CHECK (seq >= 1),
  due_on  TEXT NOT NULL,
  amount  INTEGER NOT NULL CHECK (amount > 0),
  PRIMARY KEY (plan_id, seq)
) STRICT;
CREATE INDEX idx_instalment_dues_due_on ON instalment_dues (due_on);
CREATE TRIGGER instalment_dues_no_update BEFORE UPDATE ON instalment_dues
BEGIN
  SELECT RAISE(ABORT, 'a signed schedule is immutable');
END;
CREATE TRIGGER instalment_dues_no_delete BEFORE DELETE ON instalment_dues
BEGIN
  SELECT RAISE(ABORT, 'a signed schedule is immutable');
END;

-- LAN: parent(plan_id -> instalment_plans).
CREATE TABLE instalment_guarantees (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  plan_id     TEXT NOT NULL REFERENCES instalment_plans (id),
  -- TMP-009: a closed list without any cheque.
  kind        TEXT NOT NULL CHECK (kind IN ('id_document', 'work_certificate', 'pay_slip',
                'ccp_account', 'residence_certificate', 'guarantor', 'other')),
  holder_name TEXT CHECK (holder_name IS NULL OR length(holder_name) <= 120),
  reference   TEXT CHECK (reference IS NULL OR length(reference) <= 60),
  phone       TEXT CHECK (phone IS NULL OR length(phone) <= 20),
  note        TEXT CHECK (note IS NULL OR length(note) <= 200),
  created_at  INTEGER NOT NULL,
  user_id     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_instalment_guarantees_plan_id ON instalment_guarantees (plan_id);
CREATE TRIGGER instalment_guarantees_no_update BEFORE UPDATE ON instalment_guarantees
BEGIN
  SELECT RAISE(ABORT, 'instalment_guarantees is append-only');
END;
CREATE TRIGGER instalment_guarantees_no_delete BEFORE DELETE ON instalment_guarantees
BEGIN
  SELECT RAISE(ABORT, 'instalment_guarantees is append-only');
END;

-- LAN: parent(plan_id -> instalment_plans).
CREATE TABLE instalment_events (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  plan_id    TEXT NOT NULL REFERENCES instalment_plans (id),
  kind       TEXT NOT NULL CHECK (kind IN ('reminder_given', 'note')),
  due_seq    INTEGER,
  note       TEXT CHECK (note IS NULL OR length(note) <= 200),
  created_at INTEGER NOT NULL,
  user_id    TEXT NOT NULL
) STRICT;
CREATE INDEX idx_instalment_events_plan_id ON instalment_events (plan_id, created_at);
CREATE TRIGGER instalment_events_no_update BEFORE UPDATE ON instalment_events
BEGIN
  SELECT RAISE(ABORT, 'instalment_events is append-only');
END;
CREATE TRIGGER instalment_events_no_delete BEFORE DELETE ON instalment_events
BEGIN
  SELECT RAISE(ABORT, 'instalment_events is append-only');
END;

-- Wholesale (PRC-003, PRD-033, VRT-100, DOC-030) --------------------------------------------------
-- LAN: shared.
CREATE TABLE customer_prices (
  customer_id TEXT NOT NULL REFERENCES customers (id),
  product_id  TEXT NOT NULL REFERENCES products (id),
  price       INTEGER NOT NULL CHECK (price >= 0),
  updated_at  INTEGER NOT NULL,
  PRIMARY KEY (customer_id, product_id)
) STRICT;
CREATE INDEX idx_customer_prices_product_id ON customer_prices (product_id);

-- LAN: shared.
CREATE TABLE product_wholesale (
  product_id    TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  min_order_qty INTEGER CHECK (min_order_qty IS NULL OR min_order_qty > 0),
  updated_at    INTEGER NOT NULL
) STRICT;

-- LAN: host-only, with the company-series delivery notes it carries.
CREATE TABLE delivery_rounds (
  id                TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  number            TEXT NOT NULL UNIQUE,
  business_date     TEXT NOT NULL,
  driver_name       TEXT NOT NULL CHECK (length(driver_name) BETWEEN 1 AND 80),
  driver_user_id    TEXT REFERENCES users (id),
  vehicle_plate     TEXT CHECK (vehicle_plate IS NULL OR length(vehicle_plate) <= 20),
  van_location_id   TEXT REFERENCES stock_locations (id),
  status            TEXT NOT NULL DEFAULT 'planned' CHECK (status IN ('planned', 'out', 'closed')),
  note              TEXT,
  started_at        INTEGER,
  closed_at         INTEGER,
  closed_session_id TEXT REFERENCES cash_sessions (id),
  created_at        INTEGER NOT NULL,
  updated_at        INTEGER NOT NULL,
  user_id           TEXT NOT NULL,
  terminal_id       TEXT NOT NULL,
  CHECK (status <> 'closed' OR closed_at IS NOT NULL)
) STRICT;
CREATE INDEX idx_delivery_rounds_driver_user_id ON delivery_rounds (driver_user_id);
CREATE INDEX idx_delivery_rounds_van_location_id ON delivery_rounds (van_location_id);
CREATE INDEX idx_delivery_rounds_status ON delivery_rounds (status, business_date);
CREATE INDEX idx_delivery_rounds_closed_session_id ON delivery_rounds (closed_session_id);
CREATE TRIGGER delivery_rounds_final BEFORE UPDATE ON delivery_rounds
WHEN OLD.status = 'closed'
BEGIN
  SELECT RAISE(ABORT, 'a closed delivery round is final');
END;
CREATE TRIGGER delivery_rounds_no_delete BEFORE DELETE ON delivery_rounds
BEGIN
  SELECT RAISE(ABORT, 'delivery rounds are closed, never deleted');
END;

-- LAN: host-only, parent(round_id -> delivery_rounds).
CREATE TABLE delivery_round_stops (
  round_id        TEXT NOT NULL REFERENCES delivery_rounds (id),
  seq             INTEGER NOT NULL CHECK (seq >= 1),
  document_id     TEXT NOT NULL REFERENCES documents (id),
  customer_id     TEXT REFERENCES customers (id),
  amount_due      INTEGER NOT NULL CHECK (amount_due >= 0),
  status          TEXT NOT NULL DEFAULT 'pending' CHECK (status IN ('pending', 'delivered', 'failed')),
  collected       INTEGER NOT NULL DEFAULT 0 CHECK (collected >= 0),
  method          TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                          'cheque', 'transfer')),
  reference       TEXT,
  cheque_bank     TEXT,
  cheque_date     TEXT,
  ledger_entry_id TEXT REFERENCES customer_ledger (id),
  note            TEXT,
  updated_at      INTEGER NOT NULL,
  PRIMARY KEY (round_id, seq),
  CHECK (collected = 0 OR method IS NOT NULL)
) STRICT;
CREATE INDEX idx_delivery_round_stops_document_id ON delivery_round_stops (document_id);
CREATE INDEX idx_delivery_round_stops_customer_id ON delivery_round_stops (customer_id);
CREATE INDEX idx_delivery_round_stops_ledger_entry_id ON delivery_round_stops (ledger_entry_id);
-- The stops of a closed round never move again, their versement link included: the round's close
-- links each stop to its versement while the round is still out, then closes it.
CREATE TRIGGER delivery_round_stops_frozen BEFORE UPDATE ON delivery_round_stops
WHEN (SELECT status FROM delivery_rounds WHERE id = OLD.round_id) = 'closed'
BEGIN
  SELECT RAISE(ABORT, 'a closed delivery round is final');
END;
CREATE TRIGGER delivery_round_stops_frozen_insert BEFORE INSERT ON delivery_round_stops
WHEN (SELECT status FROM delivery_rounds WHERE id = NEW.round_id) = 'closed'
BEGIN
  SELECT RAISE(ABORT, 'a closed delivery round is final');
END;
CREATE TRIGGER delivery_round_stops_no_delete BEFORE DELETE ON delivery_round_stops
WHEN (SELECT status FROM delivery_rounds WHERE id = OLD.round_id) <> 'planned'
BEGIN
  SELECT RAISE(ABORT, 'stops leave a round only while it is planned');
END;

-- Customer orders, layaway, reservations, appointments (ORD-001…003, ORD-009, CRD-008) -----------
-- LAN: parent(document_id -> documents).
CREATE TABLE customer_orders (
  document_id           TEXT PRIMARY KEY NOT NULL REFERENCES documents (id),
  kind                  TEXT NOT NULL CHECK (kind IN ('order', 'layaway')),
  due_on                TEXT,
  status                TEXT NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'ready', 'fulfilled', 'closed')),
  fulfilled_document_id TEXT REFERENCES documents (id),
  created_at            INTEGER NOT NULL,
  updated_at            INTEGER NOT NULL,
  CHECK (status <> 'fulfilled' OR fulfilled_document_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_customer_orders_fulfilled ON customer_orders (fulfilled_document_id);
CREATE INDEX idx_customer_orders_status ON customer_orders (status, due_on);
CREATE TRIGGER customer_orders_final BEFORE UPDATE ON customer_orders
WHEN OLD.status IN ('fulfilled', 'closed')
BEGIN
  SELECT RAISE(ABORT, 'a fulfilled or closed order is final');
END;
CREATE TRIGGER customer_orders_no_delete BEFORE DELETE ON customer_orders
BEGIN
  SELECT RAISE(ABORT, 'orders are closed, never deleted');
END;

-- LAN: parent(document_id -> documents).
CREATE TABLE order_events (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  document_id TEXT NOT NULL REFERENCES documents (id),
  status      TEXT NOT NULL,
  note        TEXT,
  created_at  INTEGER NOT NULL,
  user_id     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_order_events_document_id ON order_events (document_id, created_at);
CREATE TRIGGER order_events_no_update BEFORE UPDATE ON order_events
BEGIN
  SELECT RAISE(ABORT, 'order_events is append-only');
END;
CREATE TRIGGER order_events_no_delete BEFORE DELETE ON order_events
BEGIN
  SELECT RAISE(ABORT, 'order_events is append-only');
END;

-- LAN: parent(owner_id -> documents), the customer order holding the goods.
CREATE TABLE stock_reservations (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  product_id  TEXT NOT NULL REFERENCES products (id),
  location_id TEXT NOT NULL REFERENCES stock_locations (id),
  qty         INTEGER NOT NULL CHECK (qty > 0),
  owner_type  TEXT NOT NULL CHECK (owner_type IN ('customer_order')),
  owner_id    TEXT NOT NULL CHECK (length(owner_id) = 36),
  created_at  INTEGER NOT NULL,
  released_at INTEGER,
  user_id     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_stock_reservations_active ON stock_reservations (product_id, location_id)
  WHERE released_at IS NULL;
CREATE INDEX idx_stock_reservations_location_id ON stock_reservations (location_id);
CREATE INDEX idx_stock_reservations_owner ON stock_reservations (owner_type, owner_id);
-- Only `released_at` moves, once and alone: every other column is pinned.
CREATE TRIGGER stock_reservations_released_once BEFORE UPDATE ON stock_reservations
WHEN OLD.released_at IS NOT NULL OR NEW.released_at IS NULL
  OR NEW.qty IS NOT OLD.qty OR NEW.product_id IS NOT OLD.product_id
  OR NEW.location_id IS NOT OLD.location_id OR NEW.owner_id IS NOT OLD.owner_id
  OR NEW.id IS NOT OLD.id OR NEW.owner_type IS NOT OLD.owner_type
  OR NEW.created_at IS NOT OLD.created_at OR NEW.user_id IS NOT OLD.user_id
BEGIN
  SELECT RAISE(ABORT, 'a reservation is released once and never changed');
END;
CREATE TRIGGER stock_reservations_no_delete BEFORE DELETE ON stock_reservations
BEGIN
  SELECT RAISE(ABORT, 'reservations are released, never deleted');
END;

-- LAN: shared.
CREATE TABLE appointments (
  id                 TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  customer_id        TEXT REFERENCES customers (id),
  customer_name      TEXT NOT NULL CHECK (length(customer_name) BETWEEN 1 AND 120),
  phone              TEXT CHECK (phone IS NULL OR length(phone) <= 20),
  service_product_id TEXT REFERENCES products (id),
  staff_user_id      TEXT REFERENCES users (id),
  starts_at          INTEGER NOT NULL,
  duration_min       INTEGER NOT NULL CHECK (duration_min BETWEEN 5 AND 600),
  status             TEXT NOT NULL DEFAULT 'booked' CHECK (status IN ('booked', 'done', 'cancelled', 'no_show')),
  note               TEXT CHECK (note IS NULL OR length(note) <= 200),
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL,
  user_id            TEXT NOT NULL
) STRICT;
CREATE INDEX idx_appointments_customer_id ON appointments (customer_id);
CREATE INDEX idx_appointments_service_product_id ON appointments (service_product_id);
CREATE INDEX idx_appointments_staff_user_id ON appointments (staff_user_id);
CREATE INDEX idx_appointments_starts_at ON appointments (starts_at);

-- Loyalty and vouchers (LOY-001…006, PAY-010/012) --------------------------------------------------
-- LAN: shared.
CREATE TABLE loyalty_cards (
  code        TEXT PRIMARY KEY NOT NULL CHECK (length(code) BETWEEN 8 AND 20),
  customer_id TEXT NOT NULL REFERENCES customers (id),
  issued_at   INTEGER NOT NULL,
  blocked_at  INTEGER
) STRICT;
CREATE UNIQUE INDEX idx_loyalty_cards_live ON loyalty_cards (customer_id) WHERE blocked_at IS NULL;
-- Only `blocked_at` moves, once and alone: every other column is pinned.
CREATE TRIGGER loyalty_cards_blocked_once BEFORE UPDATE ON loyalty_cards
WHEN OLD.blocked_at IS NOT NULL OR NEW.code IS NOT OLD.code OR NEW.customer_id IS NOT OLD.customer_id
  OR NEW.issued_at IS NOT OLD.issued_at
BEGIN
  SELECT RAISE(ABORT, 'a loyalty card is blocked once and never changed');
END;
CREATE TRIGGER loyalty_cards_no_delete BEFORE DELETE ON loyalty_cards
BEGIN
  SELECT RAISE(ABORT, 'loyalty cards are blocked, never deleted');
END;

-- LAN: till(terminal_id), the till that issued the voucher.
CREATE TABLE gift_vouchers (
  id                 TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code               TEXT NOT NULL UNIQUE,
  barcode            TEXT NOT NULL UNIQUE CHECK (length(barcode) = 13),
  kind               TEXT NOT NULL CHECK (kind IN ('gift', 'exchange', 'loyalty')),
  initial_amount     INTEGER NOT NULL CHECK (initial_amount > 0),
  issued_on          TEXT NOT NULL,
  expires_on         TEXT NOT NULL,
  customer_id        TEXT REFERENCES customers (id),
  buyer_name         TEXT CHECK (buyer_name IS NULL OR length(buyer_name) <= 120),
  source_document_id TEXT REFERENCES documents (id),
  created_at         INTEGER NOT NULL,
  user_id            TEXT NOT NULL,
  terminal_id        TEXT NOT NULL,
  CHECK (expires_on >= issued_on)
) STRICT;
CREATE INDEX idx_gift_vouchers_customer_id ON gift_vouchers (customer_id);
CREATE INDEX idx_gift_vouchers_source ON gift_vouchers (source_document_id);
CREATE TRIGGER gift_vouchers_no_update BEFORE UPDATE ON gift_vouchers
BEGIN
  SELECT RAISE(ABORT, 'a voucher is immutable; its ledger moves');
END;
CREATE TRIGGER gift_vouchers_no_delete BEFORE DELETE ON gift_vouchers
BEGIN
  SELECT RAISE(ABORT, 'a voucher is immutable; its ledger moves');
END;

-- LAN: till(terminal_id); a voucher's balance is the sum over every till.
CREATE TABLE gift_voucher_ledger (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  voucher_id      TEXT NOT NULL REFERENCES gift_vouchers (id),
  kind            TEXT NOT NULL CHECK (kind IN ('issue', 'redeem', 'expire', 'refund')),
  amount          INTEGER NOT NULL CHECK (amount <> 0),
  method          TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                          'cheque', 'transfer', 'return', 'points')),
  reference       TEXT,
  stamp_duty      INTEGER NOT NULL DEFAULT 0 CHECK (stamp_duty >= 0),
  stamp_charged   INTEGER NOT NULL DEFAULT 0 CHECK (stamp_charged BETWEEN 0 AND stamp_duty),
  cash_session_id TEXT REFERENCES cash_sessions (id),
  document_id     TEXT REFERENCES documents (id),
  business_date   TEXT NOT NULL,
  created_at      INTEGER NOT NULL,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  CHECK ((kind = 'issue' AND amount > 0 AND method IS NOT NULL)
      OR (kind IN ('redeem', 'expire', 'refund') AND amount < 0)),
  CHECK (kind <> 'redeem' OR document_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_gift_voucher_ledger_voucher_id ON gift_voucher_ledger (voucher_id);
CREATE INDEX idx_gift_voucher_ledger_cash_session_id ON gift_voucher_ledger (cash_session_id);
CREATE INDEX idx_gift_voucher_ledger_document_id ON gift_voucher_ledger (document_id);
CREATE TRIGGER gift_voucher_ledger_never_negative BEFORE INSERT ON gift_voucher_ledger
WHEN NEW.amount < 0
  AND (SELECT COALESCE(SUM(amount), 0) FROM gift_voucher_ledger WHERE voucher_id = NEW.voucher_id)
      + NEW.amount < 0
BEGIN
  SELECT RAISE(ABORT, 'voucher balance below zero');
END;
CREATE TRIGGER gift_voucher_ledger_no_update BEFORE UPDATE ON gift_voucher_ledger
BEGIN
  SELECT RAISE(ABORT, 'gift_voucher_ledger is append-only');
END;
CREATE TRIGGER gift_voucher_ledger_no_delete BEFORE DELETE ON gift_voucher_ledger
BEGIN
  SELECT RAISE(ABORT, 'gift_voucher_ledger is append-only');
END;

-- LAN: till(terminal_id); a customer's points are the sum over every till.
CREATE TABLE loyalty_ledger (
  id            TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  customer_id   TEXT NOT NULL REFERENCES customers (id),
  kind          TEXT NOT NULL CHECK (kind IN ('earn', 'redeem', 'expire', 'reversal', 'adjust')),
  points        INTEGER NOT NULL CHECK (points <> 0),
  document_id   TEXT REFERENCES documents (id),
  voucher_id    TEXT REFERENCES gift_vouchers (id),
  expires_on    TEXT,
  business_date TEXT NOT NULL,
  note          TEXT,
  created_at    INTEGER NOT NULL,
  user_id       TEXT NOT NULL,
  terminal_id   TEXT NOT NULL,
  CHECK ((kind = 'earn' AND points > 0) OR (kind IN ('redeem', 'expire', 'reversal') AND points < 0)
      OR kind = 'adjust'),
  CHECK (kind <> 'redeem' OR voucher_id IS NOT NULL)
) STRICT;
CREATE INDEX idx_loyalty_ledger_customer_id ON loyalty_ledger (customer_id, created_at);
CREATE INDEX idx_loyalty_ledger_document_id ON loyalty_ledger (document_id);
CREATE INDEX idx_loyalty_ledger_voucher_id ON loyalty_ledger (voucher_id);
CREATE TRIGGER loyalty_ledger_never_negative BEFORE INSERT ON loyalty_ledger
WHEN NEW.points < 0
  AND (SELECT COALESCE(SUM(points), 0) FROM loyalty_ledger WHERE customer_id = NEW.customer_id)
      + NEW.points < 0
BEGIN
  SELECT RAISE(ABORT, 'points below zero');
END;
CREATE TRIGGER loyalty_ledger_no_update BEFORE UPDATE ON loyalty_ledger
BEGIN
  SELECT RAISE(ABORT, 'loyalty_ledger is append-only');
END;
CREATE TRIGGER loyalty_ledger_no_delete BEFORE DELETE ON loyalty_ledger
BEGIN
  SELECT RAISE(ABORT, 'loyalty_ledger is append-only');
END;

-- Jewellery (VRT-140/141/143/144) -----------------------------------------------------------------
-- LAN: shared.
CREATE TABLE jewellery_pieces (
  product_id       TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  metal            TEXT NOT NULL CHECK (metal IN ('gold', 'silver', 'platinum')),
  titre_permille   INTEGER NOT NULL CHECK (titre_permille BETWEEN 1 AND 1000),
  weight_mg        INTEGER NOT NULL CHECK (weight_mg > 0),
  facon            INTEGER NOT NULL DEFAULT 0 CHECK (facon >= 0),
  stones           TEXT CHECK (stones IS NULL OR length(stones) <= 200),
  hallmark         TEXT CHECK (hallmark IS NULL OR length(hallmark) <= 40),
  priced_by_weight INTEGER NOT NULL DEFAULT 1 CHECK (priced_by_weight IN (0, 1)),
  updated_at       INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_jewellery_pieces_metal ON jewellery_pieces (metal, titre_permille);

-- LAN: shared.
CREATE TABLE metal_prices (
  id             TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  metal          TEXT NOT NULL CHECK (metal IN ('gold', 'silver', 'platinum')),
  titre_permille INTEGER NOT NULL CHECK (titre_permille BETWEEN 1 AND 1000),
  price_per_gram INTEGER NOT NULL CHECK (price_per_gram > 0),
  effective_at   INTEGER NOT NULL,
  created_at     INTEGER NOT NULL,
  user_id        TEXT NOT NULL
) STRICT;
CREATE INDEX idx_metal_prices_current ON metal_prices (metal, titre_permille, effective_at);
CREATE TRIGGER metal_prices_no_update BEFORE UPDATE ON metal_prices
BEGIN
  SELECT RAISE(ABORT, 'metal_prices is append-only');
END;
CREATE TRIGGER metal_prices_no_delete BEFORE DELETE ON metal_prices
BEGIN
  SELECT RAISE(ABORT, 'metal_prices is append-only');
END;

-- CII art. 359 as amended by LF 2026 art. 62: the special client register, paged, never edited,
-- kept at least five years (it is never deleted at all).
-- LAN: till(terminal_id), with its sale.
CREATE TABLE jeweller_register (
  id              TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  year            INTEGER NOT NULL,
  seq             INTEGER NOT NULL CHECK (seq >= 1),
  business_date   TEXT NOT NULL,
  document_id     TEXT NOT NULL REFERENCES documents (id),
  line_id         TEXT NOT NULL REFERENCES document_lines (id),
  product_id      TEXT NOT NULL REFERENCES products (id),
  nature          TEXT NOT NULL CHECK (length(nature) BETWEEN 1 AND 160),
  pieces          INTEGER NOT NULL CHECK (pieces > 0),
  weight_mg       INTEGER NOT NULL CHECK (weight_mg > 0),
  titre_permille  INTEGER NOT NULL CHECK (titre_permille BETWEEN 1 AND 1000),
  price           INTEGER NOT NULL CHECK (price >= 0),
  buyer_name      TEXT NOT NULL CHECK (length(buyer_name) BETWEEN 1 AND 120),
  buyer_id_kind   TEXT NOT NULL CHECK (buyer_id_kind IN ('cni', 'passport', 'driving_licence', 'other')),
  buyer_id_number TEXT NOT NULL CHECK (length(buyer_id_number) BETWEEN 1 AND 40),
  buyer_address   TEXT NOT NULL CHECK (length(buyer_address) BETWEEN 1 AND 200),
  created_at      INTEGER NOT NULL,
  user_id         TEXT NOT NULL,
  terminal_id     TEXT NOT NULL,
  UNIQUE (year, seq)
) STRICT;
CREATE INDEX idx_jeweller_register_document_id ON jeweller_register (document_id);
CREATE INDEX idx_jeweller_register_line_id ON jeweller_register (line_id);
CREATE INDEX idx_jeweller_register_product_id ON jeweller_register (product_id);
CREATE TRIGGER jeweller_register_no_update BEFORE UPDATE ON jeweller_register
BEGIN
  SELECT RAISE(ABORT, 'jeweller_register is append-only');
END;
CREATE TRIGGER jeweller_register_no_delete BEFORE DELETE ON jeweller_register
BEGIN
  SELECT RAISE(ABORT, 'jeweller_register is append-only');
END;

-- Employees lite and zakat (EMP-002/003, FIN-007) -------------------------------------------------
-- LAN: shared.
CREATE TABLE staff_profiles (
  user_id        TEXT PRIMARY KEY NOT NULL REFERENCES users (id),
  phone          TEXT CHECK (phone IS NULL OR length(phone) <= 20),
  hire_date      TEXT,
  monthly_salary INTEGER CHECK (monthly_salary IS NULL OR monthly_salary >= 0),
  commission_bp  INTEGER NOT NULL DEFAULT 0 CHECK (commission_bp BETWEEN 0 AND 5000),
  note           TEXT CHECK (note IS NULL OR length(note) <= 200),
  updated_at     INTEGER NOT NULL
) STRICT;

-- LAN: till(terminal_id); what an employee owes is the sum over every till.
CREATE TABLE staff_ledger (
  id               TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  user_id          TEXT NOT NULL REFERENCES users (id),
  kind             TEXT NOT NULL CHECK (kind IN ('advance', 'repayment', 'salary_deduction', 'adjustment')),
  amount           INTEGER NOT NULL CHECK (amount <> 0),
  method           TEXT CHECK (method IS NULL OR method IN ('cash', 'card_cib', 'edahabia', 'baridimob',
                                                           'cheque', 'transfer')),
  expense_id       TEXT REFERENCES expenses (id),
  cash_session_id  TEXT REFERENCES cash_sessions (id),
  cash_movement_id TEXT REFERENCES cash_movements (id),
  month            TEXT CHECK (month IS NULL OR length(month) = 7),
  business_date    TEXT NOT NULL,
  note             TEXT,
  created_at       INTEGER NOT NULL,
  created_by       TEXT NOT NULL,
  terminal_id      TEXT NOT NULL,
  CHECK ((kind = 'advance' AND amount > 0) OR (kind IN ('repayment', 'salary_deduction') AND amount < 0)
      OR kind = 'adjustment'),
  CHECK (kind <> 'salary_deduction' OR month IS NOT NULL)
) STRICT;
CREATE INDEX idx_staff_ledger_user_id ON staff_ledger (user_id, created_at);
CREATE INDEX idx_staff_ledger_expense_id ON staff_ledger (expense_id);
CREATE INDEX idx_staff_ledger_cash_session_id ON staff_ledger (cash_session_id);
CREATE INDEX idx_staff_ledger_cash_movement_id ON staff_ledger (cash_movement_id);
CREATE TRIGGER staff_ledger_never_negative BEFORE INSERT ON staff_ledger
WHEN NEW.amount < 0
  AND (SELECT COALESCE(SUM(amount), 0) FROM staff_ledger WHERE user_id = NEW.user_id) + NEW.amount < 0
BEGIN
  SELECT RAISE(ABORT, 'advance owed below zero');
END;
CREATE TRIGGER staff_ledger_no_update BEFORE UPDATE ON staff_ledger
BEGIN
  SELECT RAISE(ABORT, 'staff_ledger is append-only');
END;
CREATE TRIGGER staff_ledger_no_delete BEFORE DELETE ON staff_ledger
BEGIN
  SELECT RAISE(ABORT, 'staff_ledger is append-only');
END;

-- LAN: host-only, drawn up on the main PC that holds the books.
CREATE TABLE zakat_statements (
  id           TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  number       TEXT NOT NULL UNIQUE,
  day          TEXT NOT NULL,
  hijri_date   TEXT NOT NULL,
  valuation    TEXT NOT NULL CHECK (valuation IN ('cost', 'sale_price')),
  stock_value  INTEGER NOT NULL CHECK (stock_value >= 0),
  cash         INTEGER NOT NULL CHECK (cash >= 0),
  bank         INTEGER NOT NULL CHECK (bank >= 0),
  other_assets INTEGER NOT NULL CHECK (other_assets >= 0),
  receivables  INTEGER NOT NULL CHECK (receivables >= 0),
  payables     INTEGER NOT NULL CHECK (payables >= 0),
  other_debts  INTEGER NOT NULL CHECK (other_debts >= 0),
  base         INTEGER NOT NULL,
  nisab        INTEGER NOT NULL CHECK (nisab > 0),
  rate_bp      INTEGER NOT NULL CHECK (rate_bp BETWEEN 1 AND 10000),
  zakat_due    INTEGER NOT NULL CHECK (zakat_due >= 0),
  details      TEXT NOT NULL CHECK (json_valid(details)),
  note         TEXT,
  created_at   INTEGER NOT NULL,
  user_id      TEXT NOT NULL
) STRICT;
CREATE TRIGGER zakat_statements_no_update BEFORE UPDATE ON zakat_statements
BEGIN
  SELECT RAISE(ABORT, 'zakat_statements is append-only');
END;
CREATE TRIGGER zakat_statements_no_delete BEFORE DELETE ON zakat_statements
BEGIN
  SELECT RAISE(ABORT, 'zakat_statements is append-only');
END;

-- Custom payment types over the base tenders (PAY-006, PAY-007 prélèvement CCP) -------------------
-- LAN: shared.
CREATE TABLE payment_types (
  id                 TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  code               TEXT NOT NULL CHECK (length(code) BETWEEN 1 AND 24),
  name_ar            TEXT NOT NULL DEFAULT '',
  name_fr            TEXT NOT NULL DEFAULT '',
  name_en            TEXT NOT NULL DEFAULT '',
  base_method        TEXT NOT NULL CHECK (base_method IN ('card_cib', 'edahabia', 'baridimob', 'cheque', 'transfer')),
  requires_reference INTEGER NOT NULL DEFAULT 1 CHECK (requires_reference IN (0, 1)),
  sort_order         INTEGER NOT NULL DEFAULT 0,
  archived_at        INTEGER,
  created_at         INTEGER NOT NULL,
  updated_at         INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX idx_payment_types_code ON payment_types (code) WHERE archived_at IS NULL;
CREATE TRIGGER payment_types_no_delete BEFORE DELETE ON payment_types
BEGIN
  SELECT RAISE(ABORT, 'payment types are archived, never deleted');
END;

-- LAN: parent(payment_id -> payments).
CREATE TABLE payment_type_uses (
  payment_id      TEXT PRIMARY KEY NOT NULL REFERENCES payments (id),
  payment_type_id TEXT NOT NULL REFERENCES payment_types (id),
  created_at      INTEGER NOT NULL
) STRICT;
CREATE INDEX idx_payment_type_uses_type ON payment_type_uses (payment_type_id);
CREATE TRIGGER payment_type_uses_no_update BEFORE UPDATE ON payment_type_uses
BEGIN
  SELECT RAISE(ABORT, 'payment_type_uses is append-only');
END;
CREATE TRIGGER payment_type_uses_no_delete BEFORE DELETE ON payment_type_uses
BEGIN
  SELECT RAISE(ABORT, 'payment_type_uses is append-only');
END;

-- Personal data (PDP-002): marketing consent per customer and channel, granted or withdrawn,
-- with its source; the live state is the newest row per (customer, channel). Append-only.
-- LAN: shared, like the customer it belongs to.
CREATE TABLE customer_consents (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  customer_id TEXT NOT NULL REFERENCES customers (id),
  channel     TEXT NOT NULL CHECK (channel IN ('sms', 'whatsapp', 'loyalty')),
  granted     INTEGER NOT NULL CHECK (granted IN (0, 1)),
  source      TEXT NOT NULL CHECK (source IN ('counter', 'form', 'import', 'customer_request')),
  note        TEXT CHECK (note IS NULL OR length(note) <= 200),
  created_at  INTEGER NOT NULL,
  user_id     TEXT NOT NULL
) STRICT;
CREATE INDEX idx_customer_consents_customer ON customer_consents (customer_id, channel, created_at);
CREATE TRIGGER customer_consents_no_update BEFORE UPDATE ON customer_consents
BEGIN
  SELECT RAISE(ABORT, 'customer_consents is append-only');
END;
CREATE TRIGGER customer_consents_no_delete BEFORE DELETE ON customer_consents
BEGIN
  SELECT RAISE(ABORT, 'customer_consents is append-only');
END;

INSERT INTO payment_types (id, code, name_ar, name_fr, name_en, base_method, requires_reference,
                           sort_order, created_at, updated_at)
VALUES ('00000000-0000-7000-8000-000000000601', 'ccp_debit', 'اقتطاع من الحساب البريدي',
        'Prélèvement CCP', 'CCP direct debit', 'transfer', 1, 1, unixepoch() * 1000, unixepoch() * 1000);

-- The Gérant holds the new pharmacy permission (the Propriétaire holds every permission by
-- construction; a fresh shop seeds the Gérant from `manager_permissions()`).
UPDATE roles
SET permissions = json_insert(permissions, '$[#]', 'pharmacy.register'),
    updated_at = unixepoch() * 1000
WHERE id = '00000000-0000-7000-8000-000000000002'
  AND NOT EXISTS (SELECT 1 FROM json_each(roles.permissions) WHERE value = 'pharmacy.register');
