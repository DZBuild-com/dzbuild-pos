import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { jewellerEntry, packState, sessionWith } from "@/test/fixtures-m8";
import { JewellerRegister } from "./JewellerRegister";

// The IPC mocks are cleared by the setup file after each test, once the screen is unmounted.
beforeEach(() => {
  // jsdom has no layout: M2's DataTable (TanStack Virtual) shows no row at 0×0.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

const manager = sessionWith(["customers.view", "settings.company"]);
const stored = { number: "117/2026", date: "2026-01-15", authority: "Direction de la réglementation" };

/** The register's backend; `settings_get` answers `authorisation` (or a promise of it) and a cash
 * limit of 50 000 DA; every `settings_set` is logged with its key and value. */
function backend(authorisation: unknown) {
  const saved: Array<{ key: unknown; value: unknown }> = [];
  mockCommands({
    auth_session: () => manager,
    packs_state: () => packState({ modules: ["jewellery"] }),
    jeweller_register_entries: () => [jewellerEntry()],
    settings_get: ({ key }) => (key === "jewellery.max_cash" ? 5_000_000 : authorisation),
    settings_set: ({ key, value }) => {
      saved.push({ key, value });
      return value;
    },
  });
  return saved;
}

describe("the register's authorisation and cash limit", () => {
  it("keeps the stored authorisation when it arrives after the card and only the limit is saved", async () => {
    let arrive: (value: unknown) => void = () => undefined;
    const saved = backend(
      new Promise((resolve) => {
        arrive = resolve;
      }),
    );
    const user = userEvent.setup();
    await renderM8(<JewellerRegister year={2026} />);
    const number = await screen.findByLabelText("N° de l'autorisation");
    expect(number).toHaveValue("");
    arrive(stored);
    await screen.findByText(/Autorisation n° 117\/2026 du 2026-01-15/);
    await waitFor(() => expect(number).toHaveValue("117/2026"));
    expect(screen.getByLabelText("Délivrée par")).toHaveValue("Direction de la réglementation");
    const limit = screen.getByLabelText("Plafond des paiements en espèces");
    await user.clear(limit);
    await user.type(limit, "200000");
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toEqual([{ key: "jewellery.max_cash", value: 20_000_000 }]));
  });

  it("saves the authorisation with the stored fields the user left alone, and not the untouched limit", async () => {
    const saved = backend(stored);
    const user = userEvent.setup();
    await renderM8(<JewellerRegister year={2026} />);
    await screen.findByText(/Autorisation n° 117\/2026 du 2026-01-15/);
    const authority = await screen.findByLabelText("Délivrée par");
    await waitFor(() => expect(authority).toHaveValue("Direction de la réglementation"));
    await user.clear(authority);
    await user.type(authority, "Wilaya d'Alger");
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() =>
      expect(saved).toEqual([
        {
          key: "jewellery.authorisation",
          value: { number: "117/2026", date: "2026-01-15", authority: "Wilaya d'Alger" },
        },
      ]),
    );
  });

  it("clears the authorisation when the user empties every field", async () => {
    const saved = backend(stored);
    const user = userEvent.setup();
    await renderM8(<JewellerRegister year={2026} />);
    const number = await screen.findByLabelText("N° de l'autorisation");
    await waitFor(() => expect(number).toHaveValue("117/2026"));
    await user.clear(number);
    await user.clear(screen.getByLabelText("Date de l'autorisation"));
    await user.clear(screen.getByLabelText("Délivrée par"));
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toEqual([{ key: "jewellery.authorisation", value: null }]));
  });
});
