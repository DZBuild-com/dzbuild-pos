import { act, fireEvent, screen, waitFor } from "@testing-library/react";
import { useRef, useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { OnScreenKeyboard } from "@/components/osk/OnScreenKeyboard";
import { ScanField } from "@/features/pos/ScanField";
import type { ProductHit } from "@/lib/ipc/types";
import { productHitsFixture } from "@/test/fixtures-m1";
import { mockCommands, renderWithProviders } from "@/test/render";

function Till({ onPick }: { onPick: (hit: ProductHit) => void }) {
  const inputRef = useRef<HTMLInputElement>(null);
  const [value, setValue] = useState("");
  return (
    <>
      <ScanField
        inputRef={inputRef}
        value={value}
        onValueChange={setValue}
        multiplier={null}
        onCode={() => undefined}
        onPick={onPick}
        onMultiplier={() => undefined}
        onBump={() => undefined}
        onVoid={() => undefined}
        onMove={() => undefined}
      />
      <OnScreenKeyboard />
    </>
  );
}

const press = (label: string) => fireEvent.click(screen.getByRole("button", { name: label }));

describe("searching a product by name on a touch-only till (finding 21)", () => {
  beforeEach(() => {
    document.documentElement.dataset.osk = "1";
  });
  afterEach(() => {
    delete document.documentElement.dataset.osk;
  });

  it("the scan field stays the scanner's until 'Rechercher par nom', then the keyboard types in it", async () => {
    mockCommands({ products_search: () => productHitsFixture.slice(0, 1) });
    const onPick = vi.fn();
    renderWithProviders(<Till onPick={onPick} />);
    const field = screen.getByRole("combobox");
    act(() => field.focus());
    expect(screen.queryByRole("group", { name: "Clavier à l'écran" })).not.toBeInTheDocument();
    press("Rechercher par nom");
    expect(await screen.findByRole("group", { name: "Clavier à l'écran" })).toBeInTheDocument();
    expect(field).toHaveFocus();
    press("h");
    press("u");
    expect(field).toHaveValue("hu");
    await screen.findByRole("listbox");
    act(() => {
      fireEvent.keyDown(field, { key: "ArrowDown" });
    });
    press("Entrée");
    expect(onPick).toHaveBeenCalledWith(productHitsFixture[0]);
    // Enter hands the field back to the scanner.
    await waitFor(() =>
      expect(screen.queryByRole("group", { name: "Clavier à l'écran" })).not.toBeInTheDocument(),
    );
    expect(field).toHaveAttribute("data-osk", "off");
  });
});
