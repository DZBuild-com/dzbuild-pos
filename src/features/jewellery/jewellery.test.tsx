import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { jewellerEntry, metalBoard, packState, sessionWith } from "@/test/fixtures-m8";
import { JewellerRegister } from "./JewellerRegister";
import { JewelleryScreen } from "./JewelleryScreen";
import { MetalBoard } from "./MetalBoard";

// The IPC mocks are cleared by the setup file after each test, once the screen is unmounted: the
// register reads the LAN status, whose event listener is removed through those mocks.
beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600 (the register is M2's DataTable).
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});
const owner = sessionWith(["products.view", "products.price", "customers.view", "customers.manage"]);

describe("jewellery", () => {
  it("sets a new gold price and reprices the catalogue", async () => {
    const calls: string[] = [];
    mockCommands({
      auth_session: () => owner,
      packs_state: () => packState({ modules: ["jewellery"] }),
      metal_board: () => metalBoard(),
      metal_price_set: () => {
        calls.push("set");
        return metalBoard()[0];
      },
      jewellery_reprice: () => {
        calls.push("reprice");
        return { pieces: 12, changed: 12 };
      },
    });
    const user = userEvent.setup();
    await renderM8(<MetalBoard />);
    const gold = (await screen.findByText("Or 750 ‰ (18 carats)")).closest("tr") as HTMLElement;
    await user.click(within(gold).getByRole("button", { name: "Nouveau prix" }));
    await user.type(screen.getByLabelText("Prix du gramme"), "15000");
    await user.click(screen.getByRole("button", { name: "Enregistrer le prix" }));
    await user.click(screen.getByRole("button", { name: "Recalculer les prix du catalogue" }));
    await waitFor(() => expect(calls).toEqual(["set", "reprice"]));
    expect(await screen.findByText("12 prix mis à jour")).toBeInTheDocument();
  });

  it("lists the register with the buyer's identity for authorised users", async () => {
    mockCommands({
      auth_session: () => owner,
      packs_state: () => packState({ modules: ["jewellery"] }),
      jeweller_register_entries: () => [jewellerEntry()],
      settings_get: () => ({
        number: "117/2026",
        date: "2026-01-15",
        authority: "Direction de la réglementation",
      }),
    });
    await renderM8(<JewellerRegister year={2026} />);
    expect(await screen.findByText("Samira Belkacem")).toBeInTheDocument();
    expect(screen.getByText(/Autorisation n° 117\/2026 du 2026-01-15/)).toBeInTheDocument();
  });

  it("shows the register to a manager, never to a cashier who only reads customers", async () => {
    mockCommands({
      packs_state: () => packState({ modules: ["jewellery"] }),
      metal_board: () => metalBoard(),
    });
    const cashier = sessionWith(["sales.create", "products.view", "customers.view"]);
    const { unmount } = await renderM8(<JewelleryScreen session={cashier} />);
    expect(await screen.findByRole("tab", { name: "Prix du gramme" })).toBeInTheDocument();
    expect(screen.queryByRole("tab", { name: "Registre" })).not.toBeInTheDocument();
    unmount();
    await renderM8(<JewelleryScreen session={sessionWith(["products.view", "customers.manage"])} />);
    expect(await screen.findByRole("tab", { name: "Registre" })).toBeInTheDocument();
  });
});
