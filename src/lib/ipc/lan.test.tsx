import { emit } from "@tauri-apps/api/event";
import { act, renderHook, waitFor } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { lanHostStatusFixture, lanStandaloneFixture, lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, providers } from "@/test/render";
import { useBecomeHost, useJoin, useLanStatus } from "./lan";

describe("lan ipc", () => {
  it("reads the status and refreshes it after a role change", async () => {
    let current: unknown = lanStandaloneFixture;
    mockCommands({
      lan_status: () => current,
      lan_become_host: () => {
        current = lanHostStatusFixture;
        return lanHostStatusFixture;
      },
    });
    const { result } = renderHook(() => ({ status: useLanStatus(), become: useBecomeHost() }), {
      wrapper: providers(),
    });
    await waitFor(() => expect(result.current.status.data?.role).toBe("standalone"));
    await act(() => result.current.become.mutateAsync());
    await waitFor(() => expect(result.current.status.data?.role).toBe("host"));
  });

  it("takes the pushed status without asking again", async () => {
    const calls = mockCommands({ lan_status: () => lanTillStatusFixture });
    const { result } = renderHook(() => useLanStatus(), { wrapper: providers() });
    await waitFor(() => expect(result.current.data?.client?.link).toBe("online"));
    const offline = {
      ...lanTillStatusFixture,
      client: { ...lanTillStatusFixture.client, link: "offline", pending: 3 },
    };
    await act(() => emit("lan-status-changed", { status: offline }));
    await waitFor(() => expect(result.current.data?.client?.pending).toBe(3));
    expect(calls.filter((c) => c === "lan_status")).toHaveLength(1);
  });

  it("sends the owner's PIN with a join from Settings and none from the setup wizard", async () => {
    const joined: unknown[] = [];
    mockCommands({
      lan_join: ({ input }) => {
        joined.push(input);
        return { hostName: "Supérette El Amel", terminalCode: "C2" };
      },
    });
    const { result } = renderHook(() => useJoin(), { wrapper: providers() });
    const draft = {
      address: "192.168.1.10:7345",
      code: "042917",
      terminalCode: "C2",
      terminalName: "Caisse C2",
    };
    await act(() => result.current.mutateAsync(draft));
    await act(() => result.current.mutateAsync({ ...draft, ownerPin: "2580" }));
    expect(joined).toEqual([draft, { ...draft, ownerPin: "2580" }]);
  });
});
