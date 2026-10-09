import { type QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { mockIPC } from "@tauri-apps/api/mocks";
import type { PostProcessorModule } from "i18next";
import type { ReactElement, ReactNode } from "react";
import { MemoryRouter } from "react-router";
import { i18n } from "@/lib/i18n";
import { createQueryClient } from "@/lib/query";
import { scanDigits } from "./scan";

export type Handler = (args: Record<string, unknown>) => unknown;

/**
 * Every key a screen translated that its language lacks, as `<lang>:<namespace>:<key>`: found
 * only through the French fallback, or not at all (the key itself, or a `defaultValue`, shown).
 * A test clears it, renders, and expects it empty.
 */
export const missingKeys = new Set<string>();

/** What the translator tells a post-processor with `postProcessPassResolved`. */
interface Resolved {
  res?: unknown;
  usedKey?: string;
  usedLng?: string;
  usedNS?: string;
}

const missingKeysCollector: PostProcessorModule = {
  type: "postProcessor",
  name: "missingKeys",
  process(value, key, options, translator: { language?: string }) {
    const resolved = (options as { i18nResolved?: Resolved }).i18nResolved;
    const lang = options.lng ?? translator.language;
    if (resolved && (resolved.res === undefined || resolved.usedLng !== lang)) {
      const name = resolved.usedKey ?? (Array.isArray(key) ? key.join("|") : key);
      missingKeys.add(`${lang}:${resolved.usedNS ?? ""}:${name}`);
    }
    return value;
  },
};

// Runs after every translation of the test instance; the translator reads its options live.
i18n.use(missingKeysCollector);
i18n.options.postProcess = ["missingKeys"];
i18n.options.postProcessPassResolved = true;

/** The handlers {@link mockCommands} installed last, its call log and the IPC function it
 * installed (cleared mocks, or another `mockIPC`, replace that function). */
let installed: { handlers: Record<string, Handler>; calls: string[]; invoke: unknown } | null = null;

function stillInstalled(): typeof installed {
  const current = (window as { __TAURI_INTERNALS__?: { invoke?: unknown } }).__TAURI_INTERNALS__?.invoke;
  return installed && current !== undefined && installed.invoke === current ? installed : null;
}

/** The commands {@link mockCommands} answers right now (none once the mocks are cleared or
 * replaced by another `mockIPC`): a helper adds its own to them instead of dropping them. */
export function mockedCommands(): Record<string, Handler> {
  return stillInstalled()?.handlers ?? {};
}

/** The call log of the commands {@link mockCommands} answers right now: a helper that mocks more
 * commands keeps writing to the log the test already holds. */
export function mockedCalls(): string[] {
  return stillInstalled()?.calls ?? [];
}

/**
 * Mocks the given commands (others reject like an unknown backend error). Returns the call log
 * (`calls`, a new one by default). Tauri events are mocked too: `emit` from
 * `@tauri-apps/api/event` reaches the page's listeners.
 */
export function mockCommands(handlers: Record<string, Handler>, calls: string[] = []): string[] {
  mockIPC(
    (cmd, args) => {
      calls.push(cmd);
      const handler = handlers[cmd];
      if (!handler) return Promise.reject({ code: "unknown", message: `unmocked command ${cmd}` });
      return handler((args ?? {}) as Record<string, unknown>);
    },
    { shouldMockEvents: true },
  );
  installed = {
    handlers,
    calls,
    invoke: (window as { __TAURI_INTERNALS__?: { invoke?: unknown } }).__TAURI_INTERNALS__?.invoke,
  };
  return calls;
}

/** A Tauri event from the backend (`import-progress`, `print-failed`…), delivered to the page's
 * listeners by the mocked IPC of {@link mockCommands}; usable inside a command handler. */
export function emitEvent(name: string, payload: unknown): void {
  void emit(name, payload);
}

/** A `renderHook` wrapper with a fresh QueryClient (or the given one). */
export function queryWrapper(client: QueryClient = createQueryClient()) {
  return function QueryWrapper({ children }: { children: ReactNode }) {
    return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
  };
}

/** The providers of `renderWithProviders` as a `renderHook` wrapper, on `client` (a fresh one by
 * default). */
export function providers(client: QueryClient = createQueryClient()) {
  return function Providers({ children }: { children: ReactNode }) {
    return (
      <QueryClientProvider client={client}>
        <MemoryRouter>{children}</MemoryRouter>
      </QueryClientProvider>
    );
  };
}

/** `state` is the router state of the first entry (what `navigate(route, { state })` passes). */
export function renderWithProviders(
  ui: ReactElement,
  { route = "/", state }: { route?: string; state?: unknown } = {},
) {
  const client = createQueryClient();
  return {
    client,
    ...render(
      <QueryClientProvider client={client}>
        <MemoryRouter initialEntries={[state === undefined ? route : { pathname: route, state }]}>
          {ui}
        </MemoryRouter>
      </QueryClientProvider>,
    ),
  };
}

/** A barcode of digits read by the scanner anywhere in the window (what `useScanner` listens to):
 * the keys at scanner speed, then Enter. */
export function scan(code: string): void {
  scanDigits(document.activeElement ?? document.body, code);
}
