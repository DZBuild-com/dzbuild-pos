import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { setLanguage } from "@/lib/i18n";
import { useErrorText } from "@/lib/use-error-text";

afterEach(async () => {
  await act(async () => {
    await setLanguage("fr");
  });
});

describe("useErrorText", () => {
  it("fills the message's placeholders from the error's details", async () => {
    const { result } = renderHook(() => useErrorText());
    const error = { code: "outbox_pending_at_close", details: { pending: 3 } };
    expect(result.current(error)).toContain(": 3.");
    await act(async () => {
      await setLanguage("ar");
    });
    const { result: arabic } = renderHook(() => useErrorText());
    expect(arabic.current(error)).toContain(": 3.");
  });

  it("translates the code alone when the details are not an object", () => {
    const { result } = renderHook(() => useErrorText());
    expect(result.current({ code: "offline_z_needs_approval", details: [1, 2] })).toBe(
      "Un Z hors ligne demande l'accord d'un gérant.",
    );
    expect(result.current({ code: "no_such_code" })).toBe(result.current({ code: "unknown" }));
  });
});
