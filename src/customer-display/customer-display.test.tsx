import { act, render, screen, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { afterEach, describe, expect, it, vi } from "vitest";
import { CustomerDisplay } from "@/customer-display/CustomerDisplay";
import { frameFixture } from "@/test/fixtures-m4";
import { mockCommands } from "@/test/render";

const shop = {
  nameAr: "سوبيرات الأمل",
  nameFr: "Supérette El Amel",
  nameEn: "",
  logo: null,
  showLogo: true,
  thanksSeconds: 5,
};

afterEach(() => vi.useRealTimers());

describe("customer display", () => {
  it("renders only the frames it receives and thanks in three languages", async () => {
    const calls = mockCommands({ display_current: () => null, display_shop: () => shop });
    render(<CustomerDisplay />);
    expect(await screen.findByText("Supérette El Amel")).toBeInTheDocument();
    expect(screen.getByText("Bienvenue")).toBeInTheDocument();
    await act(() => emit("customer-display-update", { frame: frameFixture }));
    expect(await screen.findByText("Lait Candia 1 L")).toBeInTheDocument();
    expect(screen.getByTestId("amount-due")).toHaveTextContent("450,00");
    await act(() =>
      emit("customer-display-update", {
        frame: { ...frameFixture, mode: "paid", paid: 50_000, change: 5_000 },
      }),
    );
    expect(await screen.findByText("Merci")).toBeInTheDocument();
    expect(screen.getByText("شكراً")).toBeInTheDocument();
    expect(screen.getByText("Thank you")).toBeInTheDocument();
    expect(screen.getByTestId("change")).toHaveTextContent("50,00");
    expect(calls.filter((c) => !c.startsWith("plugin:event"))).toEqual(["display_current", "display_shop"]);
  });

  it("follows the ticket's language and direction", async () => {
    mockCommands({
      display_current: () => ({
        ...frameFixture,
        lang: "ar",
        line: { ...frameFixture.line!, name: "حليب كانديا" },
      }),
      display_shop: () => shop,
    });
    render(<CustomerDisplay />);
    expect(await screen.findByText("حليب كانديا")).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement.dir).toBe("rtl"));
  });

  it("goes back to the welcome screen after the thanks delay", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    mockCommands({
      display_current: () => ({ ...frameFixture, mode: "paid", paid: 45_000, change: 0 }),
      display_shop: () => ({ ...shop, thanksSeconds: 2 }),
    });
    render(<CustomerDisplay />);
    expect(await screen.findByText("Merci")).toBeInTheDocument();
    await act(async () => {
      vi.advanceTimersByTime(2_100);
    });
    expect(await screen.findByText("Bienvenue")).toBeInTheDocument();
  });

  it("shows the weight and the price per kilo while weighing", async () => {
    const weighing = { ...frameFixture, weight: { net: 1_250, stable: true, unitPrice: 12_000, tare: 150 } };
    mockCommands({ display_current: () => weighing, display_shop: () => shop });
    render(<CustomerDisplay />);
    expect(await screen.findByTestId("weight")).toHaveTextContent("1,25");
    expect(screen.getByTestId("weight")).toHaveTextContent("120,00");
  });

  it("shows the store QR while idle when the shop prints it", async () => {
    mockCommands({
      display_current: () => ({ ...frameFixture, mode: "idle", line: null, items: 0, total: 0 }),
      display_shop: () => ({
        ...shop,
        storeUrl: "https://amel.dzbuild.app",
        storeQr: { size: 21, modules: "1".repeat(441) },
      }),
    });
    render(<CustomerDisplay />);
    expect(await screen.findByRole("img", { name: "Commandez en ligne" })).toBeInTheDocument();
    expect(screen.getByText("amel.dzbuild.app")).toBeInTheDocument();
  });

  it("shows the BaridiMob QR while the customer pays by BaridiMob (HW-042)", async () => {
    mockCommands({
      display_current: () => ({ ...frameFixture, payQr: { size: 21, modules: "1".repeat(441) } }),
      display_shop: () => shop,
    });
    render(<CustomerDisplay />);
    expect(await screen.findByRole("img", { name: "Scannez avec BaridiMob pour payer" })).toBeInTheDocument();
    expect(screen.getByTestId("amount-due")).toHaveTextContent("450,00");
  });
});
