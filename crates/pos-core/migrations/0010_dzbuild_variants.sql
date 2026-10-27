-- 0010 DZBuild variant families (wire contract v1.12). Forward-only; never edit once committed.
-- Money INTEGER centimes.

-- The online twin of each variant child ------------------------------------------------------
-- One row per variant child DZBuild knows: the child's variant key (`cloud::variants::variant_key`)
-- under its parent's remote product id. The parent keeps its own `cloud_map` row (entity
-- 'product'); DZBuild has no product per child. `price_online` is what the store charges for the
-- child when DZBuild answered one (NULL until then). A child whose key is gone online keeps its
-- product row and loses this one.
-- LAN: local-only, like `cloud_map`: the link runs on the main PC; a till's copy has none.
CREATE TABLE cloud_variant_map (
  store_id          TEXT NOT NULL,
  local_id          TEXT NOT NULL REFERENCES products (id),
  parent_id         TEXT NOT NULL REFERENCES products (id),
  remote_product_id TEXT NOT NULL,
  variant_key       TEXT NOT NULL,
  price_online      INTEGER,
  updated_at        INTEGER NOT NULL,
  PRIMARY KEY (store_id, local_id)
) STRICT, WITHOUT ROWID;
CREATE INDEX idx_cloud_variant_map_remote ON cloud_variant_map (store_id, remote_product_id, variant_key);
CREATE INDEX idx_cloud_variant_map_local_id ON cloud_variant_map (local_id);
CREATE INDEX idx_cloud_variant_map_parent_id ON cloud_variant_map (parent_id);
