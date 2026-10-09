import { screen } from "@testing-library/react";
import { expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import { renderM8 } from "@/test/render-m8";
import { packState, sessionWith, zakatPreview } from "@/test/fixtures-m8";
import { ZakatScreen } from "./ZakatScreen";

// The setup file clears the IPC mocks after unmounting: clearing them first would break the LAN
// status listener's unlisten on unmount (an unhandled rejection).

it("shows the base against the nisab, the Hijri date and the disclaimer, in Arabic", async () => {
  const owner = sessionWith(["reports.profit"], "ar");
  mockCommands({
    auth_session: () => owner,
    packs_state: () => packState({ modules: ["zakat"] }),
    zakat_preview: () => zakatPreview(),
    zakat_list: () => [],
  });
  // `auth_session` is mocked above: `renderM8`'s `session` would mock again and drop the others.
  await renderM8(<ZakatScreen />, { lang: "ar" });
  expect(await screen.findByText("17 ربيع الآخر 1448")).toBeInTheDocument();
  expect(screen.getByText("الزكاة المستحقة")).toBeInTheDocument();
  expect(screen.getByText(/وليس فتوى/)).toBeInTheDocument();
});
