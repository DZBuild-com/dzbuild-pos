import "@testing-library/jest-dom/vitest";
import { cleanup, configure } from "@testing-library/react";
import { clearMocks } from "@tauri-apps/api/mocks";
import { randomFillSync } from "node:crypto";
import { toast } from "sonner";
import { afterEach, beforeAll, beforeEach } from "vitest";
import { i18n, loadBackOffice, setLanguage } from "@/lib/i18n";
import { usePrinterStatus } from "@/lib/printer-status";
import { setBurstGuard } from "@/lib/scanner";

// A full parallel run on a busy till PC (or CI, or while Rust compiles beside it) renders slowly:
// `findBy*`/`waitFor` wait up to 10 s instead of 1 s, so a screen that is merely slow is not
// reported as missing. The test budget (vite.config.ts `testTimeout`) stays well above it.
configure({ asyncUtilTimeout: 10_000 });

beforeAll(() => {
  // TanStack Virtual observes its scroll element; jsdom has no ResizeObserver.
  if (typeof window.ResizeObserver === "undefined") {
    class ResizeObserverStub {
      observe(): void {}
      unobserve(): void {}
      disconnect(): void {}
    }
    Object.defineProperty(window, "ResizeObserver", { value: ResizeObserverStub, configurable: true });
  }
  // Tauri's IPC mocks need WebCrypto; jsdom versions without it get Node's.
  if (typeof window.crypto?.getRandomValues !== "function") {
    Object.defineProperty(window, "crypto", {
      value: { getRandomValues: (buffer: Uint8Array) => randomFillSync(buffer) },
      configurable: true,
    });
  }
  // jsdom has no pointer capture; sonner captures the pointer when a toast is pressed.
  Element.prototype.setPointerCapture ??= () => {};
  Element.prototype.releasePointerCapture ??= () => {};
  Element.prototype.hasPointerCapture ??= () => false;
  // The tests run on a French Windows: the setup wizard starts in Windows' language when it is
  // Arabic or English (`features/setup/setup-lang.ts`).
  Object.defineProperty(window.navigator, "language", { value: "fr-FR", configurable: true });
});

// Every screen a test renders, the back office's included, has its texts (they are chunks read on
// demand in the app, `src/lib/i18n.ts`).
await loadBackOffice();
// A key found in no language (or shown from a `defaultValue`) fires `missingKey`: the screen
// smokes listen to it (`src/test/m8-smoke.test.tsx`). Nothing is saved anywhere (no backend).
i18n.options.saveMissing = true;

beforeEach(async () => {
  await setLanguage("fr");
  // Typing in tests is instant, which the numeric fields' burst guard would take for a scanner.
  setBurstGuard(false);
});

afterEach(() => {
  cleanup();
  clearMocks();
  // Toasts and the printer status live in module state: a toast still active would be replayed
  // by the next test's Toaster, with an action bound to an unmounted component.
  toast.dismiss();
  usePrinterStatus.getState().clear();
  document.documentElement.removeAttribute("data-theme");
  // This PC's display choices (`applyDisplay`) live on <html> too.
  document.documentElement.removeAttribute("data-light");
  document.documentElement.removeAttribute("data-osk");
  document.documentElement.style.removeProperty("font-size");
  // The wizard's language, remembered per PC.
  window.localStorage.removeItem("dzpos.setup.lang");
  window.localStorage.removeItem("dzpos.lang");
});
