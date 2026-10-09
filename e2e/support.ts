import { type Locator, type Page, expect } from "@playwright/test";

export const LANGS = ["ar", "fr", "en"] as const;
export type Lang = (typeof LANGS)[number];

/** The reference PC's screen (spec §2) and the common laptop one. */
export const SIZES = [
  { width: 1024, height: 768 },
  { width: 1366, height: 768 },
] as const;

/** The language toggles show each language in its own script (spec §10.5). */
export const LANG_LABEL = { ar: "العربية", fr: "Français", en: "English" } as const;

/** The fake shop of `src/test/e2e/fake-backend.ts`. */
export const OWNER = { name: "Amine", pin: "2580" } as const;
export const CASHIER = { name: "Yacine", pin: "4826" } as const;
export const SEMOULE = { code: "6130000000017", name: "Semoule moyenne 1 kg" } as const;

const errors = new WeakMap<Page, string[]>();

/** Records the page's uncaught errors and every call the fake backend does not know
 * (`unmocked command …`); {@link pageErrors} lists them for the test's last assertion. */
export function failOnPageErrors(page: Page): void {
  const seen: string[] = [];
  errors.set(page, seen);
  page.on("pageerror", (error) => seen.push(`page error: ${error.message}`));
  page.on("console", (message) => {
    if (message.type() === "error" && /unmocked command/.test(message.text())) seen.push(message.text());
  });
}

export function pageErrors(page: Page): string[] {
  return errors.get(page) ?? [];
}

/** The words on the page (its texts and accessible names) a user should never read: the key of
 * a text missing in the language (`floor.title`, what i18next shows then) or a value as the code
 * holds it (`in_progress`, `food_service`). */
export function rawIdentifiers(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const texts: string[] = [];
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT);
    for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
      if (!node.parentElement?.closest("script, style")) texts.push(node.nodeValue ?? "");
    }
    for (const element of document.body.querySelectorAll("[aria-label], [title], [placeholder]")) {
      for (const name of ["aria-label", "title", "placeholder"]) texts.push(element.getAttribute(name) ?? "");
    }
    const key = /^[a-z][a-zA-Z0-9]*(?:\.[a-z][a-zA-Z0-9]*)+$/;
    // a site or a file the screen names is no key: dzbuild.com, pos.db
    const siteOrFile = /\.(?:com|dev|app|dz|db|pdf|xlsx|csv|json|zip|dzlic|dzpack)$/;
    const snake = /^[a-z][a-z0-9]*(?:_[a-z0-9]+)+$/;
    return texts
      .flatMap((text) => text.split(/[\s,;:()«»"'!?]+/))
      .map((word) => word.replace(/\.$/, ""))
      .filter((word) => (key.test(word) && !siteOrFile.test(word)) || snake.test(word));
  });
}

/** Taps the user's tile, types the PIN on the keyboard and presses Enter. */
export async function signIn(page: Page, name: string, pin: string): Promise<void> {
  await page.getByRole("button", { name }).click();
  const keys = page.getByTestId("pin-keys");
  await expect(keys).toBeVisible();
  await page.keyboard.type(pin);
  await page.keyboard.press("Enter");
  await expect(keys).toBeHidden();
}

/**
 * Sends `code` as a USB barcode scanner does: the physical keys (`Digit6`, `KeyT` with Shift,
 * `Minus`) 5 ms apart, then Enter, to the focused element. The keys are sent from inside the
 * page: Playwright's own keyboard takes 50 ms or more per key on a busy PC, slower than any
 * scanner, and the app (HW-021) would rightly read a person typing.
 */
export async function scanText(page: Page, code: string): Promise<void> {
  await page.evaluate(async (text) => {
    const keyOf = (ch: string) => {
      if (/[0-9]/.test(ch)) return { key: ch, code: `Digit${ch}`, shiftKey: false };
      if (ch === "-") return { key: ch, code: "Minus", shiftKey: false };
      if (/[a-zA-Z]/.test(ch))
        return { key: ch, code: `Key${ch.toUpperCase()}`, shiftKey: ch !== ch.toLowerCase() };
      throw new Error(`no scanner key for ${ch}`);
    };
    for (const init of [...[...text].map(keyOf), { key: "Enter", code: "Enter", shiftKey: false }]) {
      const target = document.activeElement ?? document.body;
      target.dispatchEvent(new KeyboardEvent("keydown", { ...init, bubbles: true, cancelable: true }));
      target.dispatchEvent(new KeyboardEvent("keyup", { ...init, bubbles: true, cancelable: true }));
      await new Promise((resolve) => setTimeout(resolve, 5));
    }
  }, code);
}

/** Scans `code` and returns the page's own scan → line measure (`dzpos:scan-start` →
 * `dzpos:line-added`), in ms; 0 when no line was added within a second. */
export async function scanCode(page: Page, code: string): Promise<number> {
  await page.evaluate(() => performance.clearMarks());
  await scanText(page, code);
  return page.evaluate(async () => {
    for (let i = 0; i < 50 && performance.getEntriesByName("dzpos:line-added").length === 0; i += 1) {
      await new Promise((resolve) => setTimeout(resolve, 20));
    }
    const start = performance.getEntriesByName("dzpos:scan-start")[0];
    const end = performance.getEntriesByName("dzpos:line-added")[0];
    return start && end ? end.startTime - start.startTime : 0;
  });
}

/** Moves the in-memory router (no reload: the fake shop lives in the page). */
export async function navigate(page: Page, to: string): Promise<void> {
  await page.waitForFunction(() => typeof window.__dzposNavigate === "function");
  await page.evaluate((path) => window.__dzposNavigate?.(path), to);
}

/** A key screen of the screenshot run. Later milestones add theirs here (their routes and the
 * fake backend's commands they call). */
export interface Screen {
  name: string;
  /** `fatal`: a database that cannot open; `fresh`: a new install (the setup wizard);
   * `unlicensed`: a new install without a licence (the activation screen); `expired`: the fake
   * shop in read-only mode, its trial ended; `shop`: the fake shop. In the last two the
   * owner signs in. */
  mode: "shop" | "fatal" | "fresh" | "unlicensed" | "expired";
  /** Where the owner goes after signing in; `null` stays on the login screen. */
  route: string | null;
  /** More of the fake shop's address (`activity=food_service`: a restaurant; `pharmacy`,
   * `butcher`: those packs' shops). */
  query?: string;
  /** What to do on the screen before the shot (open a dialog). */
  prepare?: (page: Page) => Promise<void>;
  /** Visible once the screen is up, in any language. */
  ready: (page: Page) => Locator;
}

const backOffice = (page: Page) => page.locator("#main h1").first();

export const SCREENS: readonly Screen[] = [
  {
    name: "login",
    mode: "shop",
    route: null,
    ready: (page) => page.getByRole("button", { name: OWNER.name }),
  },
  { name: "pos", mode: "shop", route: "/pos", ready: (page) => page.locator("#pos-scan") },
  {
    name: "payment",
    mode: "shop",
    route: "/pos",
    prepare: async (page) => {
      await expect(page.locator("#pos-scan")).toBeFocused();
      await scanText(page, SEMOULE.code);
      await expect(page.locator("#pos-scan")).toHaveValue("");
      await page.keyboard.press("F12");
    },
    ready: (page) => page.getByRole("dialog"),
  },
  { name: "cash", mode: "shop", route: "/cash", ready: backOffice },
  // M8: the cash page's flexy tab, and the repair and service board
  { name: "cash-flexy", mode: "shop", route: "/cash?tab=flexy", ready: backOffice },
  { name: "services", mode: "shop", route: "/services", ready: backOffice },
  // M8: the instalment contracts and the customer orders (`/orders` is the online store's)
  { name: "instalments", mode: "shop", route: "/instalments", ready: backOffice },
  { name: "customer-orders", mode: "shop", route: "/customer-orders", ready: backOffice },
  // M8: the delivery rounds of a wholesaler
  { name: "rounds", mode: "shop", route: "/rounds", ready: backOffice },
  // M7: the online orders of a linked DZBuild store (the fake shop is not linked)
  { name: "orders", mode: "shop", route: "/orders", ready: backOffice },
  // M5: the shop's network, on a single PC
  { name: "settings-network", mode: "shop", route: "/settings/network", ready: backOffice },
  { name: "settings-backups", mode: "shop", route: "/settings/backups", ready: backOffice },
  // M7: the optional DZBuild link, unlinked
  { name: "settings-dzbuild", mode: "shop", route: "/settings/dzbuild", ready: backOffice },
  { name: "settings-updates", mode: "shop", route: "/settings/updates", ready: backOffice },
  // M8: the activity template, the module switches and the till screen
  { name: "settings-activity", mode: "shop", route: "/settings/activity", ready: backOffice },
  // M8 Task 30: the payment types, the till's keys, the personal data and the regulatory packs
  { name: "settings-payments", mode: "shop", route: "/settings/payments", ready: backOffice },
  { name: "settings-keys", mode: "shop", route: "/settings/keys", ready: backOffice },
  { name: "settings-privacy", mode: "shop", route: "/settings/privacy", ready: backOffice },
  { name: "settings-regulatory", mode: "shop", route: "/settings/regulatory", ready: backOffice },
  // M9: this PC's licence
  { name: "settings-license", mode: "shop", route: "/settings/license", ready: backOffice },
  // M8 restaurant: the till opens on the floor; the rooms, stations, options and daily menu
  {
    name: "restaurant-floor",
    mode: "shop",
    query: "activity=food_service",
    route: "/pos/floor",
    ready: (page) => page.getByRole("tablist"),
  },
  // the table ticket: the scan field and the grid at the start side, the ticket at the end side
  {
    name: "restaurant-ticket",
    mode: "shop",
    query: "activity=food_service",
    route: "/pos/ticket/t1",
    ready: (page) => page.locator("#pos-scan"),
  },
  {
    name: "restaurant",
    mode: "shop",
    query: "activity=food_service",
    route: "/restaurant",
    ready: backOffice,
  },
  // M8 pharmacy: the expiry board above the lots, the Chifa list and the controlled register
  {
    name: "pharmacy-expiry",
    mode: "shop",
    query: "activity=pharmacy",
    route: "/stock?tab=expiry",
    ready: backOffice,
  },
  {
    name: "pharmacy-chifa",
    mode: "shop",
    query: "activity=pharmacy",
    route: "/products?tab=chifa",
    ready: backOffice,
  },
  {
    name: "pharmacy-register",
    mode: "shop",
    query: "activity=pharmacy",
    route: "/products?tab=register",
    ready: backOffice,
  },
  // M8 butcher: the cutting with its templates, and the day's waste
  {
    name: "butcher-yield",
    mode: "shop",
    query: "activity=butcher",
    route: "/stock?tab=yield",
    ready: backOffice,
  },
  {
    name: "butcher-waste",
    mode: "shop",
    query: "activity=butcher",
    route: "/stock?tab=waste",
    ready: backOffice,
  },
  // M8 catalogue modules of a general shop: the capped prices and the department keys
  { name: "products-regulated", mode: "shop", route: "/products?tab=regulated", ready: backOffice },
  { name: "products-departments", mode: "shop", route: "/products?tab=departments", ready: backOffice },
  { name: "help", mode: "shop", route: "/help/first-day", ready: backOffice },
  // M8 packs: the vouchers and the price per gram board
  { name: "loyalty", mode: "shop", route: "/loyalty", ready: backOffice },
  { name: "jewellery", mode: "shop", route: "/jewellery", ready: backOffice },
  // M8 Task 30: the staff list and the zakat statement
  { name: "staff", mode: "shop", route: "/staff", ready: backOffice },
  { name: "zakat", mode: "shop", route: "/zakat", ready: backOffice },
  // M9 read-only mode (a trial ended): the till's card instead of the sale screen, and the back
  // office with its banner (reports, exports and backups still work)
  { name: "read-only-pos", mode: "expired", route: "/pos", ready: (page) => page.locator("main h1") },
  {
    name: "read-only-dashboard",
    mode: "expired",
    route: "/dashboard",
    prepare: async (page) => {
      await expect(backOffice(page)).toBeVisible();
    },
    ready: (page) => page.locator('[role="status"] a[href="/settings/license"]'),
  },
  { name: "fatal", mode: "fatal", route: null, ready: (page) => page.locator("h1") },
  // M9: a new install without a licence opens on the activation screen
  { name: "activation", mode: "unlicensed", route: null, ready: (page) => page.getByTestId("device-code") },
  // Finding 22: the wizard's way on (start a new shop) is in view without scrolling at 1024x768.
  {
    name: "setup",
    mode: "fresh",
    route: null,
    prepare: async (page) => {
      await expect(page.locator('form button[type="submit"]')).toBeInViewport();
    },
    ready: (page) => page.locator('form button[type="submit"]'),
  },
];

/** Opens `screen` in `lang`: the fake shop's users read `lang`, and the fatal screen is switched
 * with its own language toggle. */
export async function openScreen(page: Page, screen: Screen, lang: Lang): Promise<void> {
  if (screen.mode === "fatal") {
    await page.goto("/?e2e=fatal");
    await page.getByRole("radio", { name: LANG_LABEL[lang] }).click();
  } else if (screen.mode === "fresh" || screen.mode === "unlicensed") {
    await page.goto(`/?e2e=${screen.mode}`);
    await page.getByRole("button", { name: LANG_LABEL[lang] }).click();
  } else {
    await page.goto(`/?e2e=${screen.mode}&lang=${lang}${screen.query ? `&${screen.query}` : ""}`);
    await expect(page.getByRole("radio", { name: LANG_LABEL[lang] })).toBeChecked();
    if (screen.route !== null) {
      await signIn(page, OWNER.name, OWNER.pin);
      if (screen.route !== "/pos") await navigate(page, screen.route);
    }
  }
  await screen.prepare?.(page);
  await expect(screen.ready(page)).toBeVisible();
  await expect(page.locator("html")).toHaveAttribute("lang", lang);
  await page.evaluate(() => document.fonts.ready.then(() => undefined));
}
