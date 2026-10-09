import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { Amount } from "@/components/amount";
import { useLightMode } from "@/lib/display";
import { useAppDataDir } from "@/lib/ipc/app";
import { useCategories } from "@/lib/ipc/catalog";
import { useProductGrid } from "@/lib/ipc/products";
import type { ProductHit } from "@/lib/ipc/types";
import { mediaUrl } from "@/lib/media";
import { localizedName } from "@/lib/names";
import { useLang } from "@/lib/use-language";
import { cn, initials } from "@/lib/utils";

/** `--pos-tile`'s default (7.5rem) at 16 px, until the probe is measured (and in jsdom). */
const TILE = 120;
const GAP = 8;

/** Category chips, then square tiles of `--pos-tile` (7.5rem, 6rem in light mode; rem follows the
 * text size): the photo or the initials on surface-2, the name, the price in bold. Virtualised by
 * rows so 200 tiles stay smooth on a Celeron (spec §10.5, SAL-010); light mode shows initials
 * only. The sale screen keeps the chosen category: the scale panel (F11) offers its weighed
 * products. */
export function ProductGrid({
  onPick,
  category,
  onCategory,
}: {
  onPick: (hit: ProductHit) => void;
  category: string | null;
  onCategory: (id: string | null) => void;
}) {
  const { t } = useTranslation();
  const lang = useLang();
  const categories = useCategories();
  const grid = useProductGrid(category);
  const hits = grid.data ?? [];
  const light = useLightMode();
  const scroller = useRef<HTMLDivElement>(null);
  // As wide as a tile: its size follows light mode and the text size, and the observer sees it.
  const probe = useRef<HTMLDivElement>(null);
  const [{ columns, tile }, setLayout] = useState({ columns: 5, tile: TILE });
  useEffect(() => {
    const element = scroller.current;
    const sizer = probe.current;
    if (!element || !sizer) return undefined;
    const measure = () => {
      const width = sizer.getBoundingClientRect().width || TILE;
      const fit = Math.max(2, Math.floor((element.clientWidth + GAP) / (width + GAP)) || 5);
      setLayout((now) => (now.columns === fit && now.tile === width ? now : { columns: fit, tile: width }));
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    observer.observe(sizer);
    return () => observer.disconnect();
  }, []);
  const rows = Math.ceil(hits.length / columns);
  // The app does not run the React Compiler; the grid re-renders with the virtualizer's state.
  // eslint-disable-next-line react-hooks/incompatible-library -- TanStack Virtual (spec §10.5)
  const virtualizer = useVirtualizer({
    count: rows,
    getScrollElement: () => scroller.current,
    estimateSize: () => tile + GAP,
    overscan: 3,
    initialRect: { width: 800, height: 700 },
  });
  // The rows' cached heights were estimated with the previous tile size.
  useEffect(() => virtualizer.measure(), [virtualizer, tile]);
  const chip = (id: string | null, label: string) => (
    <button
      key={id ?? "all"}
      type="button"
      role="radio"
      aria-checked={category === id}
      onClick={() => onCategory(id)}
      className={cn(
        "min-h-11 shrink-0 rounded-full border px-4 font-semibold",
        category === id ? "border-brand bg-brand-soft" : "border-line bg-surface text-ink-2",
      )}
    >
      {label}
    </button>
  );
  return (
    <div className="flex min-h-0 flex-1 flex-col gap-3">
      <div
        role="radiogroup"
        aria-label={t("pos:categories.label")}
        className="flex gap-2 overflow-x-auto pb-1"
      >
        {chip(null, t("pos:categories.all"))}
        {(categories.data ?? []).filter((c) => !c.archived).map((c) => chip(c.id, localizedName(c, lang)))}
      </div>
      <div ref={scroller} aria-label={t("pos:grid.label")} className="min-h-0 flex-1 overflow-y-auto">
        {hits.length === 0 && !grid.isPending ? (
          <p className="p-6 text-ink-2">{t("pos:grid.empty")}</p>
        ) : null}
        <div style={{ height: virtualizer.getTotalSize() }} className="relative w-full">
          <div ref={probe} aria-hidden className="invisible absolute top-0 h-0 w-[var(--pos-tile,7.5rem)]" />
          {virtualizer.getVirtualItems().map((row) => (
            <div
              key={row.key}
              className="absolute inset-x-0 flex gap-2"
              style={{ top: row.start, height: tile }}
            >
              {hits.slice(row.index * columns, row.index * columns + columns).map((hit) => {
                const name = localizedName(hit, lang);
                return (
                  <button
                    key={hit.id}
                    type="button"
                    onClick={() => onPick(hit)}
                    className="flex size-[var(--pos-tile,7.5rem)] shrink-0 flex-col justify-between rounded-card border border-line bg-surface p-2 text-start hover:border-brand"
                  >
                    {hit.image && !light ? (
                      <TilePhoto image={hit.image} fallback={initials(name)} />
                    ) : (
                      <Initials text={initials(name)} />
                    )}
                    <span className="line-clamp-2 text-sm leading-tight font-semibold">{name}</span>
                    <Amount value={hit.sellPrice} className="font-bold" />
                  </button>
                );
              })}
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

function Initials({ text }: { text: string }) {
  return (
    <span
      aria-hidden
      className="grid h-10 w-full place-items-center rounded-control bg-surface-2 font-bold text-ink-2"
    >
      {text}
    </span>
  );
}

/** The product's thumbnail (`<data>/media`) in the initials' place; the name is under it. */
function TilePhoto({ image, fallback }: { image: string; fallback: string }) {
  const dataDir = useAppDataDir().data;
  const src = dataDir ? mediaUrl(dataDir, image, true) : null;
  if (!src) return <Initials text={fallback} />;
  return (
    <img
      src={src}
      alt=""
      aria-hidden
      loading="lazy"
      decoding="async"
      className="h-10 w-full rounded-control bg-surface-2 object-cover"
    />
  );
}
