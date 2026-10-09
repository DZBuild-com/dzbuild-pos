import { clearMocks } from "@tauri-apps/api/mocks";
import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { packState, sessionWith, staffProfile } from "@/test/fixtures-m8";
import { StaffPage } from "./StaffPage";

afterEach(() => clearMocks());

it("gives an advance from the till and shows what is owed", async () => {
  const given: unknown[] = [];
  const manager = sessionWith(["settings.users", "cash.paid_in_out", "reports.cash"]);
  mockCommands({
    auth_session: () => manager,
    packs_state: () => packState({ modules: ["staff"] }),
    staff_list: () => [staffProfile({ owed: 0 })],
    staff_entries: () => [],
    staff_shifts: () => [],
    staff_advance: (args) => {
      given.push(args);
      return { id: "e1", kind: "advance", amount: 500_000 };
    },
  });
  const user = userEvent.setup();
  // `auth_session` is mocked above: `renderM8`'s `session` would mock again and drop the others.
  await renderM8(<StaffPage userId="u2" />);
  await user.click(await screen.findByRole("button", { name: "Avance sur salaire" }));
  await user.type(screen.getByLabelText("Montant"), "5000");
  await user.click(screen.getByRole("checkbox", { name: "Payée depuis la caisse" }));
  await user.click(screen.getByRole("button", { name: "Verser l'avance" }));
  await waitFor(() =>
    expect(given[0]).toMatchObject({ input: { userId: "u2", amount: 500_000, fromTill: true } }),
  );
});
