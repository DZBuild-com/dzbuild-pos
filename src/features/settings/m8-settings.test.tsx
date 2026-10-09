import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { mockCommands, renderWithProviders } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { ownerSession, rolesFixture, settingsValues, userFixture } from "@/test/fixtures";
import { keyMap, packState, sessionWith } from "@/test/fixtures-m8";
import { ScheduleCard } from "./backups/ScheduleCard";
import { KeysSettings } from "./KeysSettings";
import { PaymentsSettings } from "./PaymentsSettings";
import { SalesRulesSettings } from "./SalesRulesSettings";
import { UsersSettings } from "./UsersSettings";
import { PrivacyTab } from "@/features/customers/PrivacyTab";

// The setup file clears the IPC mocks after unmounting: clearing them first would break the LAN
// status listener's unlisten on unmount (an unhandled rejection).
const owner = sessionWith(["settings.company", "customers.manage", "customers.view"]);

// `auth_session` is mocked with the other commands: `renderM8`'s `session` would mock again and
// drop them.
describe("settings of the rows M6 handed over", () => {
  it("remaps a till key and refuses one already used", async () => {
    const saved: unknown[] = [];
    mockCommands({
      auth_session: () => owner,
      packs_state: () => packState({ modules: ["flexy"] }),
      till_key_map: () => keyMap(),
      till_set_key_map: (args) => {
        saved.push(args);
        return keyMap({ flexy: "Ctrl+8" });
      },
    });
    const user = userEvent.setup();
    await renderM8(<KeysSettings />);
    await user.click(await screen.findByRole("button", { name: "Changer la touche de Recharge flexy" }));
    await user.keyboard("{F9}");
    expect(screen.getByText("F9 est déjà la touche de Tiroir.")).toBeInTheDocument();
    await user.keyboard("{Control>}8{/Control}");
    await user.click(screen.getByRole("button", { name: "Enregistrer les touches" }));
    await waitFor(() => expect(saved[0]).toEqual({ map: { flexy: "Ctrl+8" } }));
  });

  it("refuses to anonymise a customer with open items and names them", async () => {
    mockCommands({
      auth_session: () => owner,
      packs_state: () => packState({ modules: [] }),
      privacy_consents: () => [],
      privacy_anonymise: () =>
        Promise.reject({ code: "customer_has_open_items", details: { open: ["balance", "orders"] } }),
    });
    const user = userEvent.setup();
    await renderM8(<PrivacyTab customerId="c1" />);
    await user.click(await screen.findByRole("button", { name: "Anonymiser" }));
    await user.click(screen.getByRole("button", { name: "Anonymiser définitivement" }));
    expect(await screen.findByText(/Solde du compte, commande en cours/)).toBeInTheDocument();
  });
});

/** The settings answer their stored value; each write is logged in `saved`. */
function settings(saved: unknown[]) {
  return {
    settings_get: ({ key }: Record<string, unknown>) => settingsValues[String(key)] ?? null,
    settings_set: (args: Record<string, unknown>) => {
      saved.push(args);
      return args.value;
    },
  };
}

describe("the other M8 settings (characterisation)", () => {
  it("saves the business-day cut-off hour alone, as a number", async () => {
    const saved: unknown[] = [];
    mockCommands(settings(saved));
    const user = userEvent.setup();
    renderWithProviders(<SalesRulesSettings />);
    await user.selectOptions(await screen.findByLabelText("Heure de changement de journée"), "04:00");
    expect(
      screen.getByText(
        "Les ventes faites avant cette heure comptent pour la veille (jamais pour l'année précédente).",
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Enregistrer les règles" }));
    await waitFor(() => expect(saved).toEqual([{ key: "sales.day_cutoff_hour", value: 4 }]));
  });

  it("keeps 48 hourly copies", async () => {
    const saved: unknown[] = [];
    mockCommands(settings(saved));
    const user = userEvent.setup();
    renderWithProviders(<ScheduleCard />);
    await user.selectOptions(
      await screen.findByLabelText("Copies horaires pendant l'ouverture de caisse"),
      "48",
    );
    await waitFor(() => expect(saved).toEqual([{ key: "backup.keep_hourly", value: 48 }]));
  });

  it("duplicates a user with a new name and PIN, never the owner", async () => {
    const asked: unknown[] = [];
    mockCommands({
      ...settings([]),
      users_list: () => [
        userFixture,
        { ...userFixture, id: "u-other", displayName: "Yacine", isOwner: false },
      ],
      roles_list: () => rolesFixture,
      users_duplicate: (args) => {
        asked.push(args);
        return { ...userFixture, id: "u-new", displayName: "Nadia", isOwner: false };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<UsersSettings session={ownerSession} />);
    const row = (await screen.findByText("Yacine")).closest("tr");
    const ownerRow = screen.getByText("Amine Benali").closest("tr");
    if (!row || !ownerRow) throw new Error("user rows missing");
    expect(within(ownerRow).queryByRole("button", { name: "Dupliquer" })).not.toBeInTheDocument();
    await user.click(within(row).getByRole("button", { name: "Dupliquer" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvel utilisateur comme Yacine" });
    await user.type(within(dialog).getByLabelText("Nom affiché sur la tuile"), "Nadia");
    await user.type(within(dialog).getByLabelText("Code PIN (4 à 8 chiffres)"), "4826");
    await user.type(within(dialog).getByLabelText("Retapez le code PIN"), "4826");
    await user.click(within(dialog).getByRole("button", { name: "Créer l'utilisateur" }));
    await waitFor(() =>
      expect(asked).toEqual([{ templateId: "u-other", displayName: "Nadia", pin: "4826" }]),
    );
  });

  it("creates a payment type on a transfer and saves the BaridPay QR text", async () => {
    const saved: unknown[] = [];
    const types: unknown[] = [];
    mockCommands({
      ...settings(saved),
      payment_types_list: () => [],
      payments_baridpay_qr: () => null,
      payment_type_save: (args) => {
        types.push(args);
        return { id: "pt1", ...(args.input as object), archived: false };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<PaymentsSettings />);
    await user.click(await screen.findByRole("button", { name: "Nouveau type de paiement" }));
    const dialog = await screen.findByRole("dialog");
    await user.type(within(dialog).getByLabelText("Code"), "prelevement_ccp");
    await user.type(within(dialog).getByLabelText("Nom en français"), "Prélèvement CCP");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer le type" }));
    await waitFor(() =>
      expect(types).toEqual([
        {
          id: null,
          input: {
            code: "prelevement_ccp",
            nameAr: "",
            nameFr: "Prélèvement CCP",
            nameEn: "",
            baseMethod: "transfer",
            requiresReference: true,
            sortOrder: 0,
          },
        },
      ]),
    );
    await user.type(screen.getByLabelText("Texte du QR"), "00020101021126BARIDPAY");
    await user.click(screen.getByRole("button", { name: "Enregistrer le QR" }));
    await waitFor(() =>
      expect(saved).toEqual([{ key: "payments.baridpay_qr", value: "00020101021126BARIDPAY" }]),
    );
  });
});
