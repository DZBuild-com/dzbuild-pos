import { screen, waitFor, within } from "@testing-library/react";
import userEvent, { type UserEvent } from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { LoginScreen } from "@/features/auth/LoginScreen";
import { SetupWizard } from "@/features/setup/SetupWizard";
import { useSetupStatus } from "@/lib/ipc/setup";
import { appInfoFixture, ownerSession, ownerTile, packFixture, settingsValues } from "@/test/fixtures";
import { hardwareConfig, templateInfos } from "@/test/fixtures-m8";
import { mockCommands, renderWithProviders } from "@/test/render";

describe("SetupWizard", () => {
  it("creates the shop at the owner step, then sets the printer up", async () => {
    const received: unknown[] = [];
    const saved: unknown[] = [];
    mockCommands({
      datapack_get: () => packFixture,
      setup_complete_first_run: (args) => {
        received.push(args);
        return ownerSession;
      },
      hardware_get: () => ({
        version: 1,
        printers: [],
        receiptPrinterId: null,
        receipt: {
          printByDefault: true,
          openDrawerOnCash: true,
          drawerBeforePrint: false,
          logo: true,
          secondLang: null,
        },
        scanner: { layout: "us" },
      }),
      hardware_discover: () => [
        {
          kind: "windows_printer",
          name: "XP-80C",
          port: "USB001",
          driver: "XP-80",
          isDefault: false,
          offline: false,
          likelyReceipt: true,
        },
      ],
      hardware_test_print: () => ({ printer: "Caisse 1", bytes: 3_000, raster: true }),
      hardware_save: ({ config }) => {
        saved.push(config);
        return config;
      },
    });
    const user = userEvent.setup();
    const { client } = renderWithProviders(<SetupWizard />);
    client.setQueryData(["setup", "status"], {
      completed: false,
      hasOwner: false,
      terminalCode: "C1",
      loginLang: "fr",
    });
    await user.click(screen.getByRole("button", { name: "Français" }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.type(screen.getByLabelText("Nom de la boutique"), "Supérette El Amel");
    await screen.findByRole("option", { name: "16 Alger" });
    await user.selectOptions(screen.getByLabelText(/Wilaya/), "16");
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.type(screen.getByLabelText("Votre nom"), "Amine Benali");
    await user.type(screen.getByLabelText("Code PIN (4 à 8 chiffres)"), "2580");
    await user.type(screen.getByLabelText("Retapez le code PIN"), "2580");
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await waitFor(() => expect(received).toHaveLength(1));
    expect(received[0]).toEqual({
      input: {
        lang: "fr",
        shopName: "Supérette El Amel",
        activity: "general",
        taxRegime: "ifu",
        priceMode: "ttc",
        wilaya: "16",
        phone: null,
        ownerName: "Amine Benali",
        ownerPin: "2580",
      },
    });
    expect(await screen.findByText("Imprimante de tickets")).toBeInTheDocument();
    // a supérette's last step is the scale: the free note waits for it
    expect(screen.queryByText(/gratuit pour toujours/)).toBeNull();
    // the shop exists, but the gate keeps the wizard until the printer step ends
    expect(client.getQueryData(["setup", "status"])).toMatchObject({ completed: false });
    expect(screen.queryByRole("button", { name: /Retour/ })).toBeNull();
    await user.click(screen.getByRole("button", { name: "Détecter les imprimantes" }));
    await user.click(await screen.findByRole("radio", { name: /XP-80C/ }));
    await user.click(screen.getByRole("button", { name: "Imprimer une page de test" }));
    expect(await screen.findByText("Page de test envoyée à Caisse 1.")).toBeInTheDocument();
    // a supérette (the default activity) weighs its fruit: the scale step comes last (ONB-009)
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    expect(await screen.findByRole("heading", { name: "Balance" })).toBeInTheDocument();
    expect(screen.getByText(/gratuit pour toujours/)).toBeInTheDocument();
    expect(saved).toHaveLength(0);
    await user.click(screen.getByRole("button", { name: "Terminer et ouvrir le logiciel" }));
    await waitFor(() => expect(saved).toHaveLength(1));
    expect(saved[0]).toMatchObject({
      receiptPrinterId: expect.any(String),
      printers: [{ name: "Caisse 1", transport: { kind: "windows", printerName: "XP-80C" } }],
    });
    await waitFor(() => expect(client.getQueryData(["setup", "status"])).toMatchObject({ completed: true }));
  });

  it("lets the owner skip the printer for now", async () => {
    const opened: unknown[] = [];
    mockCommands({
      app_open_link: ({ kind }: Record<string, unknown>) => {
        opened.push(kind);
        return null;
      },
      datapack_get: () => packFixture,
      packs_templates: () => templateInfos(),
      setup_complete_first_run: () => ownerSession,
      hardware_get: () => ({
        version: 1,
        printers: [],
        receiptPrinterId: null,
        receipt: {
          printByDefault: true,
          openDrawerOnCash: true,
          drawerBeforePrint: false,
          logo: true,
          secondLang: null,
        },
        scanner: { layout: "us" },
      }),
    });
    const user = userEvent.setup();
    const { client } = renderWithProviders(<SetupWizard />);
    client.setQueryData(["setup", "status"], {
      completed: false,
      hasOwner: false,
      terminalCode: "C1",
      loginLang: "fr",
    });
    // a restaurant has no scale step: the printer step is the last one
    await fillUpToPrinter(user, undefined, "Restaurant, café, fast-food");
    // the last step says once that the POS is free, with the DZBuild sign-up a click away
    expect(
      screen.getByText(
        /DZBuild POS est gratuit pour toujours\. Envie d'un site pour votre boutique aussi \?/,
      ),
    ).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "dzbuild.com/register" }));
    await waitFor(() => expect(opened).toEqual(["dzbuild_register"]));
    await user.click(screen.getByRole("button", { name: "Passer pour l'instant" }));
    expect(client.getQueryData(["setup", "status"])).toMatchObject({ completed: true, hasOwner: true });
  });

  it("blocks empty names, weak and different PINs", async () => {
    mockCommands({ datapack_get: () => packFixture });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    expect(screen.getByText("Remplissez ce champ.")).toBeInTheDocument();
    await user.type(screen.getByLabelText("Nom de la boutique"), "El Amel");
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    await user.type(screen.getByLabelText("Votre nom"), "Amine");
    await user.type(screen.getByLabelText("Code PIN (4 à 8 chiffres)"), "1234");
    await user.type(screen.getByLabelText("Retapez le code PIN"), "1243");
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    expect(
      screen.getByText("Ce code PIN est trop facile à deviner (comme 1111 ou 1234)."),
    ).toBeInTheDocument();
    expect(screen.getByText("Les deux codes PIN sont différents. Retapez-les.")).toBeInTheDocument();
  });

  it("switches the whole wizard to Arabic, right to left", async () => {
    mockCommands({ datapack_get: () => packFixture });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await user.click(screen.getByRole("button", { name: "العربية" }));
    // Arabic's texts load on first use (src/lib/i18n.ts).
    await waitFor(() => expect(document.documentElement).toHaveAttribute("dir", "rtl"));
    expect(await screen.findByText("اختر لغة البرنامج")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "التالي" })).toBeInTheDocument();
  });

  it("comes back in the language chosen on this PC before (leaving the demo, a restart)", async () => {
    window.localStorage.setItem("dzpos.setup.lang", "ar");
    mockCommands({ datapack_get: () => packFixture });
    renderWithProviders(<SetupWizard />);
    expect(await screen.findByText("اختر لغة البرنامج")).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement).toHaveAttribute("dir", "rtl"));
  });

  it("starts in Windows' language on the very first start, and remembers a choice", async () => {
    Object.defineProperty(window.navigator, "language", { value: "en-GB", configurable: true });
    try {
      mockCommands({ datapack_get: () => packFixture });
      const user = userEvent.setup();
      renderWithProviders(<SetupWizard />);
      expect(await screen.findByText("Choose the language of the app")).toBeInTheDocument();
      await user.click(screen.getByRole("button", { name: "Français" }));
      expect(window.localStorage.getItem("dzpos.setup.lang")).toBe("fr");
    } finally {
      Object.defineProperty(window.navigator, "language", { value: "fr-FR", configurable: true });
    }
  });

  it("keeps the way on in view, then offers the demo, the move of a shop and the tour", async () => {
    mockCommands({ datapack_get: () => packFixture });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    const next = await screen.findByRole("button", { name: "Suivant" });
    const move = screen.getByRole("button", {
      name: "Transférer ma boutique depuis un autre PC (fichier .dzpos)",
    });
    expect(next.compareDocumentPosition(move) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
    expect(next.parentElement).toHaveClass("sticky");
    expect(screen.getByText(/N'encaissez plus sur l'ancien PC/)).toBeInTheDocument();
    await user.click(screen.getByRole("radio", { name: /Poste de caisse d'une boutique/ }));
    expect(
      screen.queryByRole("button", { name: "Transférer ma boutique depuis un autre PC (fichier .dzpos)" }),
    ).not.toBeInTheDocument();
  });

  it("sends a phone typed in Arabic-Indic digits as ASCII digits", async () => {
    const received: unknown[] = [];
    mockCommands({
      datapack_get: () => packFixture,
      setup_complete_first_run: (args) => {
        received.push(args);
        return ownerSession;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await fillUpToPrinter(user, "٠٥٥٥ ١٢ ٣٤ ٥٦");
    await waitFor(() => expect(received).toHaveLength(1));
    expect(received[0]).toMatchObject({ input: { phone: "0555 12 34 56" } });
  });

  it("sends the user to the login screen when the setup was already done", async () => {
    mockCommands({
      setup_status: () => ({ completed: false, hasOwner: false, terminalCode: "C1", loginLang: "fr" }),
      datapack_get: () => packFixture,
      setup_complete_first_run: () => Promise.reject({ code: "setup_already_done" }),
      auth_list_users: () => [ownerTile],
      company_get: () => null,
      settings_get: ({ key }) => settingsValues[String(key)],
      settings_set: ({ value }) => value,
    });
    const user = userEvent.setup();
    renderWithProviders(<SetupGate />);
    await screen.findByText("Choisissez la langue du logiciel");
    await fillOwner(user);
    expect(await screen.findByText("Qui travaille ?")).toBeInTheDocument();
    expect(screen.queryByText("Imprimante de tickets")).toBeNull();
  });

  it("opens the join wizard when the PC is a till of an existing shop", async () => {
    mockCommands({
      datapack_get: () => packFixture,
      app_info: () => appInfoFixture,
      lan_discover: () => [],
    });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await user.click(await screen.findByRole("radio", { name: /Poste de caisse d'une boutique/ }));
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    expect(await screen.findByRole("heading", { name: "Rejoindre le poste principal" })).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Retour" }));
    expect(await screen.findByRole("radio", { name: /Poste de caisse d'une boutique/ })).toBeChecked();
  });

  it("applies the chosen template at first run and offers the scale to a butcher", async () => {
    const received: unknown[] = [];
    mockCommands({
      datapack_get: () => packFixture,
      packs_templates: () => templateInfos(),
      setup_complete_first_run: (args) => {
        received.push(args);
        return ownerSession;
      },
      hardware_get: () => hardwareConfig(),
      hardware_discover: () => [],
    });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await fillOwner(user, undefined, "Boucherie");
    await waitFor(() => expect(received).toHaveLength(1));
    expect(received[0]).toMatchObject({ input: { activity: "butcher" } });
    expect(await screen.findByText("Imprimante de tickets")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Passer" }));
    expect(await screen.findByRole("heading", { name: "Balance" })).toBeInTheDocument();
  });

  it("lists the ready templates first, each with its till screen and modules", async () => {
    mockCommands({ datapack_get: () => packFixture, packs_templates: () => templateInfos() });
    const user = userEvent.setup();
    renderWithProviders(<SetupWizard />);
    await user.click(screen.getByRole("button", { name: "Suivant" }));
    const ready = await screen.findByRole("group", { name: "Prêts à l'emploi" });
    const growing = screen.getByRole("group", { name: "Bientôt enrichis" });
    expect(within(growing).getByRole("button", { name: "Bijouterie" })).toBeInTheDocument();
    const butcher = within(ready).getByRole("button", { name: "Boucherie" });
    expect(butcher).toHaveAccessibleDescription(/Commerce.*Rendement de découpe.*Pertes du jour/);
    expect(within(ready).getByRole("button", { name: "Supérette / épicerie" })).toHaveAttribute(
      "aria-pressed",
      "true",
    );
    await user.click(butcher);
    expect(butcher).toHaveAttribute("aria-pressed", "true");
  });
});

/** The App gate in miniature: the wizard until the setup is done, then the login screen. */
function SetupGate() {
  const status = useSetupStatus(true);
  if (!status.data) return null;
  return status.data.completed ? <LoginScreen /> : <SetupWizard />;
}

/** From the language step through the owner step (whose "Suivant" sends the first run); `activity`
 * is the name of the template card chosen in the shop step. */
async function fillOwner(user: UserEvent, phone?: string, activity?: string) {
  await user.click(screen.getByRole("button", { name: "Suivant" }));
  await user.type(screen.getByLabelText("Nom de la boutique"), "El Amel");
  if (phone) await user.type(screen.getByLabelText(/Téléphone/), phone);
  if (activity) await user.click(await screen.findByRole("button", { name: activity }));
  await user.click(screen.getByRole("button", { name: "Suivant" }));
  await user.type(screen.getByLabelText("Votre nom"), "Amine");
  await user.type(screen.getByLabelText("Code PIN (4 à 8 chiffres)"), "2580");
  await user.type(screen.getByLabelText("Retapez le code PIN"), "2580");
  await user.click(screen.getByRole("button", { name: "Suivant" }));
}

/** Up to the printer step, with valid answers. */
async function fillUpToPrinter(user: UserEvent, phone?: string, activity?: string) {
  await fillOwner(user, phone, activity);
  expect(await screen.findByText("Imprimante de tickets")).toBeInTheDocument();
}
