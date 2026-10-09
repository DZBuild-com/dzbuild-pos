import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import {
  useAnonymise,
  useConsents,
  useExportCustomerData,
  useInstallPack,
  useInstalledPacks,
  useSetConsent,
  useSetTillKeyMap,
  useTillKeyMap,
} from "@/lib/ipc/privacy";
import type { Consent } from "@/lib/ipc/types";
import { mockCommands, providers } from "@/test/render";

const consent: Consent = {
  channel: "sms",
  granted: true,
  source: "counter",
  note: null,
  createdAt: Date.UTC(2026, 8, 12, 9),
  userName: "Sofiane",
};

// Task 21b: the personal-data, key-map and data-pack hooks call the generated commands (no
// stand-in names), and the webview never names a file or a folder.
describe("the privacy, key map and data pack hooks", () => {
  it("reads a customer's consents and records an answer through the generated commands", async () => {
    const seen: Array<Record<string, unknown>> = [];
    const calls = mockCommands({
      privacy_consents: (args) => {
        seen.push(args);
        return [consent];
      },
      privacy_set_consent: (args) => {
        seen.push(args);
        return { ...consent, granted: false };
      },
    });
    const idle = renderHook(() => useConsents(null), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const { result } = renderHook(() => ({ list: useConsents("c1"), set: useSetConsent() }), {
      wrapper: providers(),
    });
    await waitFor(() => expect(result.current.list.data).toEqual([consent]));
    await act(() =>
      result.current.set.mutateAsync({
        customerId: "c1",
        channel: "sms",
        granted: false,
        source: "customer_request",
        note: null,
      }),
    );
    expect(calls).toContain("privacy_set_consent");
    expect(seen).toContainEqual({ customerId: "c1" });
    expect(seen).toContainEqual({
      customerId: "c1",
      channel: "sms",
      granted: false,
      source: "customer_request",
      note: null,
    });
  });

  it("exports and anonymises by the customer's id only", async () => {
    const seen: Array<[string, Record<string, unknown>]> = [];
    mockCommands({
      privacy_export_customer: (args) => {
        seen.push(["export", args]);
        return { jsonPath: "C:/x/C0001-donnees.json", xlsxPath: "C:/x/C0001-documents.xlsx" };
      },
      privacy_anonymise: (args) => {
        seen.push(["anonymise", args]);
        return { customerId: "c1", code: "C0001", keptDocuments: 2, keptRegisterEntries: 0 };
      },
    });
    const { result } = renderHook(() => ({ out: useExportCustomerData(), erase: useAnonymise() }), {
      wrapper: providers(),
    });
    const files = await act(() => result.current.out.mutateAsync("c1"));
    expect(files?.jsonPath).toMatch(/donnees\.json$/);
    await act(() => result.current.erase.mutateAsync({ customerId: "c1", note: "demande écrite" }));
    expect(seen).toEqual([
      ["export", { customerId: "c1" }],
      ["anonymise", { customerId: "c1", note: "demande écrite" }],
    ]);
  });

  it("reads and saves the till's keys, and installs a data pack without naming a file", async () => {
    const rows: Array<[string, string]> = [["drawer", "Ctrl+9"]];
    const seen: Array<[string, Record<string, unknown>]> = [];
    const calls = mockCommands({
      till_key_map: () => rows,
      till_set_key_map: (args) => {
        seen.push(["keys", args]);
        return rows;
      },
      datapack_installed: () => [],
      datapack_install: (args) => {
        seen.push(["install", args]);
        return null;
      },
    });
    const { result } = renderHook(
      () => ({
        keys: useTillKeyMap(),
        setKeys: useSetTillKeyMap(),
        packs: useInstalledPacks(),
        install: useInstallPack(),
      }),
      { wrapper: providers() },
    );
    await waitFor(() => expect(result.current.keys.data).toEqual(rows));
    await waitFor(() => expect(result.current.packs.data).toEqual([]));
    await act(() => result.current.setKeys.mutateAsync({ drawer: "Ctrl+9" }));
    await act(() => result.current.install.mutateAsync());
    expect(seen).toEqual([
      ["keys", { map: { drawer: "Ctrl+9" } }],
      ["install", {}],
    ]);
    expect(calls).toEqual(
      expect.arrayContaining(["till_key_map", "datapack_installed", "till_set_key_map", "datapack_install"]),
    );
  });
});
