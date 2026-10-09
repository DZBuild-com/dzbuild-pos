-- M4 Hardware: per-product scale settings (PRD-074: hotkey, tare, shelf life; the PLU stays
-- products.plu) and the last label printed per product, the reference of the LBL-007 reprint
-- queue. The print history itself is the audit journal (labels.printed). Forward-only: this
-- file is immutable once committed.

CREATE TABLE scale_items (
  product_id      TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  hotkey          INTEGER CHECK (hotkey IS NULL OR hotkey BETWEEN 1 AND 999),
  tare_g          INTEGER NOT NULL DEFAULT 0 CHECK (tare_g BETWEEN 0 AND 99999),
  shelf_life_days INTEGER CHECK (shelf_life_days IS NULL OR shelf_life_days BETWEEN 0 AND 3650),
  updated_at      INTEGER NOT NULL
) STRICT;
CREATE UNIQUE INDEX idx_scale_items_hotkey ON scale_items (hotkey) WHERE hotkey IS NOT NULL;

CREATE TABLE label_prints (
  product_id  TEXT PRIMARY KEY NOT NULL REFERENCES products (id),
  printed_at  INTEGER NOT NULL,
  user_id     TEXT REFERENCES users (id),
  template_id TEXT NOT NULL CHECK (length(template_id) BETWEEN 1 AND 64),
  lang        TEXT NOT NULL CHECK (lang IN ('ar', 'fr', 'en', 'ar_fr', 'ar_en', 'fr_en')),
  price       INTEGER NOT NULL CHECK (price >= 0),
  old_price   INTEGER CHECK (old_price IS NULL OR old_price >= 0),
  name        TEXT NOT NULL,
  barcode     TEXT,
  copies      INTEGER NOT NULL CHECK (copies >= 0)
) STRICT;
CREATE INDEX idx_label_prints_user_id ON label_prints (user_id);
CREATE INDEX idx_label_prints_printed_at ON label_prints (printed_at);
