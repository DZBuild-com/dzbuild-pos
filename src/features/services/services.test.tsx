import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { consents, packState, serviceBoard, serviceTicket, sessionWith } from "@/test/fixtures-m8";
import { ServiceBoard } from "./ServiceBoard";
import { ServiceTicketPage } from "./ServiceTicketPage";

afterEach(() => clearMocks());
const tech = sessionWith(["sales.create", "customers.view"]);

describe("service tickets", () => {
  it("lays the tickets out by status", async () => {
    mockCommands({
      auth_session: () => tech,
      packs_state: () => packState({ modules: ["repairs"] }),
      service_board: () => serviceBoard(),
    });
    await renderM8(<ServiceBoard />, { session: tech });
    const ready = await screen.findByRole("region", { name: "Prêt" });
    expect(within(ready).getByText("RP2026-000004")).toBeInTheDocument();
    expect(within(ready).getByText("Samsung A54")).toBeInTheDocument();
  });

  it("copies the ready message and says whether the customer agreed to WhatsApp", async () => {
    // user-event's setup() puts its own clipboard on `navigator`: the spy goes on after it
    const user = userEvent.setup();
    const copied: string[] = [];
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText: async (text: string) => copied.push(text) },
    });
    mockCommands({
      auth_session: () => tech,
      packs_state: () => packState({ modules: ["repairs"] }),
      service_get: () => serviceTicket({ status: "ready" }),
      service_ready_message: () =>
        "Boutique El Amel : votre Samsung A54 (RP2026-000004) est prêt. Reste à payer 3 500,00 DA.",
      privacy_consents: () => consents({ whatsapp: true }),
    });
    await renderM8(<ServiceTicketPage id="st4" />, { session: tech });
    await user.click(await screen.findByRole("button", { name: "Copier le message" }));
    expect(copied[0]).toContain("RP2026-000004");
    expect(screen.getByText(/Consentement WhatsApp : oui/)).toBeInTheDocument();
  });

  it("allows only the next statuses of a repair", async () => {
    const sent: unknown[] = [];
    mockCommands({
      auth_session: () => tech,
      packs_state: () => packState({ modules: ["repairs"] }),
      service_get: () => serviceTicket({ status: "received" }),
      service_set_status: (args) => {
        sent.push(args);
        return serviceTicket({ status: "diagnosing" });
      },
      privacy_consents: () => [],
    });
    const user = userEvent.setup();
    await renderM8(<ServiceTicketPage id="st4" />, { session: tech });
    expect(await screen.findByRole("button", { name: "Diagnostic" })).toBeEnabled();
    expect(screen.queryByRole("button", { name: "Remettre au client" })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Diagnostic" }));
    await waitFor(() => expect(sent[0]).toMatchObject({ id: "st4", status: "diagnosing" }));
  });
});
