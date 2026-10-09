/**
 * Loaded by `src/main.tsx` before the first render when the app runs for the E2E suite
 * (`VITE_E2E=1`): every command goes to the fake shop of `?e2e=<mode>` (`shop` by default), in the
 * language of `?lang=` (French by default), a restaurant, a pharmacy or a butcher with
 * `?activity=food_service|pharmacy|butcher`, and the specs may emit backend events.
 */
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import { isLang } from "@/lib/i18n";
import type { ShopActivity } from "@/lib/ipc/types";
import { E2E_MARKER, type FakeMode, createFakeBackend } from "./fake-backend";

const MODES: readonly FakeMode[] = ["fresh", "shop", "fatal", "unlicensed", "expired"];

const params = new URLSearchParams(window.location.search);
const asked = params.get("e2e");
const mode = MODES.find((m) => m === asked) ?? "shop";
const lang = params.get("lang");
// M8: `?activity=food_service` opens a restaurant (the floor's screenshots), `pharmacy` and
// `butcher` the shops of their packs
const ACTIVITIES: readonly ShopActivity[] = ["food_service", "pharmacy", "butcher"];
const activity = ACTIVITIES.find((a) => a === params.get("activity"));
const backend = createFakeBackend(mode, { lang: isLang(lang) ? lang : "fr", activity });

mockIPC(
  (cmd, args) => {
    try {
      return backend(cmd, (args ?? {}) as Record<string, unknown>);
    } catch (error) {
      // a screen calling a command the fake does not know fails the suite (`failOnPageErrors`)
      if (error instanceof Error) console.error(error.message);
      throw error;
    }
  },
  { shouldMockEvents: true },
);

window.__dzposEmit = (event, payload) => void emit(event, payload);
// The page says it runs on the fake backend (and the marker stays in any bundle that imports it).
document.documentElement.dataset.e2e = `${E2E_MARKER}:${mode}`;
