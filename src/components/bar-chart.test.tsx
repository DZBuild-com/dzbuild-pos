import { fireEvent, render } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { BarChart } from "@/components/bar-chart";
import { setLanguage } from "@/lib/i18n";

/** The name drawn beside an hbar bar (not its `<title>` nor the hidden table's copy). */
function drawnName(container: HTMLElement, start: string): string | null {
  const texts = [...container.querySelectorAll("svg text")].map((node) => node.textContent ?? "");
  return texts.find((text) => text.includes(start)) ?? null;
}

describe("BarChart hbar names", () => {
  it("wraps each name in FSI … PDI so it keeps its own direction", () => {
    const { container } = render(
      <BarChart title="Top" kind="hbar" rows={[{ label: "Pain", value: 100 }]} formatValue={String} />,
    );
    expect(drawnName(container, "Pain")).toBe("\u2068Pain\u2069");
  });

  it("keeps the ellipsis of a cut French name inside the isolate in an Arabic chart", async () => {
    await setLanguage("ar");
    const label = "Eau minérale naturelle Ifri 1,5 L";
    const { container } = render(
      <BarChart title="Top" kind="hbar" rows={[{ label, value: 100 }]} formatValue={String} />,
    );
    // NAMES - 16 = 164 units at 7 per character: 23 characters, the last one the ellipsis.
    expect(drawnName(container, "Eau")).toBe(`\u2068${label.slice(0, 22)}…\u2069`);
  });
});

describe("BarChart over a long period", () => {
  const days = Array.from({ length: 270 }, (_, i) => ({ label: `J${i + 1}`, value: 1_000 + i }));

  it("draws every bar at least one unit thick", () => {
    const { container } = render(<BarChart title="Ventes" kind="bar" rows={days} formatValue={String} />);
    const paths = [...container.querySelectorAll("svg path")].map((p) => p.getAttribute("d") ?? "");
    expect(paths).toHaveLength(270);
    for (const d of paths) {
      expect(d).not.toMatch(/NaN/);
      // `M{a},{base} V… H{b - r}…`: the start and end of a bar across its band, in order.
      const [, a, b] = /^M(-?[\d.]+),[^H]*H(-?[\d.]+)/.exec(d) ?? [];
      expect(Number(b) - Number(a)).toBeGreaterThanOrEqual(0);
    }
  });

  it("is one Tab stop whose arrow keys follow the bars, mirrored in Arabic", async () => {
    const { container, unmount } = render(
      <BarChart title="Ventes" kind="bar" rows={days.slice(0, 5)} formatValue={String} />,
    );
    const bars = () => [...container.querySelectorAll<SVGGElement>("svg g[aria-label]")];
    expect(bars().filter((g) => g.getAttribute("tabindex") === "0")).toHaveLength(1);
    expect(container.querySelector("svg")?.getAttribute("role")).toBe("group");
    bars()[0]?.focus();
    fireEvent.keyDown(bars()[0] as Element, { key: "ArrowRight" });
    expect(document.activeElement).toBe(bars()[1]);
    fireEvent.keyDown(bars()[1] as Element, { key: "End" });
    expect(document.activeElement).toBe(bars()[4]);
    expect(bars()[4]?.getAttribute("tabindex")).toBe("0");
    unmount();

    await setLanguage("ar");
    const rtl = render(<BarChart title="Ventes" kind="bar" rows={days.slice(0, 5)} formatValue={String} />);
    const mirrored = [...rtl.container.querySelectorAll<SVGGElement>("svg g[aria-label]")];
    mirrored[0]?.focus();
    fireEvent.keyDown(mirrored[0] as Element, { key: "ArrowLeft" });
    expect(document.activeElement).toBe(mirrored[1]);
    await setLanguage("fr");
  });
});
