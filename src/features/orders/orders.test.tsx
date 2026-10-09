import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { Route, Routes } from "react-router";
import { Toaster } from "sonner";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { SessionFrame } from "@/app/SessionFrame";
import { StatusPills } from "@/app/StatusPills";
import { SyncDot } from "@/app/SyncDot";
import { OrdersScreen } from "@/features/orders/OrdersScreen";
import { setLanguage } from "@/lib/i18n";
import type { Session } from "@/lib/ipc/types";
import { cashierSession, ownerSession, ownerTile } from "@/test/fixtures";
import { lanHostStatusFixture } from "@/test/fixtures-m5";
import { linkedStatus, order, orderSummary, unlinkedStatus } from "@/test/fixtures-m7";
import { activeStatus } from "@/test/fixtures-m9";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";

const beeps = vi.hoisted(() => [] as string[]);
vi.mock("@/lib/sounds", () => ({ beep: (kind: string) => beeps.push(kind) }));

beforeEach(() => {
  beeps.length = 0;
});

const statusPills = {
  cash_current: () => null,
  hardware_get: () => ({ receiptPrinterId: "p" }),
  // M9: the shell's licence banner (a lifetime licence: no banner)
  license_status: () => activeStatus,
};

describe("the sync dot", () => {
  it("is absent without a link and amber offline with the waiting count", async () => {
    mockCommands({ ...statusPills, cloud_status: () => unlinkedStatus });
    const { unmount } = renderWithProviders(<StatusPills session={cashierSession} />);
    await waitFor(() => expect(screen.queryByRole("img", { name: /DZBuild/ })).not.toBeInTheDocument());
    unmount();
    mockCommands({ ...statusPills, cloud_status: () => ({ ...linkedStatus, online: false }) });
    renderWithProviders(<StatusPills session={cashierSession} />);
    expect(
      await screen.findByRole("img", { name: "DZBuild : hors ligne, 3 en attente" }),
    ).toBeInTheDocument();
  });

  it("sits beside the shop network's pill on a main PC", async () => {
    mockCommands({
      ...statusPills,
      cloud_status: () => linkedStatus,
      lan_status: () => lanHostStatusFixture,
    });
    renderWithProviders(<StatusPills session={ownerSession} />);
    expect(
      await screen.findByRole("link", { name: "Postes de caisse connectés : 1 / 2" }),
    ).toBeInTheDocument();
    expect(
      await screen.findByRole("img", { name: "DZBuild : synchronisé, 3 en attente" }),
    ).toBeInTheDocument();
  });

  it("follows the engine's events", async () => {
    mockCommands({ ...statusPills, cloud_status: () => linkedStatus });
    renderWithProviders(<StatusPills session={cashierSession} />);
    await screen.findByRole("img", { name: "DZBuild : synchronisé, 3 en attente" });
    await act(() => emit("sync-status-changed", { status: { ...linkedStatus, pending: 0, failed: 2 } }));
    expect(await screen.findByRole("img", { name: "DZBuild : 2 envoi(s) refusé(s)" })).toBeInTheDocument();
  });
});

describe("online orders", () => {
  const base = {
    cloud_status: () => linkedStatus,
    cloud_orders_list: () => [orderSummary],
    cloud_orders_get: () => order,
    cloud_orders_seen: () => 1,
    cloud_customer_flag: () => null,
  };

  it("claims, shows the online price against the shop's, and marks the order seen", async () => {
    const calls = mockCommands({
      ...base,
      cloud_orders_claim: () => ({
        ...order,
        localState: "claimed",
        claimedByMe: true,
        claimConfirmed: true,
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    expect(await screen.findByText("Prix en ligne 90,00 DA · en magasin 100,00 DA")).toBeInTheDocument();
    expect(screen.getByText("après 17h")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Prendre en charge" }));
    await waitFor(() => expect(calls).toContain("cloud_orders_claim"));
    expect(calls).toContain("cloud_orders_seen");
  });

  it("a flagged buyer is a warning, never a block", async () => {
    mockCommands({ ...base, cloud_customer_flag: () => ({ isBanned: true, fraudScore: 90 }) });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    expect(await screen.findByRole("status", { name: "Acheteur signalé" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Prendre en charge" })).toBeEnabled();
  });

  it("sells at the till with the quote's amount due and asks a manager for the price difference", async () => {
    let refusals = 1;
    const sold: unknown[] = [];
    mockCommands({
      ...base,
      cloud_orders_get: () => ({ ...order, localState: "prepared", claimedByMe: true }),
      cloud_orders_quote: () => ({
        cart: { totals: { totalTtc: 18_000 } },
        settlement: { totalDue: 18_000, stampDuty: 0, change: 0 },
      }),
      cloud_orders_sell: (a) => {
        if (refusals-- > 0) return Promise.reject({ code: "price_override_not_allowed" });
        sold.push(a.request);
        return { sale: { document: { number: "T-000042", totalTtc: 18_000 } }, printQueued: true };
      },
      auth_list_users: () => [ownerTile],
      auth_session: () => cashierSession,
      auth_manager_override: () => ({
        approverId: ownerTile.id,
        approverName: ownerTile.displayName,
        permission: "sales.price_override",
        grantedAt: 0,
      }),
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    await user.click(await screen.findByRole("button", { name: "Encaisser à la caisse" }));
    expect(await screen.findByText("180,00 DA")).toBeInTheDocument();
    // accessible names keep formatDZD's no-break space: match it loosely
    await user.click(screen.getByRole("button", { name: /Encaisser 180,00.DA/ }));
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    expect(sold).toEqual([]);
  });

  it("an order paid online is handed over, never cashed again at the till", async () => {
    const handed: unknown[] = [];
    mockCommands({
      ...base,
      cloud_orders_get: () => ({
        ...order,
        paymentMethod: "digital_payment",
        localState: "prepared",
        claimedByMe: true,
      }),
      cloud_orders_hand_over: (a) => {
        handed.push(a);
        return { sale: { document: { number: "T-000043", totalTtc: 20_000 } }, printQueued: false };
      },
      auth_session: () => cashierSession,
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    expect(await screen.findByText("Déjà payée en ligne : rien à encaisser.")).toBeInTheDocument();
    expect(screen.getAllByText("Payée en ligne").length).toBeGreaterThan(0);
    expect(screen.queryByRole("button", { name: "Encaisser à la caisse" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Expédier avec un BL" })).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Remettre la commande" }));
    await waitFor(() =>
      expect(handed).toEqual([{ id: "o1", shippingLabel: "Livraison", courier: false, printReceipt: false }]),
    );
  });

  it("the orders item is in the rail only for a linked shop, with the unseen count", async () => {
    const { NavRail } = await import("@/app/NavRail");
    mockCommands({ cloud_status: () => ({ ...linkedStatus, unseenOrders: 2 }) });
    renderWithProviders(<NavRail session={cashierSession} />);
    expect(await screen.findByRole("link", { name: /Commandes en ligne.*2/ })).toBeInTheDocument();
  });

  it("an unlinked shop (or a LAN till) has no orders item in its rail", async () => {
    const { NavRail } = await import("@/app/NavRail");
    const calls = mockCommands({ cloud_status: () => unlinkedStatus });
    renderWithProviders(<NavRail session={cashierSession} />);
    expect(await screen.findByRole("link", { name: "Vendre" })).toBeInTheDocument();
    await waitFor(() => expect(calls).toContain("cloud_status"));
    expect(screen.queryByRole("link", { name: /Commandes en ligne/ })).not.toBeInTheDocument();
  });

  it("ships with a delivery note through a manager's approval when the online prices need one", async () => {
    const shipper: Session = {
      ...cashierSession,
      permissions: [...cashierSession.permissions, "documents.delivery"],
    };
    let refusals = 1;
    const shipped: unknown[] = [];
    mockCommands({
      ...base,
      cloud_orders_get: () => ({ ...order, localState: "prepared", claimedByMe: true }),
      cloud_orders_ship: (a) => {
        if (refusals-- > 0) return Promise.reject({ code: "price_override_not_allowed" });
        shipped.push(a);
        return { id: "d1" };
      },
      auth_list_users: () => [ownerTile],
      auth_session: () => shipper,
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={shipper} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    await user.click(await screen.findByRole("button", { name: "Expédier avec un BL" }));
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    expect(beeps).toEqual(["error"]);
    expect(shipped).toEqual([]);
  });

  it("refuses with the reason and an optional note", async () => {
    const refused: unknown[] = [];
    mockCommands({
      ...base,
      cloud_orders_refuse: (a) => {
        refused.push(a);
        return { ...order, localState: "refused", refuseReason: a.reason };
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    await user.click(await screen.findByRole("button", { name: "Refuser" }));
    const dialog = await screen.findByRole("dialog", { name: "Refuser la commande 1042 ?" });
    await user.click(within(dialog).getByRole("radio", { name: "Acheteur injoignable" }));
    await user.type(within(dialog).getByLabelText("Précision (facultative)"), "  3 appels ");
    await user.click(within(dialog).getByRole("button", { name: "Refuser" }));
    await waitFor(() =>
      expect(refused).toEqual([{ id: "o1", reason: "customer_unreachable", note: "3 appels" }]),
    );
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("a cashier without the delivery note sells at the till but is offered no shipping", async () => {
    mockCommands({
      ...base,
      cloud_orders_get: () => ({ ...order, localState: "claimed", claimedByMe: true }),
    });
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    expect(await screen.findByRole("button", { name: "Encaisser à la caisse" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Préparer" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Expédier avec un BL" })).not.toBeInTheDocument();
  });

  it("reads in Arabic without a missing word", async () => {
    await setLanguage("ar");
    mockCommands({ ...base, cloud_customer_flag: () => ({ isBanned: false, fraudScore: 80 }) });
    missingKeys.clear();
    const user = userEvent.setup();
    renderWithProviders(<OrdersScreen session={cashierSession} />);
    await user.click(await screen.findByRole("button", { name: /1042/ }));
    expect(await screen.findByRole("status", { name: "مشترٍ مُبلَّغ عنه" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "التكفّل" })).toBeInTheDocument();
    expect([...missingKeys]).toEqual([]);
  });
});

describe("new online orders", () => {
  function signedIn(session: Session) {
    return renderWithProviders(
      <>
        <Toaster />
        <Routes>
          <Route element={<SessionFrame session={session} />}>
            <Route index element={<SyncDot />} />
          </Route>
        </Routes>
      </>,
    );
  }

  it("whoever sells hears them arrive, not the ones already waiting at sign-in", async () => {
    mockCommands({ cloud_status: () => ({ ...linkedStatus, unseenOrders: 1 }), settings_get: () => null });
    signedIn(cashierSession);
    await screen.findByRole("img", { name: "DZBuild : synchronisé, 3 en attente" });
    expect(beeps).toEqual([]);
    await act(() => emit("sync-status-changed", { status: { ...linkedStatus, unseenOrders: 3 } }));
    expect(await screen.findByText("3 nouvelles commandes en ligne.")).toBeInTheDocument();
    expect(beeps).toEqual(["ok"]);
  });

  it("a user who does not sell is not disturbed", async () => {
    const storekeeper: Session = { ...cashierSession, permissions: ["products.view", "stock.view"] };
    mockCommands({ cloud_status: () => linkedStatus, settings_get: () => null });
    signedIn(storekeeper);
    await screen.findByRole("img", { name: "DZBuild : synchronisé, 3 en attente" });
    await act(() => emit("sync-status-changed", { status: { ...linkedStatus, unseenOrders: 2 } }));
    expect(screen.queryByText(/nouvelles commandes en ligne/)).not.toBeInTheDocument();
    expect(beeps).toEqual([]);
  });
});
