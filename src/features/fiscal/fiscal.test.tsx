import { act, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it, vi } from "vitest";
import { ReportsPage } from "@/features/reports/ReportsPage";
import { cashierSession, ownerSession } from "@/test/fixtures";
import { mockCommands, renderWithProviders } from "@/test/render";
import type { ArchiveDone } from "@/lib/ipc/types";
import { chainsBroken } from "./ArchiveExport";
import { FiscalPage } from "./FiscalPage";

const g50 = {
  from: "2026-09-01",
  to: "2026-09-30",
  form: "G n° 50 (2025)",
  lines: [
    { key: "tva19", box: "E1-19", amount: 171_000 },
    { key: "tva9", box: "E1-9", amount: 90_000 },
    { key: "tvaTotal", box: "E1-T", amount: 261_000 },
    { key: "stampDuty", box: "", amount: 11_400 },
  ],
  otherRates: [],
  tvaCollected: 261_000,
  tvaDeductible: 0,
  tvaDue: 261_000,
  tvaCredit: 0,
  stampDuty: 11_400,
};

describe("fiscal screen", () => {
  it("shows the G50 lines of a month and copies an amount the way the forms want it", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });
    const asked: unknown[] = [];
    mockCommands({
      fiscal_g50: (args) => {
        asked.push(args);
        return g50;
      },
    });
    renderWithProviders(<FiscalPage session={ownerSession} />);
    const month = await screen.findByLabelText("Mois déclaré");
    await userEvent.clear(month);
    await userEvent.type(month, "2026-09");
    expect(await screen.findByText("TVA due — 19 %")).toBeInTheDocument();
    expect(screen.getByText("E1-19")).toBeInTheDocument();
    await waitFor(() => expect(asked.at(-1)).toEqual({ month: "2026-09" }));
    await userEvent.click(screen.getByRole("button", { name: "Copier TVA due — 19 %" }));
    expect(writeText).toHaveBeenCalledWith("1710,00");
    expect(
      screen.getByText("IRG sur salaires, TAP et autres impôts : hors de la caisse, à déclarer vous-même."),
    ).toBeInTheDocument();
  });

  it("exports the fiscal archive with its progress and shows the manifest", async () => {
    let finish: (value: unknown) => void = () => {};
    mockCommands({
      fiscal_g50: () => g50,
      fiscal_export_archive: () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    });
    renderWithProviders(<FiscalPage session={ownerSession} />);
    await userEvent.click(await screen.findByRole("tab", { name: "Archive fiscale" }));
    await userEvent.click(screen.getByRole("button", { name: "Exporter l'archive" }));
    await act(() => emit("archive-progress", { stage: "parts", done: 4, total: 12 }));
    expect(await screen.findByRole("progressbar")).toHaveAttribute("aria-valuenow", "33");
    await act(async () => {
      finish({
        path: "C:\\Archives\\archive-fiscale-2025-01-01-2025-12-31.zip",
        manifestSha256: "9f2c5d1e0a7b4c3d2e1f00112233445566778899aabbccddeeff001122334455",
        files: [{ name: "documents.csv", sha256: "ab", bytes: 1_024, rows: 12_450 }],
        chains: {
          documents: [],
          audit: { rows: 90_000, firstBrokenSeq: null, lastSeq: 90_000, lastHash: "cd" },
          closures: [],
          receipts: { rows: 2, firstBrokenNumber: null },
          tillJournals: [],
          tillMergeFirstBroken: null,
        },
      });
    });
    expect(
      await screen.findByText(
        "Archive enregistrée : C:\\Archives\\archive-fiscale-2025-01-01-2025-12-31.zip",
      ),
    ).toBeInTheDocument();
    expect(
      screen.getByText("9f2c5d1e0a7b4c3d2e1f00112233445566778899aabbccddeeff001122334455"),
    ).toBeInTheDocument();
    expect(screen.getByText("Aucun document n'a été modifié.")).toBeInTheDocument();
  });

  it("counts the versement receipts among the chains of an archive", () => {
    const done: ArchiveDone = {
      path: "archive-fiscale-2025-01-01-2025-12-31.zip",
      manifestSha256: "ab",
      files: [],
      chains: {
        documents: [],
        audit: { rows: 3, firstBrokenSeq: null, lastSeq: 3, lastHash: "cd" },
        closures: [],
        receipts: { rows: 2, firstBrokenNumber: null },
        tillJournals: [],
        tillMergeFirstBroken: null,
      },
    };
    expect(chainsBroken(done)).toBe(false);
    expect(
      chainsBroken({
        ...done,
        chains: { ...done.chains, receipts: { rows: 2, firstBrokenNumber: "RV2025-000002" } },
      }),
    ).toBe(true);
    const tillJournal = (broken: number) => ({
      terminalId: "0190a000-0000-7000-8000-0000000000b2",
      terminalCode: "C2",
      report: { rows: 5, broken, firstBrokenSeq: broken > 0 ? 3 : null, lastSeq: 5, lastHash: "ef" },
    });
    expect(chainsBroken({ ...done, chains: { ...done.chains, tillJournals: [tillJournal(0)] } })).toBe(false);
    expect(chainsBroken({ ...done, chains: { ...done.chains, tillJournals: [tillJournal(1)] } })).toBe(true);
    expect(
      chainsBroken({
        ...done,
        chains: { ...done.chains, tillJournals: [tillJournal(0)], tillMergeFirstBroken: 4 },
      }),
    ).toBe(true);
  });

  it("reports an altered archive file", async () => {
    mockCommands({
      fiscal_g50: () => g50,
      fiscal_verify_archive: () => ({ manifestSha256: "9f", badFiles: ["documents.csv"] }),
    });
    renderWithProviders(<FiscalPage session={ownerSession} />);
    await userEvent.click(await screen.findByRole("tab", { name: "Archive fiscale" }));
    await userEvent.click(screen.getByRole("button", { name: "Vérifier une archive" }));
    expect(await screen.findByText("Fichiers modifiés depuis l'export : documents.csv")).toBeInTheDocument();
  });

  it("opens from the reports catalogue for whoever holds reports.tax", async () => {
    mockCommands({ auth_session: () => ownerSession, reports_catalog: () => [] });
    const owner = renderWithProviders(<ReportsPage />, { route: "/reports" });
    expect(await screen.findByRole("link", { name: "Fiscalité" })).toHaveAttribute("href", "/reports/fiscal");
    owner.unmount();
    mockCommands({ auth_session: () => cashierSession, reports_catalog: () => [] });
    renderWithProviders(<ReportsPage />, { route: "/reports" });
    expect(
      await screen.findByText(
        "Votre rôle ne peut ouvrir aucun rapport. Demandez au propriétaire d'en ajouter un.",
      ),
    ).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Fiscalité" })).toBeNull();
  });
});
