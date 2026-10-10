import { act, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useCart } from "@/features/pos/cart-store";
import { LineEditDialog } from "@/features/pos/LineEditDialog";
import type { PosContext } from "@/features/pos/PosDialogs";
import { mockCommands, renderWithProviders } from "@/test/render";

const milk = {
  productId: "p-milk",
  qty: 1_000,
  display: { name: "Lait Candia 1 L", unitCode: "pc", unitDecimals: 0, price: 15_000 },
};
const oil = {
  productId: "p-oil",
  qty: 1_000,
  display: { name: "Huile Elio 5 L", unitCode: "pc", unitDecimals: 0, price: 65_000 },
};

/** A void the test settles by hand: `true` once the line is gone, `false` when refused. */
function deferredVoid() {
  let settle: (removed: boolean) => void = () => {};
  const voidLine = vi.fn(
    (index: number) =>
      new Promise<boolean>((resolve) => {
        settle = (removed) => {
          if (removed) useCart.getState().remove(index);
          resolve(removed);
        };
      }),
  );
  return { voidLine, settle: (removed: boolean) => settle(removed) };
}

function ctxWith(overrides: Partial<PosContext>): PosContext {
  return {
    reduceQty: () => Promise.resolve(true),
    voidLine: () => Promise.resolve(true),
    ...overrides,
  } as PosContext;
}

describe("the line edit dialog", () => {
  beforeEach(() => {
    useCart.getState().clear("fr");
    useCart.getState().addProduct(milk);
    useCart.getState().addProduct(oil);
    mockCommands({ promotions_presets: () => [] });
  });

  it("closes on 'Supprimer la ligne' only once the void went through, the button busy meanwhile", async () => {
    const { voidLine, settle } = deferredVoid();
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<LineEditDialog index={0} ctx={ctxWith({ voidLine })} onClose={onClose} />);
    const dialog = await screen.findByRole("dialog", { name: "Modifier la ligne" });
    await user.type(within(dialog).getByLabelText(/Note/), "sans sucre");
    const remove = within(dialog).getByRole("button", { name: "Supprimer la ligne" });
    await user.click(remove);
    expect(voidLine).toHaveBeenCalledWith(0);
    expect(remove).toBeDisabled();
    await user.click(remove);
    expect(voidLine).toHaveBeenCalledTimes(1);
    // the manager's approval is cancelled: the dialog stays, with what the cashier typed
    await act(async () => settle(false));
    expect(onClose).not.toHaveBeenCalled();
    expect(remove).toBeEnabled();
    expect(within(dialog).getByLabelText(/Note/)).toHaveValue("sans sucre");
    await user.click(remove);
    await act(async () => settle(true));
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(useCart.getState().lines.map((d) => d.display.name)).toEqual(["Huile Elio 5 L"]);
  });

  it("closes, and never shows the next line, once the line it edits is gone", async () => {
    const onClose = vi.fn();
    renderWithProviders(<LineEditDialog index={0} ctx={ctxWith({})} onClose={onClose} />);
    expect(await screen.findByRole("dialog", { name: "Modifier la ligne" })).toHaveTextContent(
      "Lait Candia 1 L",
    );
    act(() => {
      useCart.getState().remove(0);
    });
    await waitFor(() => expect(onClose).toHaveBeenCalled());
    expect(screen.queryByText("Huile Elio 5 L")).toBeNull();
  });

  it("edits the line it was opened for when a line before it goes", async () => {
    const onClose = vi.fn();
    const user = userEvent.setup();
    renderWithProviders(<LineEditDialog index={1} ctx={ctxWith({})} onClose={onClose} />);
    const dialog = await screen.findByRole("dialog", { name: "Modifier la ligne" });
    act(() => {
      useCart.getState().remove(0);
    });
    expect(dialog).toHaveTextContent("Huile Elio 5 L");
    const qty = within(dialog).getByLabelText("Quantité");
    await user.clear(qty);
    await user.type(qty, "3");
    await user.click(within(dialog).getByRole("button", { name: "Enregistrer la ligne" }));
    expect(onClose).toHaveBeenCalled();
    expect(useCart.getState().lines.map((d) => [d.display.name, d.line.qty])).toEqual([
      ["Huile Elio 5 L", 3_000],
    ]);
  });
});
