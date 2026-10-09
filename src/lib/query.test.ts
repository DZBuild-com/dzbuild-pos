import { onlineManager } from "@tanstack/react-query";
import { afterEach, describe, expect, it } from "vitest";
import { createQueryClient } from "./query";

/** Settles with `promise`, or rejects with "paused" when it is still pending after `ms`. */
function within<T>(ms: number, promise: Promise<T>): Promise<T> {
  return Promise.race([
    promise,
    new Promise<T>((_, reject) => setTimeout(() => reject(new Error("paused")), ms)),
  ]);
}

describe("the query client", () => {
  afterEach(() => onlineManager.setOnline(true));

  // Every call goes to the local Rust backend: a PC whose cable or Wi-Fi drops (the webview's
  // `offline` event) keeps selling, so nothing waits for a network to come back.
  it("keeps calling the backend while Windows reports no network", async () => {
    onlineManager.setOnline(false);
    const client = createQueryClient();
    await expect(
      within(1_000, client.fetchQuery({ queryKey: ["probe"], queryFn: () => Promise.resolve(42) })),
    ).resolves.toBe(42);
    const mutation = client
      .getMutationCache()
      .build(client, { mutationFn: (n: number) => Promise.resolve(n + 1) });
    await expect(within(1_000, mutation.execute(1))).resolves.toBe(2);
  });
});
