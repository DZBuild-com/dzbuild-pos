import { screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { visibleNavItems } from "@/app/nav";
import { AppRoutes } from "@/app/routes";
import { visibleKinds } from "@/features/documents/kinds";
import type { Permission, Session } from "@/lib/ipc/types";
import {
  ALL_PERMISSIONS,
  cashierSession,
  companyFixture,
  ownerSession,
  packFixture,
  settingsValues,
} from "@/test/fixtures";
import { documentFixture } from "@/test/fixtures-m1";
import { activeStatus } from "@/test/fixtures-m9";
import { type Handler, mockCommands, renderWithProviders } from "@/test/render";

/** A role holding `permission` and nothing else. */
function alone(permission: Permission): Session {
  return { ...ownerSession, isOwner: false, permissions: [permission] };
}

/** The shell's commands and an empty archive (and sales history). Returns the call log. */
function mockArchive(session: Session, extra: Record<string, Handler> = {}): string[] {
  return mockCommands({
    auth_session: () => session,
    // M9: the shell's licence banner (a lifetime licence: no banner)
    license_status: () => activeStatus,
    company_get: () => companyFixture,
    settings_get: ({ key }) => settingsValues[String(key)],
    datapack_get: () => packFixture,
    cash_current: () => null,
    documents_search: () => ({ rows: [], next: null }),
    documents_list: () => [],
    ...extra,
  });
}

beforeEach(() => {
  // jsdom has no layout: TanStack Virtual shows no row at 0×0, so every element measures 1024×600.
  vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(1024);
  vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(600);
});

describe("documents archive access", () => {
  // The M3 permissions rule: any of the four kinds' permissions or `sales.view_all` searches the
  // archive; `purchases.view` and `stock.transfer` find their own kinds there (`visible_kinds`).
  it.each([
    "documents.invoice",
    "documents.quote",
    "documents.delivery",
    "documents.credit_note",
    "sales.view_all",
    "purchases.view",
    "stock.transfer",
  ] as const)("opens the archive to a role holding %s alone", async (permission) => {
    const session = alone(permission);
    mockArchive(session);
    renderWithProviders(<AppRoutes session={session} />, { route: "/documents" });
    expect(await screen.findByRole("heading", { level: 1, name: "Documents" })).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "Menu principal" });
    expect(within(nav).getByRole("link", { name: "Documents" })).toBeInTheDocument();
  });

  it("opens a ticket of the archive to whoever sees every sale", async () => {
    const session = alone("sales.view_all");
    const calls = mockArchive(session, {
      documents_get: () => documentFixture,
      documents_state: () => ({ state: "final" }),
      documents_links: () => ({ sources: [], targets: [] }),
    });
    renderWithProviders(<AppRoutes session={session} />, { route: `/documents/${documentFixture.id}` });
    expect(
      await screen.findByRole("heading", { level: 1, name: "Ticket TK2026-C1-000123" }),
    ).toBeInTheDocument();
    expect(calls).toContain("documents_get");
  });

  it("keeps the archive out of a cashier's menu: reprinting his tickets shows him no kind there", async () => {
    mockArchive(cashierSession);
    renderWithProviders(<AppRoutes session={cashierSession} />, { route: "/documents" });
    // the address falls back to his first module, the sales history
    expect(await screen.findByRole("heading", { level: 1, name: "Tickets" })).toBeInTheDocument();
    const nav = screen.getByRole("navigation", { name: "Menu principal" });
    expect(within(nav).queryByRole("link", { name: "Documents" })).toBeNull();
  });

  it("shows the archive to exactly the permissions that see a kind in it", () => {
    for (const permission of ALL_PERMISSIONS) {
      const shown = visibleNavItems(alone(permission)).some((item) => item.path === "/documents");
      expect(shown, permission).toBe(visibleKinds([permission]).length > 0);
    }
  });
});
