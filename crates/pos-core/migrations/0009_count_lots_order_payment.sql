-- M8 follow-up. Forward-only; never edit once committed.
-- Quantities INTEGER milli-units. LAN intent as in 0008: `shared` is host data every till mirrors.

-- Per-lot count lines (STK-004) --------------------------------------------------------------
-- A lot-tracked product counted lot by lot: one row per lot found on the shelf (or, at the
-- close, per lot in the books nobody found, counted 0), under the product's line of
-- `stock_count_lines`, whose counted quantity is the sum of its lot lines. `expected_qty` is the
-- lot's level at the count's location when the lot was counted; the close posts
-- `counted_qty - expected_qty` in that lot and keeps it in `posted_delta`. Like the product
-- lines, the lot lines change while their count is open only.
-- LAN: shared, like the stock_counts row it belongs to.
CREATE TABLE stock_count_lot_lines (
  count_id     TEXT NOT NULL REFERENCES stock_counts (id),
  product_id   TEXT NOT NULL REFERENCES products (id),
  lot_id       TEXT NOT NULL REFERENCES lots (id),
  expected_qty INTEGER NOT NULL,
  counted_qty  INTEGER NOT NULL CHECK (counted_qty >= 0),
  posted_delta INTEGER,
  counted_at   INTEGER NOT NULL,
  counted_by   TEXT,
  PRIMARY KEY (count_id, lot_id),
  FOREIGN KEY (count_id, product_id) REFERENCES stock_count_lines (count_id, product_id)
) STRICT;
CREATE INDEX idx_stock_count_lot_lines_line ON stock_count_lot_lines (count_id, product_id);
CREATE INDEX idx_stock_count_lot_lines_product_id ON stock_count_lot_lines (product_id);
CREATE INDEX idx_stock_count_lot_lines_lot_id ON stock_count_lot_lines (lot_id);

CREATE TRIGGER stock_count_lot_lines_open_insert BEFORE INSERT ON stock_count_lot_lines
WHEN (SELECT status FROM stock_counts WHERE id = NEW.count_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'lines are added to an open count only');
END;

CREATE TRIGGER stock_count_lot_lines_lot_of_product BEFORE INSERT ON stock_count_lot_lines
WHEN (SELECT product_id FROM lots WHERE id = NEW.lot_id) IS NOT NEW.product_id
BEGIN
  SELECT RAISE(ABORT, 'a lot line counts a lot of its own product');
END;

CREATE TRIGGER stock_count_lot_lines_open_update BEFORE UPDATE ON stock_count_lot_lines
WHEN (SELECT status FROM stock_counts WHERE id = OLD.count_id) IS NOT 'open'
  OR NEW.count_id IS NOT OLD.count_id OR NEW.product_id IS NOT OLD.product_id
  OR NEW.lot_id IS NOT OLD.lot_id
BEGIN
  SELECT RAISE(ABORT, 'the lines of a closed count are final');
END;

CREATE TRIGGER stock_count_lot_lines_open_delete BEFORE DELETE ON stock_count_lot_lines
WHEN (SELECT status FROM stock_counts WHERE id = OLD.count_id) IS NOT 'open'
BEGIN
  SELECT RAISE(ABORT, 'the lines of a closed count are final');
END;

-- Online orders: DZBuild's payment status --------------------------------------------
-- The `payment_status` of DZBuild's order payload (research 06 §3.1.6: e.g. `pending`, `paid`),
-- as DZBuild sends it; NULL for the orders mirrored before 0009 or a payload without it.
ALTER TABLE online_orders ADD COLUMN payment_status TEXT
  CHECK (payment_status IS NULL OR length(payment_status) BETWEEN 1 AND 40);
