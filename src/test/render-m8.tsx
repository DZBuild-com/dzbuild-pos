import { mockIPC } from "@tauri-apps/api/mocks";
import type { ReactElement } from "react";
import { TillKitContext } from "@/features/pos/till-kit";
import { tillKit } from "@/features/pos/till-kit-parts";
import { type Lang, setLanguage } from "@/lib/i18n";
import type { Session } from "@/lib/ipc/types";
import { type Handler, mockCommands, mockedCalls, mockedCommands, renderWithProviders } from "./render";

interface RenderM8Options {
  lang?: Lang;
  route?: string;
  /** Answered by `auth_session`. */
  session?: Session;
  /** The other commands the screen calls (`mockCommands`'s handlers). */
  commands?: Record<string, Handler>;
}

type Invoke = (cmd: string, args?: unknown, options?: unknown) => Promise<unknown>;

/** The IPC mock a test installed before rendering, if any. */
function installedMock(): Invoke | undefined {
  return (window as unknown as { __TAURI_INTERNALS__?: { invoke?: Invoke } }).__TAURI_INTERNALS__?.invoke;
}

/** An M8 screen in `lang` (M0's `setLanguage`, so RTL and the locale bundles load), inside M1b's
 * providers and the till's parts (`TillKitContext`, as `AppRoutes` provides them); with `session`
 * or `commands`, the backend answers them besides what the test mocked before (they win over it).
 * After the test's own `mockCommands`, the call log that comes back as `calls` is the very log it
 * returned; after a raw IPC mock, the earlier mock answers what these do not, and `calls` logs from
 * the render on. */
export async function renderM8(
  ui: ReactElement,
  { lang = "fr", route = "/", session, commands = {} }: RenderM8Options = {},
) {
  await setLanguage(lang);
  const handlers: Record<string, Handler> =
    session === undefined ? commands : { auth_session: () => session, ...commands };
  const earlier = installedMock();
  let calls: string[] = [];
  if (Object.keys(handlers).length > 0) {
    const mocked = mockedCommands();
    if (Object.keys(mocked).length > 0) {
      calls = mockCommands({ ...mocked, ...handlers }, mockedCalls());
    } else if (earlier) {
      mockIPC(
        (cmd, args) => {
          calls.push(cmd);
          const handler = handlers[cmd];
          return handler ? handler((args ?? {}) as Record<string, unknown>) : earlier(cmd, args);
        },
        { shouldMockEvents: true },
      );
    } else {
      calls = mockCommands(handlers);
    }
  }
  return {
    calls,
    ...renderWithProviders(<TillKitContext.Provider value={tillKit}>{ui}</TillKitContext.Provider>, {
      route,
    }),
  };
}
