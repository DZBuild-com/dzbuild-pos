import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { isApprovalCancelled, useApproval } from "@/components/manager-approval";
import { cashierSession, ownerTile } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";

function Harness({ action, onResult }: { action: () => Promise<string>; onResult: (r: string) => void }) {
  const approval = useApproval();
  return (
    <>
      <button
        onClick={() =>
          void approval
            .run(action)
            .then(onResult, (e: unknown) => onResult(isApprovalCancelled(e) ? "cancelled" : "failed"))
        }
      >
        Appliquer la remise
      </button>
      {approval.dialog}
    </>
  );
}

describe("manager approval", () => {
  it("asks a manager's PIN, then retries the action once", async () => {
    const overrides: unknown[] = [];
    mockCommands({
      auth_session: () => cashierSession,
      auth_list_users: () => [ownerTile, { ...ownerTile, id: "u-cashier", displayName: "Yacine" }],
      auth_manager_override: (args) => {
        overrides.push(args);
        return {
          approverId: "u-owner",
          approverName: "Amine Benali",
          permission: "sales.discount",
          grantedAt: 1,
        };
      },
    });
    let calls = 0;
    const action = () => {
      calls += 1;
      return calls === 1 ? Promise.reject({ code: "discount_above_limit" }) : Promise.resolve("ok");
    };
    const results: string[] = [];
    const user = userEvent.setup();
    renderWithProviders(<Harness action={action} onResult={(r) => results.push(r)} />);
    await user.click(screen.getByRole("button", { name: "Appliquer la remise" }));
    expect(await screen.findByText("Accord d'un responsable")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /Yacine/ })).toBeNull(); // not oneself
    await user.click(screen.getByRole("button", { name: /Amine Benali/ }));
    await user.keyboard("2580");
    await user.click(screen.getByRole("button", { name: "Autoriser" }));
    await waitFor(() => expect(results).toEqual(["ok"]));
    expect(overrides).toEqual([
      { input: { approverId: "u-owner", pin: "2580", permission: "sales.discount" } },
    ]);
    expect(calls).toBe(2);
  });

  it("gives up without an error when the cashier cancels", async () => {
    mockCommands({ auth_session: () => cashierSession, auth_list_users: () => [ownerTile] });
    const results: string[] = [];
    const user = userEvent.setup();
    renderWithProviders(
      <Harness
        action={() =>
          Promise.reject({ code: "insufficient_permission", details: { permission: "sales.void_line" } })
        }
        onResult={(r) => results.push(r)}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Appliquer la remise" }));
    expect(await screen.findByText(/Annuler une ligne de vente/)).toBeInTheDocument();
    await act(() => user.click(screen.getByRole("button", { name: "Annuler" })));
    await waitFor(() => expect(results).toEqual(["cancelled"]));
  });

  it("does not ask for what no manager can grant", async () => {
    mockCommands({});
    const results: string[] = [];
    const user = userEvent.setup();
    renderWithProviders(
      <Harness
        action={() => Promise.reject({ code: "stock_insufficient" })}
        onResult={(r) => results.push(r)}
      />,
    );
    await user.click(screen.getByRole("button", { name: "Appliquer la remise" }));
    await waitFor(() => expect(results).toEqual(["failed"]));
    expect(screen.queryByText("Accord d'un responsable")).toBeNull();
  });
});
