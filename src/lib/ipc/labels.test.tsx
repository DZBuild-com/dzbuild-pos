import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useLabelPreview, useLabelTemplates, usePrintLabels, useStaleLabels } from "@/lib/ipc/labels";
import { labelBatchFixture, labelTemplatesFixture, staleLabelsFixture } from "@/test/fixtures-m4";
import { mockCommands, providers } from "@/test/render";

describe("label hooks", () => {
  it("reads the templates and the reprint queue", async () => {
    mockCommands({ labels_templates: () => labelTemplatesFixture, labels_stale: () => staleLabelsFixture });
    const templates = renderHook(() => useLabelTemplates(), { wrapper: providers() });
    await waitFor(() => expect(templates.result.current.data?.templates[0]?.id).toBe("builtin.price_40x25"));
    const stale = renderHook(() => useStaleLabels(), { wrapper: providers() });
    await waitFor(() => expect(stale.result.current.data?.[0]?.reasons).toEqual(["price"]));
  });

  it("sends the Arabic override only when asked", async () => {
    const sent: unknown[] = [];
    mockCommands({
      labels_print: (args) => {
        sent.push(args.input);
        return {
          outcome: { printer: "Étiquettes", bytes: 100, raster: true },
          labels: 2,
          assigned: [],
          warnings: [],
        };
      },
    });
    const { result } = renderHook(() => usePrintLabels(), { wrapper: providers() });
    await result.current.mutateAsync({
      request: {
        source: { kind: "products", items: [{ productId: "p-milk", copies: 2, lotId: null }] },
        templateId: null,
        lang: null,
      },
      printerId: null,
      assignMissingBarcodes: false,
      arabicOverride: false,
    });
    expect(sent).toEqual([expect.objectContaining({ arabicOverride: false })]);
  });

  it("previews the last draft once it settles, then stays quiet", async () => {
    const label = labelBatchFixture.groups[0]!.labels[0]!;
    const sent: unknown[] = [];
    mockCommands({
      labels_preview: (args) => {
        sent.push(args.input);
        return { png: "data:image/png;base64,", label, dpi: "d203", warnings: [] };
      },
    });
    const template = labelTemplatesFixture.templates[0]!;
    let renders = 0;
    // a new object on every render, as a designer's draft is
    const { result, rerender } = renderHook(
      ({ widthDmm }: { widthDmm: number }) => {
        renders += 1;
        return useLabelPreview({ template: { ...template, widthDmm }, productId: "p-milk", lang: null });
      },
      { wrapper: providers(), initialProps: { widthDmm: 400 } },
    );
    rerender({ widthDmm: 500 });
    rerender({ widthDmm: 580 });
    await waitFor(() => expect(sent).toHaveLength(2));
    await waitFor(() => expect(result.current.data?.dpi).toBe("d203"));
    // the first draft at once, then only the last change
    expect(sent.map((input) => (input as { template: { widthDmm: number } }).template.widthDmm)).toEqual([
      400, 580,
    ]);
    const settled = renders;
    await new Promise((resolve) => setTimeout(resolve, 700));
    expect(renders).toBe(settled);
  });
});
