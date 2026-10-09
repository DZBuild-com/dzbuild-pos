import { act, screen, waitFor } from "@testing-library/react";
import { mockConvertFileSrc } from "@tauri-apps/api/mocks";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { ProductGrid } from "@/features/pos/ProductGrid";
import { applyDisplay } from "@/lib/display";
import { mediaUrl } from "@/lib/media";
import { productHitsFixture } from "@/test/fixtures-m1";
import { mockCommands, renderWithProviders } from "@/test/render";

const DATA_DIR = "C:\\Users\\a\\AppData\\Roaming\\dev.dzbuild.pos";
const IMAGE = "products/01923f4e-8a1b-7c2d-9e3f-4a5b6c7d8e9f-1790593200000.jpg";

describe("Light mode at the till", () => {
  beforeEach(() => {
    // jsdom has no layout: TanStack Virtual would render no tile row at 0×0.
    vi.spyOn(HTMLElement.prototype, "offsetWidth", "get").mockReturnValue(800);
    vi.spyOn(HTMLElement.prototype, "offsetHeight", "get").mockReturnValue(700);
  });

  it("shows the product photo on its tile, and only the initials in light mode", async () => {
    const [milk] = productHitsFixture;
    mockCommands({
      app_data_dir: () => DATA_DIR,
      catalog_categories_list: () => [],
      products_grid: () => [{ ...milk, image: IMAGE }],
    });
    mockConvertFileSrc("windows");
    const { container } = renderWithProviders(
      <ProductGrid onPick={() => undefined} category={null} onCategory={() => undefined} />,
    );
    await waitFor(() =>
      expect(container.querySelector("img")).toHaveAttribute("src", mediaUrl(DATA_DIR, IMAGE, true)),
    );
    await act(async () => applyDisplay({ lightMode: true, textSize: "normal", onscreenKeyboard: false }));
    await waitFor(() => expect(container.querySelector("img")).toBeNull());
    expect(screen.getByRole("button", { name: /Lait Candia 1 L/ })).toBeInTheDocument();
  });
});
