import { clearMocks } from "@tauri-apps/api/mocks";
import { screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AppointmentBook } from "@/features/customer-orders/AppointmentBook";
import { CustomerOrdersScreen } from "@/features/customer-orders/CustomerOrdersScreen";
import { OrderPage } from "@/features/customer-orders/OrderPage";
import { missingKeys, mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { appointments, order, packState, plan, sessionWith } from "@/test/fixtures-m8";
import { InstalmentsScreen } from "./InstalmentsScreen";
import { PlanPage } from "./PlanPage";

afterEach(() => clearMocks());
const manager = sessionWith([
  "sales.create",
  "sales.return",
  "credit.view",
  "credit.collect",
  "customers.view",
  "customers.manage",
  "documents.quote",
  "documents.delivery",
  "documents.invoice",
]);

function backend() {
  mockCommands({
    auth_session: () => manager,
    packs_state: () => packState({ modules: ["instalments", "orders", "appointments"] }),
    instalments_get: () => plan({ paidDues: 2, partial: 300_000, overdueDays: 12 }),
    instalments_list: () => [plan({ paidDues: 1, overdueDays: 5 })],
    instalments_reminders: () => [],
    privacy_consents: () => [],
    orders_get: () => order({ held: 1_200_000, kind: "layaway" }),
    orders_list: () => [],
    deposits_list: () => [],
    appointments_day: () => appointments(),
    auth_list_users: () => [{ id: "u1", displayName: "Nadia" }],
  });
}

describe("the instalment and order screens in Arabic", () => {
  it("every text of the contracts, a contract, an order and the appointment book is translated", async () => {
    backend();
    missingKeys.clear();
    const contract = await renderM8(<PlanPage id="pl1" />, { lang: "ar", session: manager });
    expect(await screen.findByText("متأخر منذ 12 يومًا")).toBeInTheDocument();
    expect(document.documentElement.dir).toBe("rtl");
    contract.unmount();
    const list = await renderM8(<InstalmentsScreen />, { lang: "ar", session: manager, route: "/" });
    expect(await screen.findByRole("tab", { name: "العقود" })).toBeInTheDocument();
    list.unmount();
    const page = await renderM8(<OrderPage id="o1" />, { lang: "ar", session: manager });
    expect(await screen.findByRole("button", { name: "إقفال" })).toBeDisabled();
    expect(screen.getByText("أرجِع العربون أو احتفظ به قبل الإقفال.")).toBeInTheDocument();
    page.unmount();
    const orders = await renderM8(<CustomerOrdersScreen />, { lang: "ar", session: manager, route: "/" });
    expect(await screen.findByRole("tab", { name: "المواعيد" })).toBeInTheDocument();
    orders.unmount();
    await renderM8(<AppointmentBook day="2026-09-28" />, { lang: "ar", session: manager });
    expect(await screen.findByRole("button", { name: "Nadia 10:00 متاح" })).toBeInTheDocument();
    expect([...missingKeys]).toEqual([]);
  });
});
