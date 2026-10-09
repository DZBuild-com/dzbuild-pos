import { clearMocks, mockIPC } from "@tauri-apps/api/mocks";
import { renderHook, screen, waitFor } from "@testing-library/react";
import { useTranslation } from "react-i18next";
import { afterEach, describe, expect, it } from "vitest";
import { board, money, packState, sessionWith, yieldPreview } from "@/test/fixtures-m8";
import { providers } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { useYieldPreview, useYieldRun } from "./butcher";
import { commands } from "./client";
import { useModule, usePackState, useSetModules } from "./packs";
import { useChifaOf, useRegisterEntries } from "./pharmacy";
import { useDiningBoard, useSendToKitchen } from "./restaurant";

afterEach(() => clearMocks());

describe("pack hooks", () => {
  it("reads the pack state and says which modules are on", async () => {
    mockIPC((cmd) => (cmd === "packs_state" ? packState({ modules: ["restaurant", "loyalty"] }) : null));
    const { result } = renderHook(
      () => ({ state: usePackState(), restaurant: useModule("restaurant"), pharmacy: useModule("pharmacy") }),
      { wrapper: providers() },
    );
    await waitFor(() => expect(result.current.state.isSuccess).toBe(true));
    expect(result.current.restaurant).toBe(true);
    expect(result.current.pharmacy).toBe(false);
  });

  it("refreshes the floor after sending a ticket to the kitchen", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "dining_board") return board();
      if (cmd === "dining_ticket_send") return [];
      return null;
    });
    const { result } = renderHook(() => ({ board: useDiningBoard(null), send: useSendToKitchen() }), {
      wrapper: providers(),
    });
    await waitFor(() => expect(result.current.board.isSuccess).toBe(true));
    await result.current.send.mutateAsync("t1");
    await waitFor(() => expect(calls.filter((c) => c === "dining_board").length).toBe(2));
  });

  it("refreshes every screen after a module change: they all read the pack state", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "packs_state") return packState({ modules: [] });
      if (cmd === "packs_set_modules") return packState({ modules: ["pharmacy"] });
      return null;
    });
    const { result } = renderHook(() => ({ state: usePackState(), set: useSetModules() }), {
      wrapper: providers(),
    });
    await waitFor(() => expect(result.current.state.isSuccess).toBe(true));
    await result.current.set.mutateAsync(["pharmacy"]);
    await waitFor(() => expect(calls.filter((c) => c === "packs_state").length).toBe(2));
  });

  it("asks for a yield preview only with a carcass and its cuts, and refreshes the runs after a run", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "yield_preview") return yieldPreview();
      if (cmd === "yield_run") return null;
      return null;
    });
    const idle = renderHook(() => useYieldPreview(null), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const input = {
      templateId: null,
      sourceProductId: "p-carcass",
      locationId: null,
      inputQty: 20_000,
      inputCost: null,
      method: "weight" as const,
      cuts: [{ productId: "p-cut", qty: 15_000 }],
      note: null,
    };
    const preview = renderHook(() => useYieldPreview(input), { wrapper: providers() });
    await waitFor(() => expect(preview.result.current.data?.lossBp).toBe(703));
    const run = renderHook(() => useYieldRun(), { wrapper: providers() });
    await run.result.current.mutateAsync(input);
    expect(calls).toEqual(["yield_preview", "yield_run"]);
  });

  it("renders an M8 screen in its language with the session the backend answers", async () => {
    function Probe() {
      const { i18n } = useTranslation();
      return <p>{i18n.language}</p>;
    }
    const { calls } = await renderM8(<Probe />, { lang: "ar", session: sessionWith(["sales.create"], "ar") });
    expect(await screen.findByText("ar")).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
    await expect(commands.authSession()).resolves.toMatchObject({
      permissions: ["sales.create"],
      lang: "ar",
    });
    expect(calls).toEqual(["auth_session"]);
    expect(money(180_000)).toBe("1\u202F800,00\u00A0DA");
  });

  it("reads the register and a medicine's Chifa price only when asked", async () => {
    const calls: string[] = [];
    mockIPC((cmd) => {
      calls.push(cmd);
      if (cmd === "controlled_register_entries") return [];
      if (cmd === "chifa_of_product") return null;
      return null;
    });
    const idle = renderHook(() => useChifaOf(null), { wrapper: providers() });
    expect(idle.result.current.fetchStatus).toBe("idle");
    const entries = renderHook(() => useRegisterEntries("2026-09-01", "2026-09-30"), {
      wrapper: providers(),
    });
    await waitFor(() => expect(entries.result.current.isSuccess).toBe(true));
    expect(calls).toEqual(["controlled_register_entries"]);
  });
});
