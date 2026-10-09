import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { flexyOperators, sessionWith } from "@/test/fixtures-m8";
import { type Handler, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import {
  FlexyOperatorsSetting,
  NumberSetting,
  PolicySetting,
  ProductSetting,
  TrilingualSetting,
} from "./setting-cards";

const owner = sessionWith(["settings.company"]);

/** The settings as stored, and what `settings_set` received. */
function settings(values: Record<string, unknown>, extra: Record<string, Handler> = {}) {
  const saved: Array<[unknown, unknown]> = [];
  mockCommands({
    auth_session: () => owner,
    settings_get: ({ key }) => values[String(key)] ?? null,
    settings_set: ({ key, value }) => {
      saved.push([key, value]);
      values[String(key)] = value;
      return value;
    },
    ...extra,
  });
  return saved;
}

describe("the packs' settings where they are used (final review)", () => {
  it("turns SAL-030's ceiling from a warning into a refusal", async () => {
    const saved = settings({ "sales.regulated_policy": "warn" });
    const user = userEvent.setup();
    await renderM8(
      <PolicySetting
        settingKey="sales.regulated_policy"
        title="Vente au-dessus d'un prix plafonné"
        help="aide"
        warn="Avertir le caissier"
        block="Refuser la vente"
      />,
      { session: owner },
    );
    await user.click(await screen.findByRole("radio", { name: "Refuser la vente" }));
    await waitFor(() => expect(saved).toEqual([["sales.regulated_policy", "block"]]));
  });

  it("saves a number of days within its bounds only", async () => {
    const saved = settings({ "repairs.uncollected_days": 30 });
    const user = userEvent.setup();
    await renderM8(
      <NumberSetting
        settingKey="repairs.uncollected_days"
        id="days"
        title="Appareils non retirés"
        label="Jours après « prêt »"
        help="aide"
        min={1}
        max={365}
      />,
      { session: owner },
    );
    const input = await screen.findByLabelText("Jours après « prêt »");
    await user.clear(input);
    await user.type(input, "400");
    expect(screen.getByText("Entre 1 et 365")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Enregistrer" })).toBeDisabled();
    await user.clear(input);
    await user.type(input, "45");
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toEqual([["repairs.uncollected_days", 45]]));
  });

  it("saves a text in the three languages", async () => {
    const saved = settings({
      "instalments.reminder_message": { ar: "تذكير", fr: "Rappel", en: "Reminder" },
    });
    const user = userEvent.setup();
    await renderM8(
      <TrilingualSetting
        settingKey="instalments.reminder_message"
        id="message"
        title="Texte du rappel"
        help="aide"
        max={300}
      />,
      { session: owner },
    );
    const fr = await screen.findByLabelText("Français");
    await user.clear(fr);
    await user.type(fr, "Rappel {{number}");
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() =>
      expect(saved).toEqual([
        ["instalments.reminder_message", { ar: "تذكير", fr: "Rappel {number}", en: "Reminder" }],
      ]),
    );
  });

  it("names the product a kept deposit is sold as, and takes it back", async () => {
    const saved = settings(
      { "orders.kept_deposit_product_id": null },
      {
        products_search: () => [
          {
            id: "00000000-0000-7000-8000-0000000000aa",
            reference: "ARRHES",
            nameAr: "عربون",
            nameFr: "Arrhes conservées",
            nameEn: "Kept deposit",
            sellPrice: 0,
            unitCode: "pc",
          },
        ],
      },
    );
    const user = userEvent.setup();
    await renderM8(
      <ProductSetting
        settingKey="orders.kept_deposit_product_id"
        title="Arrhes conservées"
        help="aide"
        none="Aucun produit"
        clear="Retirer"
      />,
      { session: owner },
    );
    expect(await screen.findByText("Aucun produit")).toBeInTheDocument();
    await user.type(screen.getByRole("searchbox"), "arrhes");
    await user.click(await screen.findByRole("button", { name: /Arrhes conservées/ }));
    await waitFor(() =>
      expect(saved).toEqual([["orders.kept_deposit_product_id", "00000000-0000-7000-8000-0000000000aa"]]),
    );
  });

  it("sets each flexy operator's commission (VRT-002)", async () => {
    const saved = settings({}, { flexy_operators: () => flexyOperators() });
    const user = userEvent.setup();
    await renderM8(<FlexyOperatorsSetting />, { session: owner });
    const mobilis = await screen.findByLabelText("Commission (%)", { selector: "#flexy-commission-mobilis" });
    await user.clear(mobilis);
    await user.type(mobilis, "3,5");
    await user.click(screen.getByRole("button", { name: "Enregistrer" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toEqual([
      "flexy.operators",
      [
        { code: "mobilis", productId: "p-flexy-mobilis", commissionBp: 350 },
        { code: "djezzy", productId: "p-flexy-djezzy", commissionBp: 400 },
        { code: "ooredoo", productId: "p-flexy-ooredoo", commissionBp: 400 },
      ],
    ]);
  });
});
