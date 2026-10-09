import { getDefaultNormalizer, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { ScaleCodesCard } from "@/features/settings/hardware/ScaleCodesCard";
import { formatDZD } from "@/lib/format";
import type { ScanHit } from "@/lib/ipc/types";
import { productFixture } from "@/test/fixtures-m1";
import { lanTillStatusFixture } from "@/test/fixtures-m5";
import { mockCommands, renderWithProviders } from "@/test/render";
import { setBurstGuard } from "@/lib/scanner";
import { scanDigits } from "@/test/scan";

const tomatoes = {
  ...productFixture,
  id: "p-tomatoes",
  nameFr: "Tomates",
  unitCode: "kg",
  isWeighed: true,
  plu: 123,
};

/** "22 00123 00451": a price label of 451 DA, read by the backend as 3 758 g. */
const priceLabel: ScanHit = {
  product: tomatoes,
  packQty: 3_758,
  scale: {
    code: { prefix: "22", plu: 123, value: "price", raw: 45_100 },
    labelPrice: 45_100,
    till: 45_096,
    // a sentinel: the screen shows `till` as the backend sends it (M4: the UI never computes
    // money), so this difference deliberately does not add up to it
    difference: 0,
  },
};

function mockCard(hit: ScanHit | null, asked: string[] = []) {
  mockCommands({
    settings_get: (args) =>
      ({ "scale.barcode_formats": null, "products.internal_ean_prefix": "200" })[args.key as string],
    datapack_get: () => ({
      scaleBarcodes: {
        source: "",
        defaultLayouts: [
          { prefix: "22", pluDigits: 5, verifier: false, value: "price", valueDigits: 5, decimals: 0 },
        ],
      },
    }),
    products_find_by_code: (args) => {
      asked.push(args.code as string);
      return hit;
    },
  });
}

/** What an AZERTY Windows types for the digits a scanner sends as a US keyboard. */
const AZERTY_DIGITS: Record<string, string> = {
  "0": "à",
  "1": "&",
  "2": "é",
  "3": '"',
  "4": "'",
  "5": "(",
  "6": "-",
  "7": "è",
  "8": "_",
  "9": "ç",
};

describe("scale codes on a till", () => {
  it("are read-only, with the main PC's name and no save, add or reset button", async () => {
    mockCommands({
      settings_get: (args) =>
        ({ "scale.barcode_formats": null, "products.internal_ean_prefix": "200" })[args.key as string],
      datapack_get: () => ({
        scaleBarcodes: {
          source: "",
          defaultLayouts: [
            { prefix: "22", pluDigits: 5, verifier: false, value: "price", valueDigits: 5, decimals: 0 },
          ],
        },
      }),
      lan_status: () => lanTillStatusFixture,
    });
    renderWithProviders(<ScaleCodesCard />);
    expect(
      await screen.findByText(
        "Lecture seule sur ce poste de caisse : faites-le sur le poste principal (Supérette El Amel).",
      ),
    ).toBeInTheDocument();
    for (const name of [/Enregistrer/, /Ajouter/, /Revenir/]) {
      expect(screen.queryByRole("button", { name })).not.toBeInTheDocument();
    }
  });
});

describe("scale codes test box", () => {
  it("reads a scanned label with the scanner's layout, whatever Windows types", async () => {
    const asked: string[] = [];
    // a scanner's burst (the other tests type instantly, with the guard off)
    setBurstGuard(true);
    mockCard(priceLabel, asked);
    renderWithProviders(<ScaleCodesCard />);
    const field = await screen.findByLabelText("Scannez une étiquette de balance");
    scanDigits(field, "2200123004515", (d) => AZERTY_DIGITS[d] ?? d);
    expect(await screen.findByText(/Prix sur l'étiquette/)).toBeInTheDocument();
    expect(asked).toEqual(["2200123004515"]);
    setBurstGuard(false);
  });

  it("shows the till's amount for a price label as the backend computed it", async () => {
    mockCard(priceLabel);
    renderWithProviders(<ScaleCodesCard />);
    await userEvent.type(
      await screen.findByLabelText("Scannez une étiquette de balance"),
      "2200123004515{Enter}",
    );
    expect(
      await screen.findByText(
        `Prix sur l'étiquette ${formatDZD(45_100, "fr")}, à la caisse ${formatDZD(45_096, "fr")}`,
        // the amounts keep their no-break spaces: compare the text exactly as rendered
        { normalizer: getDefaultNormalizer({ collapseWhitespace: false }) },
      ),
    ).toBeInTheDocument();
  });

  it("shows no price line for a weight label, which carries no till amount", async () => {
    mockCard({
      product: tomatoes,
      packQty: 1_750,
      scale: {
        code: { prefix: "21", plu: 10012, value: "weight", raw: 1_750 },
        labelPrice: null,
        till: null,
        difference: 0,
      },
    });
    renderWithProviders(<ScaleCodesCard />);
    await userEvent.type(
      await screen.findByLabelText("Scannez une étiquette de balance"),
      "2110012017506{Enter}",
    );
    expect(await screen.findByText(/Tomates/)).toBeInTheDocument();
    expect(screen.queryByText(/Prix sur l'étiquette/)).not.toBeInTheDocument();
  });
});
