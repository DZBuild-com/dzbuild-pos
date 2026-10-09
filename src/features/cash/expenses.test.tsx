import { fireEvent, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { CashScreen } from "@/features/cash/CashScreen";
import { ExpensesTab } from "@/features/cash/ExpensesTab";
import { ExpenseCategoriesSettings } from "@/features/settings/ExpenseCategoriesSettings";
import { cashierSession, ownerSession } from "@/test/fixtures";
import { cashSessionFixture } from "@/test/fixtures-m1";
import { expenseCategoriesFixture, expenseFixture } from "@/test/fixtures-m2";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

const TRANSPORT = "00000000-0000-7000-8000-000000000505";

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual measures its scroller with offsetWidth/offsetHeight and
  // shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

function mockExpenses(extra: Record<string, Handler> = {}) {
  return mockCommands({
    auth_session: () => ownerSession,
    cash_current: () => cashSessionFixture,
    expenses_categories: () => expenseCategoriesFixture,
    expenses_list: () => [],
    expenses_totals: () => [],
    suppliers_search: () => [],
    ...extra,
  });
}

describe("expense categories on a till", () => {
  it("are read-only, with the main PC's name and no add or edit button", async () => {
    mockCommands({
      auth_session: () => ownerSession,
      expenses_categories: () => expenseCategoriesFixture,
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<ExpenseCategoriesSettings />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    expect(await screen.findByText("Électricité et gaz")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Nouvelle catégorie" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Modifier" })).not.toBeInTheDocument();
  });
});

describe("expenses", () => {
  it("records a till expense in cash by default while the drawer is open", async () => {
    const recorded: unknown[] = [];
    mockExpenses({
      expenses_record: (args) => {
        recorded.push(args.input);
        return expenseFixture;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ExpensesTab />);
    await user.click(await screen.findByRole("button", { name: "Nouvelle dépense" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle dépense" });
    await user.selectOptions(await within(dialog).findByLabelText("Catégorie"), "Transport");
    await user.type(within(dialog).getByLabelText("Montant"), "800");
    expect(within(dialog).getByLabelText("Mode de paiement")).toHaveValue("cash");
    await waitFor(() =>
      expect(within(dialog).getByRole("checkbox", { name: "Payée avec l'argent du tiroir" })).toBeChecked(),
    );
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la dépense" }));
    await waitFor(() =>
      expect(recorded).toEqual([
        {
          categoryId: TRANSPORT,
          amount: 80_000,
          method: "cash",
          fromTill: true,
          reference: null,
          supplierId: null,
          businessDate: null,
          note: null,
        },
      ]),
    );
  });

  it("records a bank expense of an earlier day when the drawer is closed", async () => {
    const recorded: unknown[] = [];
    mockExpenses({
      cash_current: () => null,
      expenses_record: (args) => {
        recorded.push(args.input);
        return { ...expenseFixture, fromTill: false, method: "transfer" };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ExpensesTab />);
    await user.click(await screen.findByRole("button", { name: "Nouvelle dépense" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle dépense" });
    const fromTill = within(dialog).getByRole("checkbox", { name: "Payée avec l'argent du tiroir" });
    await waitFor(() => expect(fromTill).toBeDisabled());
    expect(fromTill).not.toBeChecked();
    await user.selectOptions(await within(dialog).findByLabelText("Catégorie"), "Électricité et gaz");
    await user.type(within(dialog).getByLabelText("Montant"), "12500,50");
    await user.selectOptions(within(dialog).getByLabelText("Mode de paiement"), "Virement");
    fireEvent.change(within(dialog).getByLabelText("Jour de la dépense"), {
      target: { value: "2026-09-28" },
    });
    await user.type(within(dialog).getByLabelText(/Référence/), "VIR-778");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la dépense" }));
    await waitFor(() =>
      expect(recorded).toEqual([
        {
          categoryId: "00000000-0000-7000-8000-000000000502",
          amount: 1_250_050,
          method: "transfer",
          fromTill: false,
          reference: "VIR-778",
          supplierId: null,
          businessDate: "2026-09-28",
          note: null,
        },
      ]),
    );
  });

  it("refuses an amount that is not one and a missing category", async () => {
    const calls = mockExpenses();
    const user = userEvent.setup();
    renderWithProviders(<ExpensesTab />);
    await user.click(await screen.findByRole("button", { name: "Nouvelle dépense" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle dépense" });
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la dépense" }));
    expect(await within(dialog).findByText("Saisissez un montant comme 1 000 000,00.")).toBeInTheDocument();
    expect(within(dialog).getByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(calls).not.toContain("expenses_record");
  });

  it("lists the period's expenses with their category totals and reverses one with a note", async () => {
    const reversed: unknown[] = [];
    mockExpenses({
      expenses_list: () => [expenseFixture],
      expenses_totals: () => [
        {
          categoryId: TRANSPORT,
          code: "transport",
          nameAr: "النقل",
          nameFr: "Transport",
          nameEn: "Transport",
          total: 80_000,
        },
      ],
      expenses_reverse: (args) => {
        reversed.push(args);
        return { ...expenseFixture, id: "e-2", amount: -80_000, reversalOf: "e-1" };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ExpensesTab />);
    const table = await screen.findByRole("table", { name: "Dépenses" });
    expect(await within(table).findByText("Transport")).toBeInTheDocument();
    expect(within(table).getByText(/800,00/)).toBeInTheDocument();
    const totals = screen.getByRole("list", { name: "Total par catégorie" });
    expect(within(totals).getByText(/800,00/)).toBeInTheDocument();
    await user.click(within(table).getByRole("button", { name: "Annuler la dépense" }));
    const dialog = await screen.findByRole("dialog", { name: "Annuler la dépense" });
    await user.click(within(dialog).getByRole("button", { name: "Annuler la dépense" }));
    expect(await within(dialog).findByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(reversed).toEqual([]);
    await user.type(within(dialog).getByLabelText("Motif de l'annulation"), "Saisie en double");
    await user.click(within(dialog).getByRole("button", { name: "Annuler la dépense" }));
    await waitFor(() => expect(reversed).toEqual([{ id: "e-1", note: "Saisie en double" }]));
  });

  it("opens the expenses tab of the cash screen for whoever reads them, not for a cashier", async () => {
    mockExpenses();
    const { unmount } = renderWithProviders(<CashScreen session={ownerSession} />, {
      route: "/cash?tab=expenses",
    });
    expect(await screen.findByRole("tab", { name: "Dépenses", selected: true })).toBeInTheDocument();
    expect(await screen.findByRole("button", { name: "Nouvelle dépense" })).toBeInTheDocument();
    unmount();
    mockExpenses({
      auth_session: () => cashierSession,
      auth_list_users: () => [],
      cash_movements: () => [],
      cash_sessions: () => [],
    });
    renderWithProviders(<CashScreen session={cashierSession} />, { route: "/cash?tab=expenses" });
    expect(await screen.findByText("Caisse ouverte")).toBeInTheDocument();
    expect(screen.queryByRole("tab")).not.toBeInTheDocument();
  });
});

describe("expense categories", () => {
  it("creates a category with its names and cash group, and archives one", async () => {
    const created: unknown[] = [];
    const archived: unknown[] = [];
    mockCommands({
      auth_session: () => ownerSession,
      expenses_categories: () => expenseCategoriesFixture,
      expenses_category_create: (args) => {
        created.push(args.input);
        return { ...expenseCategoriesFixture[4]!, id: "cat-fuel", code: "fuel" };
      },
      expenses_category_archive: (args) => {
        archived.push(args);
        return null;
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<ExpenseCategoriesSettings />);
    expect(await screen.findByText("Électricité et gaz")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Nouvelle catégorie" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle catégorie" });
    await user.type(within(dialog).getByLabelText("Code"), "fuel");
    await user.type(within(dialog).getByLabelText("Nom en français"), "Carburant");
    await user.type(within(dialog).getByLabelText("Nom en arabe"), "الوقود");
    await user.selectOptions(within(dialog).getByLabelText("Groupe dans la caisse"), "Transport");
    await user.clear(within(dialog).getByLabelText("Ordre"));
    await user.type(within(dialog).getByLabelText("Ordre"), "9");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la catégorie" }));
    await waitFor(() =>
      expect(created).toEqual([
        {
          code: "fuel",
          nameAr: "الوقود",
          nameFr: "Carburant",
          nameEn: "",
          cashGroup: "transport",
          sortOrder: 9,
        },
      ]),
    );
    const row = screen.getByText("water").closest("[role=row]") as HTMLElement;
    await user.click(within(row).getByRole("button", { name: "Archiver" }));
    const confirm = await screen.findByRole("dialog", { name: "Archiver cette catégorie ?" });
    await user.click(within(confirm).getByRole("button", { name: "Archiver" }));
    await waitFor(() => expect(archived).toEqual([{ id: "00000000-0000-7000-8000-000000000503" }]));
  });

  it("needs a code and at least one name", async () => {
    const calls = mockCommands({
      auth_session: () => ownerSession,
      expenses_categories: () => expenseCategoriesFixture,
    });
    const user = userEvent.setup();
    renderWithProviders(<ExpenseCategoriesSettings />);
    await user.click(await screen.findByRole("button", { name: "Nouvelle catégorie" }));
    const dialog = await screen.findByRole("dialog", { name: "Nouvelle catégorie" });
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la catégorie" }));
    expect(await within(dialog).findByText("Remplissez ce champ.")).toBeInTheDocument();
    expect(within(dialog).getByText("Remplissez au moins un nom.")).toBeInTheDocument();
    expect(calls).not.toContain("expenses_category_create");
  });
});
