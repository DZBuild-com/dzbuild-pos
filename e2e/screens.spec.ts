import { expect, test } from "@playwright/test";
import { LANGS, SCREENS, SIZES, failOnPageErrors, openScreen, pageErrors, rawIdentifiers } from "./support";

// One test per screen, language and size: each opens its screen in a context of
// its own, so the trace a failure keeps stays small. One context walking every screen (22 once the
// M8 packs and the licence added theirs) made Playwright's trace take longer to close than the
// test timeout, although every screen passed.
for (const lang of LANGS) {
  for (const size of SIZES) {
    test.describe(`${lang} at ${size.width}×${size.height}`, () => {
      for (const screen of SCREENS) {
        test(`${screen.name}: no horizontal scroll, no raw key`, async ({ page }) => {
          failOnPageErrors(page);
          await page.setViewportSize(size);
          await openScreen(page, screen, lang);
          const overflow = await page.evaluate(
            () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
          );
          expect.soft(overflow, `${lang} ${screen.name} scrolls sideways`).toBeLessThanOrEqual(0);
          // UX-001: every text in the language, none shown as its key or its code
          expect.soft(await rawIdentifiers(page), `${lang} ${screen.name} shows raw identifiers`).toEqual([]);
          await page.screenshot({
            path: `e2e-artifacts/screenshots/${lang}-${size.width}x${size.height}-${screen.name}.png`,
            fullPage: false,
          });
          expect(pageErrors(page)).toEqual([]);
        });
      }
    });
  }
}
