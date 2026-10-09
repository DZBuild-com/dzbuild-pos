import { useMutation } from "@tanstack/react-query";
import { renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { useActivateLicense, useLicenseStatus } from "@/lib/ipc/license";
import { createQueryClient } from "@/lib/query";
import { activeStatus, expiredStatus, unlicensedStatus } from "@/test/fixtures-m9";
import { mockCommands, queryWrapper } from "@/test/render";

describe("licence hooks", () => {
  it("reads the status and keeps what an activation answered", async () => {
    const keys: unknown[] = [];
    const calls = mockCommands({
      license_status: () => unlicensedStatus,
      license_activate: ({ key }) => {
        keys.push(key);
        return activeStatus;
      },
    });
    const client = createQueryClient();
    const status = renderHook(() => useLicenseStatus(), { wrapper: queryWrapper(client) });
    await waitFor(() => expect(status.result.current.data?.state).toBe("unlicensed"));
    const activate = renderHook(() => useActivateLicense(), { wrapper: queryWrapper(client) });
    await activate.result.current.mutateAsync("DZL1.abc.def");
    await waitFor(() => expect(status.result.current.data?.state).toBe("active"));
    expect(keys).toEqual(["DZL1.abc.def"]);
    expect(calls.filter((c) => c === "license_status")).toHaveLength(1);
  });

  it("reads the status again after any write refused for the licence", async () => {
    let current = activeStatus;
    const calls = mockCommands({ license_status: () => current });
    const client = createQueryClient();
    const status = renderHook(() => useLicenseStatus(), { wrapper: queryWrapper(client) });
    await waitFor(() => expect(status.result.current.data?.state).toBe("active"));
    current = expiredStatus; // the trial ended while the app ran
    const refused = renderHook(
      () => useMutation({ mutationFn: () => Promise.reject({ code: "license_expired" }) }),
      { wrapper: queryWrapper(client) },
    );
    await refused.result.current.mutateAsync().catch(() => undefined);
    await waitFor(() => expect(status.result.current.data?.state).toBe("expired"));
    expect(calls.filter((c) => c === "license_status")).toHaveLength(2);
  });
});
