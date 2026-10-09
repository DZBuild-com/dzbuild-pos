import { clearMocks } from "@tauri-apps/api/mocks";
import { cleanup, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it, vi } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { productHitsFixture } from "@/test/fixtures-m1";
import {
  cutProduct,
  packState,
  sessionWith,
  wasteDay,
  yieldPreview,
  yieldTemplates,
} from "@/test/fixtures-m8";
import { WasteScreen } from "./WasteScreen";
import { YieldScreen } from "./YieldScreen";

// unmount first: the screens' event listeners unregister through the mocked IPC
afterEach(() => {
  cleanup();
  clearMocks();
});

it("previews a cutting run live and validates it", async () => {
  const sent: unknown[] = [];
  const butcher = sessionWith(["stock.view", "stock.adjust"]);
  mockCommands({
    auth_session: () => butcher,
    packs_state: () => packState({ modules: ["yield", "waste"] }),
    yield_templates: () => yieldTemplates(),
    // a template names its carcass and its cuts by product: their names come in the UI language
    products_get: ({ id }) => cutProduct(String(id)),
    yield_preview: () => yieldPreview(),
    yield_run: (args) => {
      sent.push(args);
      return { id: "y1", number: "DC2026-000001" };
    },
  });
  const user = userEvent.setup();
  await renderM8(<YieldScreen />, { session: butcher });
  await user.selectOptions(await screen.findByLabelText("Modèle de découpe"), "Agneau entier");
  await user.type(screen.getByLabelText("Poids de la carcasse (kg)"), "18,500");
  await user.type(await screen.findByLabelText("Gigot (kg)"), "6");
  expect(await screen.findByText("Perte 7,03 %")).toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Valider la découpe" }));
  await waitFor(() => expect(sent[0]).toMatchObject({ input: { inputQty: 18_500 } }));
  expect(await screen.findByText("DC2026-000001")).toBeInTheDocument();
});

it("saves the day's waste per reason and shows the backend's total at cost", async () => {
  // jsdom has no layout: TanStack Virtual shows no row at 0x0, so every element measures 1024x600
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
  const sent: unknown[] = [];
  const butcher = sessionWith(["stock.view", "stock.adjust", "reports.profit"]);
  mockCommands({
    auth_session: () => butcher,
    packs_state: () => packState({ modules: ["yield", "waste"] }),
    products_search: () => productHitsFixture.slice(0, 1),
    waste_day: () => wasteDay(),
    waste_record: (args) => {
      sent.push(args);
      return [];
    },
  });
  const user = userEvent.setup();
  await renderM8(<WasteScreen />, { session: butcher });
  await user.type(await screen.findByRole("searchbox"), "Lait");
  await user.click(await screen.findByRole("button", { name: /Lait Candia 1 L/ }));
  await user.type(screen.getByLabelText("Quantité : Lait Candia 1 L"), "2");
  await user.selectOptions(screen.getByLabelText("Raison : Lait Candia 1 L"), "Péremption");
  await user.click(screen.getByRole("button", { name: "Enregistrer" }));
  await waitFor(() =>
    expect(sent[0]).toMatchObject({
      input: { lines: [{ productId: "p-milk", qty: 2_000, reason: "expiry" }] },
    }),
  );
  const day = await screen.findByRole("table", { name: "Pertes enregistrées aujourd'hui" });
  expect(within(day).getByText("Consommation interne")).toBeInTheDocument();
  expect(screen.getByText("Total au coût : 1 220,00 DA")).toBeInTheDocument();
});
