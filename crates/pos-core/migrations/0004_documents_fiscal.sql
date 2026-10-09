-- M3 Documents and fiscal (spec §5.1 documents, §5.2, §5.3, §6, §9.2; mvp-backlog DOC, TAX,
-- RPT §37.7, AUD-010). Forward-only. Every new column is NULL (or 0) on the rows of M0-M2, and
-- the document hash takes the M3 fields only when one is present, so every existing chain
-- verifies unchanged.

-- 1. Documents: how long an offer holds, when an invoice is settled, the second print language,
--    the kind of a credit note, and where an invoice came from (an invoice of a ticket carries
--    no turnover of its own, an invoice of delivery notes moves no stock).
ALTER TABLE documents ADD COLUMN valid_until TEXT
  CHECK (valid_until IS NULL OR valid_until GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');
ALTER TABLE documents ADD COLUMN due_date TEXT
  CHECK (due_date IS NULL OR due_date GLOB '[0-9][0-9][0-9][0-9]-[0-9][0-9]-[0-9][0-9]');
ALTER TABLE documents ADD COLUMN second_lang TEXT
  CHECK (second_lang IS NULL OR second_lang IN ('ar', 'fr', 'en'));
ALTER TABLE documents ADD COLUMN credit_kind TEXT
  CHECK (credit_kind IS NULL OR credit_kind IN ('goods', 'financial'));
ALTER TABLE documents ADD COLUMN origin TEXT
  CHECK (origin IS NULL OR origin IN ('direct', 'quote', 'proforma', 'delivery', 'ticket'));

CREATE TRIGGER documents_m3_fields_fit_the_kind
BEFORE UPDATE OF valid_until, due_date, credit_kind ON documents
WHEN (NEW.valid_until IS NOT NULL AND NEW.doc_type NOT IN ('quote', 'proforma'))
  OR (NEW.due_date IS NOT NULL AND NEW.doc_type <> 'invoice')
  OR (NEW.credit_kind IS NOT NULL AND NEW.doc_type <> 'credit_note')
BEGIN
  SELECT RAISE(ABORT, 'this field does not belong to this kind of document');
END;

-- The archive search (kind and period, customer and period, amount, newest first).
CREATE INDEX idx_documents_type_day ON documents (doc_type, business_date);
CREATE INDEX idx_documents_customer_day ON documents (customer_id, business_date);
CREATE INDEX idx_documents_total_due ON documents (total_due);
CREATE INDEX idx_documents_recent ON documents (COALESCE(validated_at, created_at), id);

-- 2. The line name in the second language of a bilingual document (UX-042), taken when the
--    document is validated.
ALTER TABLE document_lines ADD COLUMN description_alt TEXT;

-- 3. Seller and buyer as they were at validation (décret 05-468 art. 3): what every reprint
--    shows, whatever the company or the customer file says later. Hashed with the document.
CREATE TABLE document_parties (
  document_id    TEXT NOT NULL REFERENCES documents (id),
  role           TEXT NOT NULL CHECK (role IN ('seller', 'buyer')),
  party_id       TEXT,
  name           TEXT NOT NULL CHECK (length(trim(name)) > 0),
  name_alt       TEXT,
  legal_form     TEXT,
  activity       TEXT,
  activity_alt   TEXT,
  capital        INTEGER CHECK (capital IS NULL OR capital >= 0),
  address        TEXT,
  wilaya         TEXT,
  phone          TEXT,
  email          TEXT,
  rc             TEXT,
  ae_card        TEXT,
  nif            TEXT,
  nis            TEXT,
  ai             TEXT,
  nin            TEXT,
  rib            TEXT,
  vat_exempt_ref TEXT,
  PRIMARY KEY (document_id, role)
) STRICT;
CREATE INDEX idx_document_parties_party_id ON document_parties (party_id);

CREATE TRIGGER document_parties_draft_insert BEFORE INSERT ON document_parties
WHEN (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'parties are recorded on draft documents only');
END;

-- Like the lines of 0002: neither the row's document nor the one it would move to is final.
CREATE TRIGGER document_parties_draft_update BEFORE UPDATE ON document_parties
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
  OR (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'parties of a validated document are immutable');
END;

CREATE TRIGGER document_parties_draft_delete BEFORE DELETE ON document_parties
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'parties of a validated document are never deleted');
END;

-- 4. Which document was made from which: quote → invoice, BL → invoice (DOC-031), ticket →
--    invoice (SAL-054), invoice → credit note, quote → ticket. Append-only; a BL and a ticket
--    are invoiced once, even by two tills at the same instant.
CREATE TABLE document_links (
  id         TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  from_id    TEXT NOT NULL REFERENCES documents (id),
  to_id      TEXT NOT NULL REFERENCES documents (id),
  kind       TEXT NOT NULL CHECK (kind IN ('converted', 'delivered', 'invoiced_ticket', 'credited',
                                            'sold_from_quote')),
  created_at INTEGER NOT NULL,
  CHECK (from_id <> to_id),
  UNIQUE (from_id, to_id)
) STRICT;
CREATE INDEX idx_document_links_to_id ON document_links (to_id);
CREATE UNIQUE INDEX idx_document_links_invoiced_once ON document_links (from_id)
  WHERE kind IN ('delivered', 'invoiced_ticket');

CREATE TRIGGER document_links_no_update BEFORE UPDATE ON document_links
BEGIN
  SELECT RAISE(ABORT, 'document_links is append-only');
END;

CREATE TRIGGER document_links_no_delete BEFORE DELETE ON document_links
BEGIN
  SELECT RAISE(ABORT, 'document_links is append-only');
END;

-- 5. The cart a draft was typed from (the editor reopens it exactly), frozen with the document.
CREATE TABLE document_drafts (
  document_id TEXT PRIMARY KEY NOT NULL REFERENCES documents (id),
  cart        TEXT NOT NULL CHECK (json_valid(cart)),
  updated_at  INTEGER NOT NULL
) STRICT;

CREATE TRIGGER document_drafts_draft_insert BEFORE INSERT ON document_drafts
WHEN (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'a cart is kept for draft documents only');
END;

CREATE TRIGGER document_drafts_frozen BEFORE UPDATE ON document_drafts
WHEN (SELECT status FROM documents WHERE id = OLD.document_id) IS NOT 'draft'
  OR (SELECT status FROM documents WHERE id = NEW.document_id) IS NOT 'draft'
BEGIN
  SELECT RAISE(ABORT, 'the cart of a validated document is kept as it was');
END;

CREATE TRIGGER document_drafts_no_delete BEFORE DELETE ON document_drafts
BEGIN
  SELECT RAISE(ABORT, 'document_drafts rows are never deleted');
END;

-- 6. B2B buyers (décret 05-468 art. 3-2°: legal form and activity; TAX-004 VAT exemption with its
--    attestation) and product warranties (DOC-012, décret 13-327).
ALTER TABLE customers ADD COLUMN legal_form TEXT;
ALTER TABLE customers ADD COLUMN activity TEXT;
ALTER TABLE customers ADD COLUMN vat_exempt INTEGER NOT NULL DEFAULT 0 CHECK (vat_exempt IN (0, 1));
ALTER TABLE customers ADD COLUMN vat_exempt_ref TEXT;
ALTER TABLE product_extras ADD COLUMN warranty_months INTEGER
  CHECK (warranty_months IS NULL OR warranty_months BETWEEN 1 AND 120);

-- 7. Numbers a counter jumped over (M1 review; spec §5.2, §13). A restored database may be
--    behind the one it replaces: at its first start its counters (documents, Z, codes) move
--    past the numbers that database had printed (`backup.restored` journals its high-water
--    marks), so no number is ever handed out twice. Each jump is one row here, listed by the
--    `backup.restored` row of its restore (`restore_id` is that row's entity id): the only gap
--    numbering verification accepts. Append-only.
CREATE TABLE counter_skips (
  id          TEXT PRIMARY KEY NOT NULL CHECK (length(id) = 36),
  doc_type    TEXT NOT NULL,
  series      TEXT NOT NULL,
  fiscal_year INTEGER NOT NULL,
  from_number INTEGER NOT NULL CHECK (from_number >= 1),
  to_number   INTEGER NOT NULL CHECK (to_number >= from_number),
  restore_id  TEXT NOT NULL CHECK (length(trim(restore_id)) > 0),
  created_at  INTEGER NOT NULL,
  UNIQUE (doc_type, series, fiscal_year, from_number)
) STRICT;
CREATE INDEX idx_counter_skips_restore_id ON counter_skips (restore_id);

CREATE TRIGGER counter_skips_no_update BEFORE UPDATE ON counter_skips
BEGIN
  SELECT RAISE(ABORT, 'counter_skips is append-only');
END;

CREATE TRIGGER counter_skips_no_delete BEFORE DELETE ON counter_skips
BEGIN
  SELECT RAISE(ABORT, 'counter_skips is append-only');
END;

-- The counter guards of 0002, widened by exactly one case each: a jump that a skip of the same
-- key records, from the counter's next number up to its new last one.
DROP TRIGGER counters_start_at_one;
CREATE TRIGGER counters_start_at_one BEFORE INSERT ON counters
WHEN NEW.last_number <> 1
  AND NOT EXISTS (SELECT 1 FROM counter_skips s
                  WHERE s.doc_type = NEW.doc_type AND s.series = NEW.series
                    AND s.fiscal_year = NEW.fiscal_year AND s.from_number = 1
                    AND s.to_number = NEW.last_number)
BEGIN
  SELECT RAISE(ABORT, 'a counter starts at 1');
END;

DROP TRIGGER counters_step_by_one;
CREATE TRIGGER counters_step_by_one BEFORE UPDATE ON counters
WHEN NEW.doc_type <> OLD.doc_type
  OR NEW.series <> OLD.series
  OR NEW.fiscal_year <> OLD.fiscal_year
  OR (NEW.last_number <> OLD.last_number + 1
      AND NOT EXISTS (SELECT 1 FROM counter_skips s
                      WHERE s.doc_type = OLD.doc_type AND s.series = OLD.series
                        AND s.fiscal_year = OLD.fiscal_year
                        AND s.from_number = OLD.last_number + 1
                        AND s.to_number = NEW.last_number))
BEGIN
  SELECT RAISE(ABORT, 'a counter only moves up by one');
END;
