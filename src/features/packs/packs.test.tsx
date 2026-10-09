import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { Toaster } from "sonner";
import { afterEach, describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { cashierSession, ownerSession } from "@/test/fixtures";
import { packState, sessionWith, templateInfos } from "@/test/fixtures-m8";
import { ActivitySettings } from "@/features/settings/ActivitySettings";
import { NavRail } from "@/app/NavRail";
import { AppRoutes } from "@/app/routes";
import type { Module } from "@/lib/ipc/types";
import { ALL_MODULES, MODULE_GROUPS, withModule } from "./modules";
import { PackGate } from "./PackGate";

afterEach(() => clearMocks());

describe("pack framework", () => {
  it("groups each of the 21 modules once and keeps pos-core's order", () => {
    expect(ALL_MODULES).toHaveLength(21);
    expect(MODULE_GROUPS.flatMap((g) => g.modules).sort()).toEqual([...ALL_MODULES].sort());
    expect(withModule(["staff", "department_keys"], "loyalty", true)).toEqual([
      "department_keys",
      "loyalty",
      "staff",
    ]);
    expect(withModule(["department_keys", "loyalty"], "loyalty", false)).toEqual(["department_keys"]);
  });

  it("routes a pack screen through its gate, and keeps it from a user without its permissions", async () => {
    let modules: Module[] = [];
    mockCommands({ auth_session: () => ownerSession, packs_state: () => packState({ modules }) });
    const { unmount } = await renderM8(<AppRoutes session={ownerSession} />, { route: "/zakat" });
    expect(await screen.findByText("Ce module est désactivé.")).toBeInTheDocument();
    expect(await screen.findByRole("link", { name: "Ouvrir Activité et modules" })).toHaveAttribute(
      "href",
      "/settings/activity",
    );
    unmount();
    modules = ["zakat"];
    const owner = await renderM8(<AppRoutes session={ownerSession} />, { route: "/zakat" });
    // the zakat screen behind its gate (its route goes through `pack`)
    expect(await screen.findByRole("heading", { level: 1, name: "Zakat" })).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Zakat" })).toBeInTheDocument();
    owner.unmount();
    // a cashier (no reports.profit) never reaches the zakat screen, module on or not
    const cashier = await renderM8(<AppRoutes session={cashierSession} />, { route: "/zakat" });
    const rail = await screen.findByRole("navigation", { name: "Menu principal" });
    expect(await within(rail).findByRole("link", { name: "Tickets" })).toBeInTheDocument();
    expect(screen.queryByRole("heading", { level: 1, name: "Zakat" })).toBeNull();
    expect(screen.queryByText("Ce module est désactivé.")).toBeNull();
    expect(within(rail).queryByRole("link", { name: "Zakat" })).toBeNull();
    // the shell listens to backend events: it goes before this file's `clearMocks`
    cashier.unmount();
  });

  it("shows a pack entry only when its module is on and the user may open it", async () => {
    mockCommands({ packs_state: () => packState({ modules: ["restaurant", "staff"] }) });
    await renderM8(<NavRail session={sessionWith(["sales.create", "settings.company"])} />);
    await waitFor(() => expect(screen.getByRole("link", { name: "Restaurant" })).toBeTruthy());
    expect(screen.queryByRole("link", { name: "Bijouterie" })).toBeNull();
    expect(screen.queryByRole("link", { name: "Personnel" })).toBeNull();
  });

  it("switches a module and changes the till mode", async () => {
    const sent: unknown[] = [];
    const calls = mockCommands({
      auth_session: () => sessionWith(["settings.company"]),
      packs_state: () => packState({ modules: ["department_keys"] }),
      packs_templates: () => templateInfos(),
      packs_set_modules: (args) => {
        sent.push(args);
        return packState({ modules: ["department_keys", "loyalty"] });
      },
      packs_set_till_mode: () => packState({ modules: ["department_keys"], tillMode: "restaurant" }),
    });
    await renderM8(<ActivitySettings />);
    await userEvent.click(await screen.findByRole("switch", { name: "Fidélité (points)" }));
    await waitFor(() => expect(sent).toEqual([{ modules: ["department_keys", "loyalty"] }]));
    await userEvent.click(screen.getByRole("radio", { name: "Restaurant" }));
    await waitFor(() => expect(calls).toContain("packs_set_till_mode"));
  });

  it("explains a switched-off module in Arabic, right to left", async () => {
    mockCommands({
      auth_session: () => sessionWith(["settings.company"], "ar"),
      packs_state: () => packState({ modules: [] }),
    });
    await renderM8(
      <PackGate module="jewellery">
        <p>hidden</p>
      </PackGate>,
      { lang: "ar" },
    );
    expect(await screen.findByText("هذه الوحدة غير مفعّلة.")).toBeTruthy();
    expect(screen.queryByText("hidden")).toBeNull();
    expect(document.documentElement.dir).toBe("rtl");
  });

  it("opens the screen of a module that is on, and offers no settings link without settings.company", async () => {
    mockCommands({
      auth_session: () => sessionWith(["sales.create"]),
      packs_state: () => packState({ modules: ["orders"] }),
    });
    const { unmount } = await renderM8(
      <PackGate module={["orders", "appointments"]}>
        <p>orders screen</p>
      </PackGate>,
    );
    expect(await screen.findByText("orders screen")).toBeInTheDocument();
    unmount();
    await renderM8(
      <PackGate module="zakat">
        <p>zakat screen</p>
      </PackGate>,
    );
    expect(await screen.findByText("Ce module est désactivé.")).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Ouvrir Activité et modules" })).toBeNull();
  });

  it("changes the activity: the new template's modules and seeds, then packs_apply_template", async () => {
    const applied: unknown[] = [];
    mockCommands({
      auth_session: () => sessionWith(["settings.company"]),
      packs_state: () =>
        packState({
          modules: ["department_keys", "regulated_prices"],
          templateModules: ["department_keys", "regulated_prices"],
        }),
      packs_templates: () => templateInfos(),
      packs_apply_template: (args) => {
        applied.push(args);
        return { activity: "butcher", categories: 1, services: 1, extra: 0 };
      },
    });
    const user = userEvent.setup();
    await renderM8(<ActivitySettings />);
    expect(await screen.findByText("Supérette / épicerie")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Changer d'activité…" }));
    const dialog = await screen.findByRole("dialog");
    await user.click(within(dialog).getByRole("button", { name: "Boucherie" }));
    const chosen = within(dialog).getByRole("region", { name: "Boucherie" });
    expect(within(chosen).getByText("Viande rouge")).toBeInTheDocument();
    expect(within(chosen).getByText("Découpe et préparation")).toBeInTheDocument();
    expect(within(chosen).getByText("Rendement de découpe")).toBeInTheDocument();
    const seed = within(dialog).getByRole("checkbox", { name: "Créer les catégories et services du modèle" });
    expect(seed).toBeChecked();
    await user.click(seed);
    await user.click(within(dialog).getByRole("button", { name: "Appliquer le modèle" }));
    await waitFor(() => expect(applied).toEqual([{ activity: "butcher", seed: false }]));
    await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  });

  it("marks a hand-picked list and goes back to the template's modules", async () => {
    const calls = mockCommands({
      auth_session: () => sessionWith(["settings.company"]),
      packs_state: () =>
        packState({
          modules: ["department_keys", "loyalty"],
          templateModules: ["department_keys"],
          customised: true,
        }),
      packs_templates: () => templateInfos(),
      packs_reset_modules: () =>
        packState({ modules: ["department_keys"], templateModules: ["department_keys"] }),
    });
    const user = userEvent.setup();
    await renderM8(
      <>
        <ActivitySettings />
        <Toaster />
      </>,
    );
    expect(await screen.findByText("Personnalisé")).toBeInTheDocument();
    expect(screen.getByRole("switch", { name: "Fidélité (points)" })).toBeChecked();
    expect(screen.getByRole("switch", { name: "Fidélité (points)" })).toHaveAccessibleDescription(
      "Points par dinar, chèques fidélité.",
    );
    await user.click(screen.getByRole("button", { name: "Revenir aux modules du modèle" }));
    await waitFor(() => expect(calls).toContain("packs_reset_modules"));
    expect(await screen.findByText("Enregistré")).toBeInTheDocument();
  });
});
