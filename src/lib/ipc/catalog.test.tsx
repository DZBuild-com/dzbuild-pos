import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { useCategories, useCreateCategory } from "@/lib/ipc/catalog";
import { useFindByCode, useProductGrid, useProductSearch } from "@/lib/ipc/products";
import { useAdjustStock } from "@/lib/ipc/stock";
import { createQueryClient } from "@/lib/query";
import { categoriesFixture, productHitsFixture, scanHitFixture } from "@/test/fixtures-m1";
import { mockCommands } from "@/test/render";

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe("catalogue hooks", () => {
  it("searches and lists the grid with typed payloads", async () => {
    const received: unknown[] = [];
    mockCommands({
      products_search: (args) => {
        received.push(args);
        return productHitsFixture;
      },
      products_grid: (args) => {
        received.push(args);
        return productHitsFixture;
      },
    });
    const client = createQueryClient();
    const search = renderHook(
      () => useProductSearch({ text: "lait", limit: 8, categoryId: null, includeArchived: false }),
      {
        wrapper: wrapper(client),
      },
    );
    const grid = renderHook(() => useProductGrid("c-dairy"), { wrapper: wrapper(client) });
    await waitFor(() => expect(search.result.current.data).toEqual(productHitsFixture));
    await waitFor(() => expect(grid.result.current.data).toEqual(productHitsFixture));
    expect(received).toContainEqual({
      query: { text: "lait", limit: 8, categoryId: null, includeArchived: false },
    });
    expect(received).toContainEqual({ categoryId: "c-dairy", limit: 200 });
  });

  it("keeps the till grid and the back office list apart (different limits)", async () => {
    const limits: unknown[] = [];
    mockCommands({
      products_grid: ({ limit }) => {
        limits.push(limit);
        return productHitsFixture;
      },
    });
    const client = createQueryClient();
    const till = renderHook(() => useProductGrid(null), { wrapper: wrapper(client) });
    await waitFor(() => expect(till.result.current.isSuccess).toBe(true));
    const backOffice = renderHook(() => useProductGrid(null, 500), { wrapper: wrapper(client) });
    await waitFor(() => expect(backOffice.result.current.isSuccess).toBe(true));
    expect(limits).toEqual([200, 500]);
  });

  it("looks a scanned code up every time, without a cache", async () => {
    let calls = 0;
    mockCommands({
      products_find_by_code: ({ code }) => {
        calls += 1;
        return code === "6130000000017" ? scanHitFixture : null;
      },
    });
    const client = createQueryClient();
    const { result } = renderHook(() => useFindByCode(), { wrapper: wrapper(client) });
    await expect(result.current.mutateAsync("6130000000017")).resolves.toEqual(scanHitFixture);
    await expect(result.current.mutateAsync("6130000000017")).resolves.toEqual(scanHitFixture);
    await expect(result.current.mutateAsync("0000")).resolves.toBeNull();
    expect(calls).toBe(3);
  });

  it("refreshes the categories after a creation", async () => {
    let listed = 0;
    mockCommands({
      catalog_categories_list: () => {
        listed += 1;
        return categoriesFixture;
      },
      catalog_categories_create: ({ input }) => ({
        ...categoriesFixture[0],
        ...(input as object),
        id: "c-new",
      }),
    });
    const client = createQueryClient();
    const list = renderHook(() => useCategories(), { wrapper: wrapper(client) });
    await waitFor(() => expect(list.result.current.data).toEqual(categoriesFixture));
    const create = renderHook(() => useCreateCategory(), { wrapper: wrapper(client) });
    await create.result.current.mutateAsync({
      parentId: null,
      nameAr: "مشروبات",
      nameFr: "Boissons",
      nameEn: "Drinks",
      color: null,
      sortOrder: 2,
      image: null,
    });
    await waitFor(() => expect(listed).toBe(2));
  });

  it("invalidates stock and products after an adjustment", async () => {
    mockCommands({
      stock_adjust: ({ input }) => ({ id: "m-1", ...(input as object) }),
    });
    const client = createQueryClient();
    client.setQueryData(["stock", "levels", "p-milk"], []);
    client.setQueryData(["products", "grid", null], []);
    const { result } = renderHook(() => useAdjustStock(), { wrapper: wrapper(client) });
    await result.current.mutateAsync({
      productId: "p-milk",
      locationId: null,
      qtyDelta: 12_000,
      reason: "opening",
      unitCost: 9_000,
      note: null,
    });
    expect(client.getQueryState(["stock", "levels", "p-milk"])?.isInvalidated).toBe(true);
    expect(client.getQueryState(["products", "grid", null])?.isInvalidated).toBe(true);
  });
});
