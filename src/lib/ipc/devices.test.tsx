import { act, renderHook, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it } from "vitest";
import { useDeviceStatus, useWeight } from "@/lib/ipc/devices";
import { weightFixture } from "@/test/fixtures-m4";
import { mockCommands, providers } from "@/test/render";

describe("device hooks", () => {
  it("starts from the latest weight and follows weight-changed", async () => {
    mockCommands({ scale_latest: () => weightFixture });
    const { result } = renderHook(() => useWeight(true), { wrapper: providers() });
    await waitFor(() => expect(result.current?.net).toBe(1_250));
    await act(() =>
      emit("weight-changed", {
        reading: { ...weightFixture, gross: 2_000, net: 2_000, stable: false, state: "motion" },
      }),
    );
    await waitFor(() => expect(result.current?.stable).toBe(false));
    expect(result.current?.net).toBe(2_000);
  });

  it("updates a device's state from device-status-changed", async () => {
    mockCommands({
      devices_status: () => [{ device: "scale", state: { state: "connected", port: "COM3" } }],
    });
    const { result } = renderHook(() => useDeviceStatus(), { wrapper: providers() });
    await waitFor(() => expect(result.current.data?.[0]?.state.state).toBe("connected"));
    await act(() =>
      emit("device-status-changed", {
        device: "scale",
        state: { state: "disconnected", port: "COM3", code: "port_unavailable" },
      }),
    );
    await waitFor(() => expect(result.current.data?.[0]?.state.state).toBe("disconnected"));
  });
});
