import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { mockCommands, queryWrapper } from "@/test/render";
import { archivePageFixture, draftPreviewFixture } from "@/test/fixtures-m3";
import { useArchive, useDraftPreview, useSaveDraft } from "./billing";
import type { DraftInput } from "./types";

describe("billing hooks", () => {
  it("pages the archive on its cursor", async () => {
    const calls: unknown[] = [];
    mockCommands({
      documents_search: (args) => {
        calls.push(args);
        return calls.length === 1
          ? { ...archivePageFixture, next: { sortAt: 1, id: "b" } }
          : { rows: [], next: null };
      },
    });
    const { result } = renderHook(() => useArchive({ kinds: ["invoice"] }), { wrapper: queryWrapper() });
    await waitFor(() => expect(result.current.data?.pages).toHaveLength(1));
    await act(() => result.current.fetchNextPage());
    expect(calls[1]).toMatchObject({ query: { kinds: ["invoice"], before: { sortAt: 1, id: "b" } } });
  });

  it("asks the backend for every figure of a draft", async () => {
    mockCommands({ documents_preview_draft: () => draftPreviewFixture });
    const input: DraftInput = {
      docType: "quote",
      cart: { lines: [], priceLevelId: null, customerId: null, lang: "fr", note: null },
    };
    const { result } = renderHook(() => useDraftPreview(input), { wrapper: queryWrapper() });
    await waitFor(() => expect(result.current.data?.priced.cart.totals.totalTtc).toBe(119_000));
  });

  it("invalidates the archive after saving a draft", async () => {
    const calls = mockCommands({
      documents_save_draft: () => ({ id: "d1" }),
      documents_search: () => archivePageFixture,
    });
    const { result } = renderHook(() => ({ save: useSaveDraft(), archive: useArchive({}) }), {
      wrapper: queryWrapper(),
    });
    await waitFor(() => expect(result.current.archive.isSuccess).toBe(true));
    await act(() =>
      result.current.save.mutateAsync({
        id: null,
        input: {
          docType: "quote",
          cart: { lines: [], priceLevelId: null, customerId: null, lang: "fr", note: null },
        },
      }),
    );
    await waitFor(() => expect(calls.filter((c) => c === "documents_search")).toHaveLength(2));
  });
});
