import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { contractPrint, processingRegisterPrint, routeSheetPrint } from "@/test/fixtures-m8";
import { ContractPage } from "./contract";
import { DocumentPage, pageRtl } from "./page";
import { ProcessingRegisterDocument } from "./processing-register";
import { RouteSheetPage } from "./route-sheet";

describe("pack A4 pages", () => {
  it("prints the contract with its schedule, guarantees and the no-cheque clause, right to left in Arabic", () => {
    const { container } = render(<ContractPage model={contractPrint("ar")} />);
    expect(container.firstElementChild?.getAttribute("dir")).toBe("rtl");
    expect(screen.getAllByRole("row")).toHaveLength(1 + 6);
    expect(screen.getByText("لا يُطلب أي صك ضمان ولا يُقبل.")).toBeTruthy();
    expect(screen.getByText("Farid Bouzid")).toBeTruthy();
  });

  it("prints the route sheet with one row per stop and the signature boxes", () => {
    render(<RouteSheetPage model={routeSheetPrint("fr")} />);
    expect(screen.getByText("Bon de route")).toBeTruthy();
    expect(screen.getAllByRole("row")).toHaveLength(1 + 2 + 1);
    expect(screen.getByText("Le chauffeur")).toBeTruthy();
    expect(screen.getByText("Le magasin")).toBeTruthy();
  });

  it("opens both pages from the print host's page switch", () => {
    const contract = { template: "contract", model: contractPrint("fr") } as const;
    const sheet = { template: "route_sheet", model: routeSheetPrint("fr") } as const;
    render(<DocumentPage page={contract} />);
    expect(screen.getByText("Contrat de vente à tempérament")).toBeTruthy();
    render(<DocumentPage page={sheet} />);
    expect(screen.getAllByText("Bon de route")).toHaveLength(1);
    expect(pageRtl({ template: "contract", model: contractPrint("ar") })).toBe(true);
    expect(pageRtl(sheet)).toBe(false);
  });

  it("prints the processing register with its DPO and one row per activity (Task 21b, PDP-005)", () => {
    const fr = processingRegisterPrint("fr");
    render(<ProcessingRegisterDocument m={fr} />);
    expect(screen.getByText(fr.title)).toBeTruthy();
    expect(screen.getByText(fr.dpo ?? "")).toBeTruthy();
    expect(screen.getAllByRole("row")).toHaveLength(1 + fr.activities.length);
    expect(screen.getByText(fr.security)).toBeTruthy();
    const ar = { template: "processing_register", model: processingRegisterPrint("ar") } as const;
    expect(pageRtl(ar)).toBe(true);
    render(<DocumentPage page={ar} />);
    expect(screen.getByText(ar.model.title)).toBeTruthy();
  });
});
