import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useBuildLabels, useSaveLabelTemplates } from "@/lib/ipc/labels";
import { createQueryClient } from "@/lib/query";
import { useSetting } from "@/lib/ipc/settings";
import type { LabelRequest } from "@/lib/ipc/types";
import { labelBatchFixture, labelTemplatesFixture } from "@/test/fixtures-m4";
import { mockCommands, providers } from "@/test/render";

const request: LabelRequest = {
  source: { kind: "products", items: [{ productId: "p-milk", copies: 1, lotId: null }] },
  templateId: null,
  lang: null,
};

describe("saving the label templates", () => {
  it("rebuilds the batches and rereads the template settings made from the old ones", async () => {
    const asked: string[] = [];
    mockCommands({
      labels_build: () => {
        asked.push("labels_build");
        return labelBatchFixture;
      },
      settings_get: (args) => {
        asked.push(`settings_get ${String(args.key)}`);
        return null;
      },
      labels_save_templates: () => labelTemplatesFixture,
    });
    const client = createQueryClient();
    const wrapper = providers(client);
    const batch = renderHook(() => useBuildLabels(request), { wrapper });
    const templates = renderHook(() => useSetting("labels.templates"), { wrapper });
    const byCategory = renderHook(() => useSetting("labels.category_templates"), { wrapper });
    await waitFor(() => expect(batch.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(templates.result.current.isSuccess).toBe(true));
    await waitFor(() => expect(byCategory.result.current.isSuccess).toBe(true));
    asked.length = 0;
    const save = renderHook(() => useSaveLabelTemplates(), { wrapper });
    await save.result.current.mutateAsync({
      templates: labelTemplatesFixture.templates,
      categoryTemplates: {},
    });
    await waitFor(() =>
      expect([...asked].sort()).toEqual([
        "labels_build",
        "settings_get labels.category_templates",
        "settings_get labels.templates",
      ]),
    );
  });
});
