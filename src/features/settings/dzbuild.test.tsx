import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it } from "vitest";
import { DzbuildSettings } from "@/features/settings/DzbuildSettings";
import { setLanguage } from "@/lib/i18n";
import type { CloudNotice } from "@/lib/ipc/types";
import { cashierSession, ownerSession } from "@/test/fixtures";
import {
  chooseAttempt,
  failedItem,
  linkedStatus,
  notReadyNotice,
  publishRows,
  tillStatus,
  unlinkedStatus,
  waitingAttempt,
} from "@/test/fixtures-m7";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";

const settings: Record<string, unknown> = {
  "cloud.sync.enabled": true,
  "cloud.publish.mode": "selected",
  "cloud.images.push": true,
  "cloud.stock.push": true,
  "cloud.stock.pull": true,
  "cloud.prices.pull": false,
  "cloud.sales.push": true,
  "cloud.orders.intake": true,
  "cloud.orders.document": "ticket",
  "cloud.orders.retention_days": 90,
  "cloud.customers.sync": false,
  "cloud.customers.flags": true,
  "cloud.receipt_qr": false,
};

const base = {
  settings_get: ({ key }: Record<string, unknown>) => settings[String(key)],
  settings_set: ({ value }: Record<string, unknown>) => value,
  cloud_notices: () => [],
  cloud_outbox_failed: () => [],
  cloud_publish_list: () => publishRows,
};

describe("Settings › DZBuild", () => {
  it("unlinked: explains the link, never requires it, and nudges to create an account", async () => {
    const opened: unknown[] = [];
    mockCommands({
      ...base,
      cloud_status: () => unlinkedStatus,
      cloud_open_page: (a) => void opened.push(a.page),
    });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByRole("heading", { name: "Liaison DZBuild" })).toBeInTheDocument();
    expect(screen.getByText(/facultative/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Créer un compte DZBuild" }));
    expect(opened).toEqual(["signup"]);
  });

  it("links in the browser, then asks which store when there are several", async () => {
    let attempt = waitingAttempt;
    const chosen: unknown[] = [];
    mockCommands({
      ...base,
      cloud_status: () => unlinkedStatus,
      cloud_link_start: () => ({ attempt: waitingAttempt, qr: null }),
      cloud_link_poll: () => attempt,
      cloud_link_choose: (a) => {
        chosen.push(a.storeId);
        return {};
      },
    });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Lier la boutique" }));
    expect(await screen.findByText("Terminez la connexion dans votre navigateur.")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Copier le lien" })).toBeInTheDocument();
    attempt = chooseAttempt;
    await user.click(await screen.findByRole("radio", { name: /Amel Cosmétiques/ }, { timeout: 3000 }));
    await user.click(screen.getByRole("button", { name: "Lier cette boutique" }));
    expect(chosen).toEqual(["152"]);
  });

  it("the phone flow shows the code and the QR", async () => {
    const phoneAttempt = {
      ...waitingAttempt,
      kind: "device" as const,
      authorizeUrl: null,
      device: {
        userCode: "DZKM-4Q7P",
        verificationUri: "https://dzbuild.com/device",
        verificationUriComplete: null,
        expiresAt: 0,
      },
    };
    mockCommands({
      ...base,
      cloud_status: () => unlinkedStatus,
      cloud_device_flow_start: () => ({ attempt: phoneAttempt, qr: { size: 21, modules: "1".repeat(441) } }),
      cloud_link_poll: () => phoneAttempt,
    });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Lier avec un téléphone" }));
    expect(await screen.findByText("DZKM-4Q7P")).toBeInTheDocument();
    expect(screen.getByRole("img", { name: "Code QR de la liaison" })).toBeInTheDocument();
  });

  it("linked: per-entity status, a not-ready notice, a failed item with its reason and a retry", async () => {
    const retried: unknown[] = [];
    mockCommands({
      ...base,
      cloud_status: () => linkedStatus,
      cloud_notices: () => [notReadyNotice],
      cloud_outbox_failed: () => [failedItem],
      cloud_outbox_retry: (a) => void retried.push(a.id),
    });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByText("Boutique Amel")).toBeInTheDocument();
    expect(screen.getByText("Ventes du magasin")).toBeInTheDocument();
    expect(screen.getByText(/Clôtures de caisse : pas encore disponible chez DZBuild/)).toBeInTheDocument();
    expect(screen.getByText("Sac cuir")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Réessayer" }));
    expect(retried).toEqual(["f1"]);
  });

  it("a status event refreshes the page without a reload", async () => {
    let status = unlinkedStatus;
    mockCommands({ ...base, cloud_status: () => status });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    await screen.findByRole("button", { name: "Lier la boutique" });
    status = linkedStatus;
    await act(() => emit("sync-status-changed", { status: linkedStatus }));
    expect(await screen.findByText("Boutique Amel")).toBeInTheDocument();
  });

  it("customer sync carries the personal-data note and is off by default", async () => {
    mockCommands({ ...base, cloud_status: () => linkedStatus });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    const sync = await screen.findByRole("checkbox", { name: "Envoyer les clients à DZBuild" });
    expect(sync).not.toBeChecked();
    expect(screen.getByText(/loi 18-07/)).toBeInTheDocument();
  });

  it("a cashier sees the status but not the owner's actions", async () => {
    mockCommands({ ...base, cloud_status: () => unlinkedStatus });
    renderWithProviders(<DzbuildSettings session={cashierSession} />);
    expect(await screen.findByRole("button", { name: "Lier la boutique" })).toBeDisabled();
  });

  it("speaks Arabic right to left", async () => {
    await setLanguage("ar");
    mockCommands({ ...base, cloud_status: () => unlinkedStatus });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByRole("heading", { name: "الربط مع DZBuild" })).toBeInTheDocument();
    await waitFor(() => expect(document.documentElement.dir).toBe("rtl"));
  });

  it("on a till the link is the main PC's: the screen names it and offers no action", async () => {
    mockCommands({ ...base, cloud_status: () => tillStatus });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByText(/Supérette El Amel/)).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Liaison DZBuild" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Lier la boutique" })).not.toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Créer un compte DZBuild" })).not.toBeInTheDocument();
  });

  it("every notice the engine raises reads as a sentence", async () => {
    mockCommands({ ...base, cloud_status: () => linkedStatus, cloud_notices: () => engineNotices });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByText(/La commande 1042 a été vendue ou expédiée ici/)).toBeInTheDocument();
    expect(screen.getByText("Cette caisse a été retirée depuis DZBuild.")).toBeInTheDocument();
    expect(
      screen.getByText("Produits : 2 envoi(s) refusé(s) par DZBuild, voir ci-dessous."),
    ).toBeInTheDocument();
    expect(screen.getByText(/Le retour BR2026-000003 concerne une commande en ligne/)).toBeInTheDocument();
    expect(screen.getByText(/\(BL2026-000004, BL2026-000005\)/)).toBeInTheDocument();
    expect(screen.getByText(/restent en brouillon \(7\)/)).toBeInTheDocument();
  });

  it("an item DZBuild sent that the till refused is listed with what it is and why", async () => {
    mockCommands({ ...base, cloud_status: () => linkedStatus, cloud_notices: () => engineNotices });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(
      await screen.findByText(
        "« Sac cuir » a été modifié sur DZBuild mais la caisse ne l'a pas repris : corrigez-le sur DZBuild. Motif : Ce texte est trop long. Raccourcissez-le.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "La commande 1044 reçue de DZBuild n'a pas pu être enregistrée à la caisse : vérifiez-la sur DZBuild. Motif : Cette modification se heurte à des données déjà enregistrées (par exemple un code déjà utilisé). Vérifiez les valeurs, puis réessayez.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "DZBuild a envoyé une modification de stock que la caisse n'a pas pu reprendre. Motif : Cet emplacement de stock n'existe pas ou a été archivé.",
      ),
    ).toBeInTheDocument();
    // a product's detail refused: its event named from the details, never its raw type
    expect(
      screen.getByText(
        "DZBuild a envoyé une suppression de produit que la caisse n'a pas pu reprendre. Motif : Ce texte est trop long. Raccourcissez-le.",
      ),
    ).toBeInTheDocument();
  });

  it("names the product, the endpoint or the event every notice is about, never a raw key", async () => {
    mockCommands({ ...base, cloud_status: () => linkedStatus, cloud_notices: () => engineNotices });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(
      await screen.findByText("« Ceinture » a été supprimé de la boutique en ligne : il n'est plus publié."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Le prix de « Foulard » a été changé en ligne ; votre boutique affiche ses prix HT/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Le prix de « Bracelet » a été changé sur DZBuild : la caisse garde le sien/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/sa clé d'accès n'a pas pu être effacée du Gestionnaire d'identification de Windows/),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Ventes du magasin : votre formule DZBuild ne le permet pas. Rien n'est perdu ; la caisse réessaie chaque jour.",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "Votre formule DZBuild ne permet pas à la caisse de reprendre une nouvelle commande : changez de formule sur DZBuild.",
      ),
    ).toBeInTheDocument();
    for (const raw of ["{{", "inventory.level_changed", "product.deleted", "order.created", "pos.sales"]) {
      expect(document.body.textContent).not.toContain(raw);
    }
  });

  it("every endpoint DZBuild may not have shipped yet is named in each language", async () => {
    // pos-core `Endpoint::name` of every API call (the OAuth calls and the signed uploads aside)
    const endpoints = [
      "me",
      "store",
      "devices",
      "devices.heartbeat",
      "devices.delete",
      "oauth.device",
      "locations",
      "products.batch",
      "products",
      "products.get",
      "products.images",
      "media.uploads",
      "inventory.adjustments",
      "inventory.levels",
      "pos.sales",
      "pos.refunds",
      "pos.closures",
      "events",
      "orders",
      "orders.get",
      "orders.claim",
      "orders.update",
      "orders.cancel",
      "customers",
    ];
    const dismiss = { fr: "Compris", ar: "فهمت", en: "Got it" } as const;
    const notices = endpoints.map((subject, i) => ({ ...notReadyNotice, id: `nr${i}`, subject }));
    for (const lang of ["fr", "ar", "en"] as const) {
      await setLanguage(lang);
      mockCommands({ ...base, cloud_status: () => linkedStatus, cloud_notices: () => notices });
      const { unmount } = renderWithProviders(<DzbuildSettings session={ownerSession} />);
      const buttons = await screen.findAllByRole("button", { name: dismiss[lang] }, { timeout: 10_000 });
      expect(buttons).toHaveLength(endpoints.length);
      // a notice is "<endpoint name> : …": the raw identifier never starts one
      for (const button of buttons) {
        const sentence = button.parentElement?.querySelector("p")?.textContent ?? "";
        for (const endpoint of endpoints) expect(sentence, lang).not.toMatch(new RegExp(`^${endpoint}\\s?:`));
      }
      unmount();
    }
  });

  it("a link waiting for a new sign-in is linked again to the same store, its uploads kept", async () => {
    mockCommands({
      ...base,
      cloud_status: () => ({ ...linkedStatus, linkStatus: "relink_required" as const }),
      cloud_link_start: () => ({ attempt: waitingAttempt, qr: null }),
      cloud_link_poll: () => waitingAttempt,
    });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(await screen.findByRole("heading", { name: "Lier à nouveau" })).toBeInTheDocument();
    expect(screen.getByText(/Liez à nouveau la même boutique ci-dessous/)).toBeInTheDocument();
    expect(screen.queryByText(/Déliez/)).not.toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Lier la boutique" }));
    expect(await screen.findByText("Terminez la connexion dans votre navigateur.")).toBeInTheDocument();
  });

  it("unlinking says the waiting uploads are dropped, with their count", async () => {
    mockCommands({ ...base, cloud_status: () => ({ ...linkedStatus, pending: 3, failed: 1 }) });
    const user = userEvent.setup();
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    await user.click(await screen.findByRole("button", { name: "Délier" }));
    expect(
      await screen.findByText(/les envois en attente \(4\) sont abandonnés : ils ne partiront pas/),
    ).toBeInTheDocument();
  });

  it("in 'all active products' mode the publish list says unticking leaves a product out", async () => {
    mockCommands({
      ...base,
      settings_get: ({ key }) => (key === "cloud.publish.mode" ? "all" : settings[String(key)]),
      cloud_status: () => linkedStatus,
    });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    expect(
      await screen.findByText("Tous les produits actifs sont publiés : décochez un produit pour l'exclure."),
    ).toBeInTheDocument();
  });

  it("prices changed online are taken only when the owner turns it on", async () => {
    mockCommands({ ...base, cloud_status: () => linkedStatus });
    renderWithProviders(<DzbuildSettings session={ownerSession} />);
    const prices = await screen.findByRole("checkbox", { name: "Reprendre les prix changés en ligne" });
    await waitFor(() => expect(prices).toBeEnabled());
    expect(prices).not.toBeChecked();
    expect(screen.getByText("Désactivé : la caisse garde ses prix et vous prévient.")).toBeInTheDocument();
  });

  for (const [lang, dismiss] of [
    ["ar", "فهمت"],
    ["en", "Got it"],
  ] as const) {
    it(`says every notice in ${lang} without a missing word`, async () => {
      await setLanguage(lang);
      mockCommands({ ...base, cloud_status: () => linkedStatus, cloud_notices: () => engineNotices });
      missingKeys.clear();
      renderWithProviders(<DzbuildSettings session={ownerSession} />);
      // the whole linked page (status, notices, failed items, the switches, the publish list)
      expect(await screen.findAllByRole("button", { name: dismiss }, { timeout: 10_000 })).toHaveLength(
        engineNotices.length,
      );
      expect([...missingKeys]).toEqual([]);
    });
  }
});

function notice(
  id: string,
  kind: CloudNotice["kind"],
  code: string,
  subject: string,
  details: CloudNotice["details"] = {},
): CloudNotice {
  return { ...notReadyNotice, id, kind, code, subject, details };
}

/** One notice of each code the engine raises (pos-core `raise_notice`), with its real subject and
 * details. */
const engineNotices: CloudNotice[] = [
  notice("n2", "order", "online_order_fulfilled_claimed_elsewhere", "1042"),
  notice("n3", "revoked", "cloud_relink_required", "", { storeName: "Boutique Amel" }),
  notice("n4", "failed_items", "cloud_request_invalid", "product_upsert", { count: 2 }),
  notice("n5", "order", "online_order_returned_in_shop", "doc-1", { document: "BR2026-000003" }),
  notice("n6", "failed_items", "cloud_refund_unmatched", "AV2026-000001", {
    document: "AV2026-000001",
    deliveries: ["BL2026-000004", "BL2026-000005"],
  }),
  notice("n7", "stock", "stock_double_applied", "pos.sales"),
  notice("n8", "failed_items", "cloud_event_unreadable", "events", { type: "order.updated" }),
  notice("n9", "plan_cap", "cloud_plan_cap", "", { cap: 50, drafted: 7 }),
  notice("n12", "order", "online_order_changed", "1043"),
  notice("n13", "conflict", "cloud_conflict_online_wins", "p1", { fields: ["name"], product: "Sac cuir" }),
  // what the events feed and the dated pulls refuse here (pos-core `pull::Refusal`)
  notice("n14", "failed_items", "cloud_product_refused", "Sac cuir", { reason: "too_long" }),
  notice("n15", "failed_items", "cloud_order_refused", "1044", { reason: "constraint_violation" }),
  notice("n16", "failed_items", "cloud_event_refused", "inventory.level_changed", {
    reason: "stock_location_not_found",
  }),
  notice("n17", "failed_items", "cloud_event_refused", "rp_9", {
    reason: "too_long",
    type: "product.deleted",
  }),
  // the product's name in the details (pos-core `pull`)
  notice("n18", "conflict", "online_product_deleted", "p2", { product: "Ceinture" }),
  notice("n19", "conflict", "online_price_ht_shop", "p3", { price: 150_000, product: "Foulard" }),
  notice("n20", "conflict", "online_price_changed", "p4", { price: 150_000, product: "Bracelet" }),
  notice("n21", "relink", "cloud_credential_kept", ""),
  // the plan refused a whole kind (push) or one event's detail (pull): nothing drafted
  notice("n22", "plan_cap", "cloud_plan_cap", "pos.sales", { code: "plan_limit" }),
  notice("n23", "plan_cap", "cloud_plan_cap", "", { code: "plan_limit", type: "order.created" }),
];
