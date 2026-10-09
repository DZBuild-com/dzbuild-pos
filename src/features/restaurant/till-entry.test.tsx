import { screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { Route, Routes } from "react-router";
import { TillEntry } from "@/features/pos/TillEntry";
import type { TillMode } from "@/lib/ipc/types";
import { renderM8 } from "@/test/render-m8";
import { packState, sessionWith } from "@/test/fixtures-m8";

const cashier = sessionWith(["sales.create"]);

function till(tillMode: TillMode, modules: "restaurant"[] = ["restaurant"]) {
  return renderM8(
    <Routes>
      <Route path="/pos" element={<TillEntry session={cashier} />} />
      <Route path="/pos/floor" element={<p>floor</p>} />
    </Routes>,
    { route: "/pos", session: cashier, commands: { packs_state: () => packState({ modules, tillMode }) } },
  );
}

describe("the till entry (UX-020)", () => {
  it("opens the floor in the restaurant mode", async () => {
    await till("restaurant");
    expect(await screen.findByText("floor")).toBeInTheDocument();
  });

  it("gives the sale screen a Tables button in the mixed mode", async () => {
    const user = userEvent.setup();
    await till("mixed");
    await user.click(await screen.findByRole("button", { name: "Tables" }));
    expect(await screen.findByText("floor")).toBeInTheDocument();
  });

  it("keeps the plain sale screen for a shop without the restaurant", async () => {
    await till("restaurant", []);
    expect(await screen.findByLabelText("Scanner ou rechercher un produit")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Tables" })).toBeNull();
    expect(screen.queryByText("floor")).toBeNull();
  });
});
