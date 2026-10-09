import { act, cleanup, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { UpdateReadyDialog } from "@/features/updates/UpdateReadyDialog";
import { resetPrivateData } from "@/lib/ipc/auth";
import { qk } from "@/lib/ipc/keys";
import { createQueryClient } from "@/lib/query";
import { cashierSession, ownerSession } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

const BODY = "La version 0.6.1 est prête. L'installer maintenant ? DZBuild POS redémarrera.";

const ready = (blockers: unknown[] = []) => ({
  currentVersion: "0.6.0",
  channel: "stable",
  autoCheck: true,
  autoDownload: true,
  lastCheckAt: 1_790_593_200_000,
  lastCheckError: null,
  offer: { version: "0.6.1", notes: null, pubDate: null, decision: { kind: "offer" } },
  download: { kind: "ready" },
  blockers,
  installing: false,
});

const drawer = {
  kind: "cashSessionOpen",
  terminalCode: "C2",
  userName: "Yacine",
  openedAt: 1_790_578_800_000,
};

/** Lets the status query answer and the component settle. */
async function settle(calls: string[], count: number): Promise<void> {
  await waitFor(() => expect(calls.filter((c) => c === "updates_status").length).toBe(count));
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 20));
  });
}

describe("the update offered at sign-in", () => {
  it("is offered once per sign-in, to the owner", async () => {
    const calls = mockCommands({ updates_status: () => ready() });
    const user = userEvent.setup();
    const session = { ...ownerSession, startedAt: 1_790_600_000_000 };
    renderWithProviders(<UpdateReadyDialog session={session} />);
    expect(await screen.findByText(BODY)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Plus tard" }));
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    cleanup();
    // Back from the till screen: the same sign-in is not asked again.
    renderWithProviders(<UpdateReadyDialog session={session} />);
    await settle(calls, 2);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    cleanup();
    // The next sign-in is.
    renderWithProviders(<UpdateReadyDialog session={{ ...session, startedAt: 1_790_600_100_000 }} />);
    expect(await screen.findByText(BODY)).toBeInTheDocument();
    expect(calls).not.toContain("updates_install");
  });

  it("is not offered when a drawer is open at sign-in, nor later in that sign-in", async () => {
    let current = ready([drawer]);
    const calls = mockCommands({ updates_status: () => current });
    const { client } = renderWithProviders(
      <UpdateReadyDialog session={{ ...ownerSession, startedAt: 1_790_600_200_000 }} />,
    );
    await settle(calls, 1);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    // C2 closes its Z: the offer comes with that Z and the top bar's pill, not as a dialog here.
    current = ready();
    await act(() => client.invalidateQueries({ queryKey: qk.updates }));
    await settle(calls, 2);
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });

  it("a new sign-in reads the status again (the one cached before may show a drawer since closed)", () => {
    const client = createQueryClient();
    client.setQueryData(qk.updates, ready([drawer]));
    resetPrivateData(client);
    expect(client.getQueryData(qk.updates)).toBeUndefined();
  });

  it("is never offered to whoever cannot install it", async () => {
    const calls = mockCommands({ updates_status: () => ready() });
    renderWithProviders(<UpdateReadyDialog session={cashierSession} />);
    await act(async () => {
      await new Promise((resolve) => setTimeout(resolve, 50));
    });
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
    expect(calls).not.toContain("updates_status");
  });
});
