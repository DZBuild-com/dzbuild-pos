import {
  keepPreviousData,
  useInfiniteQuery,
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";
import { commands } from "./client";
import { qk } from "./keys";
import type { ArchiveFilter, CatalogCursor, Money, ProductHit, ProductInput, SearchQuery } from "./types";

export function useProduct(id: string | null) {
  return useQuery({
    queryKey: qk.product(id ?? ""),
    queryFn: () => commands.productsGet(id ?? ""),
    enabled: id !== null,
  });
}

export function useProductSearch(query: SearchQuery, enabled = true) {
  return useQuery({
    queryKey: qk.productSearch(query),
    queryFn: () => commands.productsSearch(query),
    enabled: enabled && query.text.trim() !== "",
    placeholderData: keepPreviousData,
    staleTime: 5_000,
  });
}

export function useProductGrid(categoryId: string | null, limit = 200) {
  return useQuery({
    queryKey: qk.productGrid(categoryId, limit),
    queryFn: () => commands.productsGrid(categoryId, limit),
    placeholderData: keepPreviousData,
    staleTime: 60_000,
  });
}

const CATALOG_PAGE = 100;

function nextCatalogPage(last: ProductHit[]): CatalogCursor | null {
  const row = last.length === CATALOG_PAGE ? last[last.length - 1] : undefined;
  return row ? { nameFr: row.nameFr, nameAr: row.nameAr, nameEn: row.nameEn, id: row.id } : null;
}

/** The back-office product list: every product (at the till or not, archived on demand), by name,
 * 100 at a time. */
export function useProductCatalog(filter: { categoryId: string | null; archived: ArchiveFilter }) {
  return useInfiniteQuery({
    queryKey: qk.productCatalog(filter),
    queryFn: ({ pageParam }) =>
      commands.productsCatalog({ ...filter, after: pageParam, limit: CATALOG_PAGE }),
    initialPageParam: null as CatalogCursor | null,
    getNextPageParam: nextCatalogPage,
    placeholderData: keepPreviousData,
  });
}

export function useVariants(parentId: string | null) {
  return useQuery({
    queryKey: qk.variants(parentId ?? ""),
    queryFn: () => commands.productsVariants(parentId ?? ""),
    enabled: parentId !== null,
  });
}

/** Scanned or typed codes are looked up every time: a code must see the latest barcode. */
export function useFindByCode() {
  return useMutation({ mutationFn: (code: string) => commands.productsFindByCode(code) });
}

function useRefreshProducts() {
  const client = useQueryClient();
  return () => client.invalidateQueries({ queryKey: ["products"] });
}

export function useCreateProduct() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: (input: ProductInput) => commands.productsCreate(input),
    onSuccess: refresh,
  });
}

export function useUpdateProduct() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: ({ id, input }: { id: string; input: ProductInput }) => commands.productsUpdate(id, input),
    onSuccess: refresh,
  });
}

export function useArchiveProduct() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: (id: string) => commands.productsArchive(id), onSuccess: refresh });
}

export function useRestoreProduct() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: (id: string) => commands.productsRestore(id), onSuccess: refresh });
}

export function useDuplicateProduct() {
  const refresh = useRefreshProducts();
  return useMutation({ mutationFn: (id: string) => commands.productsDuplicate(id), onSuccess: refresh });
}

export function useSetProductPrice() {
  const refresh = useRefreshProducts();
  return useMutation({
    mutationFn: ({ id, priceLevelId, price }: { id: string; priceLevelId: string; price: Money }) =>
      commands.productsSetPrice(id, priceLevelId, price),
    onSuccess: refresh,
  });
}
