import { commands as tillCommands, events } from "@/bindings";
import type { packCommands } from "@/bindings-packs";

type Commands = typeof tillCommands & typeof packCommands;
type AnyCommand = (...args: unknown[]) => Promise<unknown>;

/** The vertical packs' commands are a module of their own (`src/bindings-packs.ts`, out of
 * the till's first chunk), read on the first call of one of them. */
let packs: Promise<Record<string, AnyCommand>> | null = null;
const loadPacks = () => {
  packs ??= import("@/bindings-packs").then((m) => m.packCommands as unknown as Record<string, AnyCommand>);
  return packs;
};

/** Every command (spec §4.4): the till's own at once, a pack's from its module on first use. */
export const commands = new Proxy(tillCommands, {
  get(target, name, receiver) {
    if (typeof name !== "string" || name in target) return Reflect.get(target, name, receiver);
    return (...args: unknown[]) => loadPacks().then((all) => all[name]!(...args));
  },
}) as Commands;

export { events };

/** Every backend error (spec §4.4). */
export interface AppErrorPayload {
  code: string;
  message?: string | null;
  field?: string | null;
  details?: unknown;
}

export function isAppError(value: unknown): value is AppErrorPayload {
  return (
    typeof value === "object" && value !== null && typeof (value as { code?: unknown }).code === "string"
  );
}

export function toAppError(error: unknown): AppErrorPayload {
  if (isAppError(error)) return error;
  if (error instanceof Error) return { code: "internal_error", message: error.message };
  return { code: "unknown", message: typeof error === "string" ? error : undefined };
}

/** `errors:<code>` when it is translated, else `errors:unknown`. */
export function errorKey(error: unknown, exists: (key: string) => boolean): string {
  const key = `errors:${toAppError(error).code}`;
  return exists(key) ? key : "errors:unknown";
}
