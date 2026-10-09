import { act, renderHook } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { mockCommands, queryWrapper } from "@/test/render";
import { useExportDocumentPdf, usePrintDocument } from "./print";

describe("print hooks", () => {
  it("prints with the options and reports a duplicate", async () => {
    const seen: unknown[] = [];
    mockCommands({
      documents_print: (args) => {
        seen.push(args);
        return { printer: "HP", copies: 1, duplicate: true, dialog: false };
      },
    });
    const { result } = renderHook(() => usePrintDocument(), { wrapper: queryWrapper() });
    const out = await act(() =>
      result.current.mutateAsync({
        id: "d1",
        options: { lang: "ar", secondLang: null, monolingual: true },
        printer: null,
        copies: null,
      }),
    );
    expect(out.duplicate).toBe(true);
    expect(seen[0]).toMatchObject({ input: { id: "d1", options: { lang: "ar", monolingual: true } } });
  });

  it("returns null when the save dialog is cancelled", async () => {
    mockCommands({ documents_export_pdf: () => null });
    const { result } = renderHook(() => useExportDocumentPdf(), { wrapper: queryWrapper() });
    expect(
      await act(() =>
        result.current.mutateAsync({
          id: "d1",
          options: { lang: null, secondLang: null, monolingual: false },
        }),
      ),
    ).toBeNull();
  });
});
