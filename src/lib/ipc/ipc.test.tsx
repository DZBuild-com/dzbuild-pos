import { QueryClientProvider, type QueryClient } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import type { ReactNode } from "react";
import { describe, expect, it } from "vitest";
import { useLogin } from "@/lib/ipc/auth";
import { errorKey, toAppError } from "@/lib/ipc/client";
import { qk } from "@/lib/ipc/keys";
import { useUsers } from "@/lib/ipc/users";
import { isTooSimplePin, pinErrors } from "@/lib/pin";
import { createQueryClient } from "@/lib/query";
import { applyTheme, nextTheme } from "@/lib/theme";
import { ownerSession } from "@/test/fixtures";
import { mockCommands } from "@/test/render";

function wrapper(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe("ipc layer", () => {
  it("keeps backend error payloads and wraps anything else", () => {
    expect(toAppError({ code: "invalid_pin" })).toEqual({ code: "invalid_pin" });
    expect(toAppError(new Error("boom"))).toEqual({ code: "internal_error", message: "boom" });
    expect(toAppError(42).code).toBe("unknown");
    expect(errorKey({ code: "nope" }, () => false)).toBe("errors:unknown");
    expect(errorKey({ code: "invalid_pin" }, () => true)).toBe("errors:invalid_pin");
  });

  it("stores the session after a login and sends the typed payload", async () => {
    const received: unknown[] = [];
    mockCommands({
      auth_login: (args) => {
        received.push(args);
        return ownerSession;
      },
    });
    const client = createQueryClient();
    const { result } = renderHook(() => useLogin(), { wrapper: wrapper(client) });
    result.current.mutate({ user: "u-owner", pin: "2580" });
    await waitFor(() => expect(client.getQueryData(qk.session)).toEqual(ownerSession));
    expect(received).toEqual([{ input: { user: "u-owner", pin: "2580" } }]);
  });

  it("sends everyone back to the login screen when the session ended", async () => {
    mockCommands({ users_list: () => Promise.reject({ code: "session_required" }) });
    const client = createQueryClient();
    client.setQueryData(qk.session, ownerSession);
    renderHook(() => useUsers(), { wrapper: wrapper(client) });
    await waitFor(() => expect(client.getQueryData(qk.session)).toBeNull());
  });

  it("applies the theme preference to <html>", () => {
    applyTheme("dark");
    expect(document.documentElement).toHaveAttribute("data-theme", "dark");
    applyTheme("system");
    expect(document.documentElement).not.toHaveAttribute("data-theme");
    expect([nextTheme("system"), nextTheme("light"), nextTheme("dark")]).toEqual(["light", "dark", "system"]);
  });

  it("mirrors the Rust PIN simplicity rule", () => {
    expect(["1111", "1234", "9876", "34567"].every(isTooSimplePin)).toBe(true);
    expect(["2580", "7391", "20262026"].some(isTooSimplePin)).toBe(false);
    expect(pinErrors("1234", "1243")).toEqual({
      pin: "validation:pinTooSimple",
      pinConfirm: "validation:pinMismatch",
    });
    expect(pinErrors("12a", "12a")).toEqual({ pin: "validation:pinFormat" });
    expect(pinErrors("2580", "2580")).toEqual({});
  });
});
