import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ownerSession, settingsValues } from "@/test/fixtures";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";
import { PaymentsSettings } from "./PaymentsSettings";

const bnaTerminal = {
  id: "pt-bna",
  code: "tpe_bna",
  nameAr: "جهاز BNA",
  nameFr: "TPE BNA",
  nameEn: "BNA terminal",
  baseMethod: "card_cib",
  requiresReference: false,
  sortOrder: 0,
  archived: false,
};
/** A debit that needs its reference: never the till's one method. */
const ccpDebit = {
  ...bnaTerminal,
  id: "pt-ccp",
  code: "prelevement_ccp",
  nameFr: "Prélèvement CCP",
  baseMethod: "transfer",
  requiresReference: true,
};

function mockSettings(saved: unknown[], extra: Record<string, Handler> = {}) {
  const values: Record<string, unknown> = { ...settingsValues };
  return mockCommands({
    auth_session: () => ownerSession,
    settings_get: ({ key }) => values[String(key)] ?? null,
    settings_set: (args) => {
      saved.push(args);
      values[String(args.key)] = args.value;
      return args.value;
    },
    payment_types_list: () => [bnaTerminal, ccpDebit],
    payments_baridpay_qr: () => null,
    ...extra,
  });
}

const card = () => screen.getByRole("region", { name: "Encaissement à la caisse" });

describe("Settings › Payments: payment at the till", () => {
  it("saves one method, a custom type among the quick ones", async () => {
    const saved: unknown[] = [];
    mockSettings(saved);
    const user = userEvent.setup();
    renderWithProviders(<PaymentsSettings />);
    await user.click(await screen.findByRole("radio", { name: "Un seul mode" }));
    const method = within(card()).getByLabelText("Mode utilisé");
    // cash by default; card, Edahabia, BaridiMob and the quick custom type; never cheque or credit
    expect(method).toHaveValue("cash");
    const options = within(method)
      .getAllByRole("option")
      .map((o) => o.textContent);
    await waitFor(() => expect(within(method).getAllByRole("option")).toHaveLength(5));
    expect(options.slice(0, 4)).toEqual(["Espèces", "Carte CIB", "Edahabia", "BaridiMob"]);
    expect(within(method).getByRole("option", { name: "TPE BNA" })).toBeInTheDocument();
    expect(within(method).queryByRole("option", { name: "Prélèvement CCP" })).toBeNull();
    await user.selectOptions(method, "TPE BNA");
    await user.click(within(card()).getByRole("button", { name: "Enregistrer" }));
    await waitFor(() =>
      expect(saved).toEqual([
        { key: "payments.till_mode", value: "fixed" },
        { key: "payments.fixed_method", value: "type:pt-bna" },
      ]),
    );
  });

  it("refuses an empty list and saves the ticked methods", async () => {
    const saved: unknown[] = [];
    mockSettings(saved);
    const user = userEvent.setup();
    renderWithProviders(<PaymentsSettings />);
    expect(await screen.findByRole("radio", { name: "Choisir à la caisse" })).toBeChecked();
    for (const name of [
      "Espèces",
      "Carte CIB",
      "Edahabia",
      "BaridiMob",
      "Chèque",
      "Virement",
      "Crédit client",
      "Avoir client",
    ]) {
      await user.click(within(card()).getByRole("checkbox", { name }));
    }
    expect(within(card()).getByRole("alert")).toHaveTextContent("Cochez au moins un mode.");
    expect(within(card()).getByRole("button", { name: "Enregistrer" })).toBeDisabled();
    await user.click(within(card()).getByRole("checkbox", { name: "Edahabia" }));
    await user.click(within(card()).getByRole("checkbox", { name: "Carte CIB" }));
    expect(within(card()).queryByRole("alert")).toBeNull();
    await user.click(within(card()).getByRole("button", { name: "Enregistrer" }));
    await waitFor(() =>
      expect(saved).toEqual([{ key: "payments.allowed_methods", value: ["card_cib", "edahabia"] }]),
    );
  });

  it("is read-only on a till: the setting comes from the main PC", async () => {
    const saved: unknown[] = [];
    mockSettings(saved, { lan_status: () => lanTillStatusFixture });
    renderWithProviders(<PaymentsSettings />);
    expect(await screen.findByRole("note")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByRole("radio", { name: "Un seul mode" })).toBeDisabled());
    expect(within(card()).getByRole("checkbox", { name: "Espèces" })).toBeDisabled();
    expect(within(card()).queryByRole("button", { name: "Enregistrer" })).toBeNull();
  });
});
