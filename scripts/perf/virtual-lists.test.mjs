import assert from "node:assert/strict";
import { existsSync, readFileSync } from "node:fs";
import { test } from "node:test";

// NFR-010: lists that can hold thousands of rows render only what is visible.
const LISTS = [
  "src/features/pos/ProductGrid.tsx",
  "src/features/products/ProductsScreen.tsx",
  "src/features/documents/SalesHistory.tsx",
  "src/components/data-table/data-table.tsx",
];

test("every long list is virtualised", () => {
  for (const file of LISTS) {
    if (!existsSync(file)) continue; // a milestone not merged yet
    assert.match(readFileSync(file, "utf8"), /@tanstack\/react-virtual/, `${file} renders every row`);
  }
});
