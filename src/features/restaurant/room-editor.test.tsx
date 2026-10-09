import { clearMocks } from "@tauri-apps/api/mocks";
import { fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { packState, sessionWith, tables } from "@/test/fixtures-m8";
import { RoomEditor } from "./RoomEditor";

afterEach(() => clearMocks());

it("drags a table to a free cell and refuses a taken one", async () => {
  const saved: unknown[] = [];
  mockCommands({
    auth_session: () => sessionWith(["settings.company"]),
    packs_state: () => packState({ modules: ["restaurant"] }),
    dining_tables: () => tables(),
    dining_table_save: (args) => {
      saved.push(args);
      return { ...tables()[0], gridCol: 3, gridRow: 2 };
    },
  });
  await renderM8(<RoomEditor roomId="r1" gridCols={4} gridRows={3} />);
  const t1 = await screen.findByRole("button", { name: /T1/ });
  fireEvent.pointerDown(t1, { pointerId: 1, clientX: 10, clientY: 10 });
  fireEvent.pointerMove(screen.getByTestId("cell-3-2"), { pointerId: 1 });
  fireEvent.pointerUp(screen.getByTestId("cell-3-2"), { pointerId: 1 });
  await waitFor(() => expect(saved[0]).toMatchObject({ id: "tb1", input: { gridCol: 3, gridRow: 2 } }));
  fireEvent.pointerDown(t1, { pointerId: 1 });
  fireEvent.pointerUp(screen.getByTestId("cell-1-0"), { pointerId: 1 });
  expect(saved).toHaveLength(1);
  expect(screen.getByRole("status")).toHaveTextContent("Cette place est déjà prise.");
});
