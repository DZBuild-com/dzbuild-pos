import { expect, test } from "@playwright/test";
import {
  CASHIER,
  LANG_LABEL,
  OWNER,
  SEMOULE,
  failOnPageErrors,
  pageErrors,
  scanCode,
  scanText,
  signIn,
} from "./support";

test.beforeEach(({ page }) => failOnPageErrors(page));
test.afterEach(({ page }) => expect(pageErrors(page)).toEqual([]));

test("first start: language, shop, owner, then the shell", async ({ page }) => {
  await page.goto("/?e2e=fresh");
  await page.getByRole("button", { name: LANG_LABEL.fr }).click();
  await page.getByRole("button", { name: "Suivant" }).click();
  await page.getByLabel("Nom de la boutique").fill("Supérette El Amel");
  // M8: the supérette template is chosen by default (its card is pressed)
  await expect(page.getByRole("button", { name: "Supérette / épicerie", exact: true })).toHaveAttribute(
    "aria-pressed",
    "true",
  );
  await page.getByRole("button", { name: "Suivant" }).click();
  await page.getByLabel("Votre nom").fill("Amine");
  await page.getByLabel("Code PIN (4 à 8 chiffres)").fill("2580");
  await page.getByLabel("Retapez le code PIN").fill("2580");
  await page.getByRole("button", { name: "Suivant" }).click();
  // a supérette weighs: the printer step, then the scale step (ONB-009), both skipped
  await page.getByRole("button", { name: "Passer", exact: true }).click();
  await expect(page.getByRole("heading", { name: "Balance", exact: true })).toBeVisible();
  await page.getByRole("button", { name: "Passer", exact: true }).click();
  // The owner lands on the till, closed until a cash session opens; it leads to the back office.
  await expect(page.getByText("La caisse est fermée")).toBeVisible();
  await page.getByRole("button", { name: "Retour au menu" }).click();
  await expect(page.getByRole("navigation", { name: "Menu principal" })).toBeVisible();
  await expect(page.getByText("Supérette El Amel")).toBeVisible();
});

test("a new install without a licence: a refused key, a good one, then the setup wizard", async ({
  page,
}) => {
  await page.goto("/?e2e=unlicensed");
  await page.getByRole("button", { name: LANG_LABEL.fr }).click();
  await expect(page.getByTestId("device-code")).toBeVisible();
  await page.getByLabel("Clé de licence").fill("pas une clé");
  await page.getByRole("button", { name: "Activer la licence" }).click();
  await expect(page.getByText("n'est pas une clé de licence", { exact: false })).toBeVisible();
  await page.getByLabel("Clé de licence").fill("DZL1.e2e.key");
  await page.getByRole("button", { name: "Activer la licence" }).click();
  await expect(page.getByRole("heading", { name: "Installation de DZBuild POS" })).toBeVisible();
});

test("a scanned sale in three taps, a tile sale with F12, a return with a manager's PIN, then the Z", async ({
  page,
}) => {
  await page.goto("/?e2e=shop");
  await signIn(page, CASHIER.name, CASHIER.pin);
  await expect(page.getByRole("combobox", { name: /Scanner ou rechercher/ })).toBeFocused();
  const ticket = page.getByRole("list", { name: "Ticket" });
  const payment = page.getByRole("dialog", { name: "Encaissement" });
  const saleDone = page.getByRole("dialog", { name: "Vente encaissée" });

  const scanToLine = await scanCode(page, SEMOULE.code);
  console.log(`E2E scan to line: ${scanToLine.toFixed(1)} ms`);
  expect(scanToLine, "scan → line in the webview").toBeGreaterThan(0);
  expect(scanToLine, "scan → line in the webview").toBeLessThan(process.env.CI ? 150 : 50);
  await expect(ticket.getByText(SEMOULE.name)).toBeVisible();
  // UX-038: the scanner reads the items, then three taps: Encaisser, Montant exact, Valider.
  await page.getByRole("button", { name: /Encaisser/ }).click();
  await payment.getByRole("button", { name: "Montant exact" }).click();
  await payment.getByRole("button", { name: /Valider/ }).click();
  await expect(saleDone).toContainText("TK2026-C1-000001");
  await saleDone.getByRole("button", { name: /Nouvelle vente/ }).click();

  // A tile, F12, the cash typed on the keyboard and Enter.
  await page.getByRole("button", { name: /Eau minérale/ }).click();
  await expect(ticket.getByText(/Eau minérale/)).toBeVisible();
  await page.keyboard.press("F12");
  await payment.getByLabel("Espèces").fill("100");
  await expect(payment.getByRole("button", { name: /Valider/ })).toBeEnabled();
  await page.keyboard.press("Enter");
  await expect(saleDone).toContainText("TK2026-C1-000002");
  await expect(saleDone.getByRole("status", { name: "Rendu" })).toContainText("55,00");
  await saleDone.getByRole("button", { name: /Nouvelle vente/ }).click();

  // F8: the first ticket's barcode, one semolina back, the owner's PIN for the cashier's return.
  await page.keyboard.press("F8");
  const ret = page.getByRole("dialog", { name: "Retour de marchandise" });
  await expect(ret.getByLabel(/N° du ticket/)).toBeFocused();
  await scanText(page, "TK2026-C1-000001");
  await ret.getByLabel(`Quantité rendue (${SEMOULE.name})`).fill("1");
  await ret.getByRole("button", { name: "Continuer" }).click();
  const approval = page.getByRole("dialog", { name: "Accord d'un responsable" });
  await approval.getByRole("button", { name: new RegExp(OWNER.name) }).click();
  await expect(approval.getByTestId("pin-keys")).toBeVisible();
  await page.keyboard.type(OWNER.pin);
  await page.keyboard.press("Enter");
  await expect(approval).toBeHidden();
  await expect(ret.getByRole("status", { name: "À rembourser" })).toContainText("120,00");
  await ret.getByRole("button", { name: "Rembourser" }).click();
  const returnDone = page.getByRole("dialog", { name: "Retour enregistré" });
  await expect(returnDone).toContainText("BR2026-C1-000001");
  await returnDone.getByRole("button", { name: /Nouvelle vente/ }).click();

  // Esc opens the till's menu; the cash screen closes the session with the Z.
  await page.keyboard.press("Escape");
  await page
    .getByRole("dialog", { name: "Menu" })
    .getByRole("button", { name: "Caisse", exact: true })
    .click();
  await page.getByRole("button", { name: "Clôturer la caisse" }).click();
  const closing = page.getByRole("dialog", { name: "Clôturer la caisse" });
  await closing.getByLabel(/Billets de 1\s000 DA/).fill("5");
  await closing.getByRole("button", { name: "Clôturer et imprimer le Z" }).click();
  await expect(page.getByRole("dialog", { name: "Caisse clôturée" })).toContainText("ZC1-000001");
});

test("the whole UI switches between Arabic, French and English at once", async ({ page }) => {
  await page.goto("/?e2e=shop");
  await signIn(page, OWNER.name, OWNER.pin);
  // the owner lands on the till: Esc opens its menu, which leads to the back office
  await expect(page.getByRole("combobox", { name: /Scanner ou rechercher/ })).toBeFocused();
  await page.keyboard.press("Escape");
  await page.getByRole("button", { name: "Retour au back-office" }).click();
  await page.getByRole("radio", { name: LANG_LABEL.ar }).click();
  await expect(page.locator("html")).toHaveAttribute("dir", "rtl");
  await expect(page.getByRole("navigation", { name: "القائمة الرئيسية" })).toBeVisible();
  await page.getByRole("radio", { name: LANG_LABEL.en }).click();
  await expect(page.locator("html")).toHaveAttribute("dir", "ltr");
  await expect(page.getByRole("navigation", { name: "Main menu" })).toBeVisible();
});
