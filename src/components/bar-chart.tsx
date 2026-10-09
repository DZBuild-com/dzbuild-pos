import { type KeyboardEvent, useId, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

export interface BarRow {
  label: string;
  value: number;
}

/** Drawing units of the viewBox; the SVG scales to the width of its frame. */
const W = 720;
const FONT = 12;
const GAP = 2;
const MAX_BAR = 24;
const RADIUS = 4;
/** Width of the value axis (bar) or of the names (hbar), on the start side. */
const AXIS = 96;
const NAMES = 180;
/** Room after the plot: a bar's band (bar), half the last value label (hbar). */
const END = 12;
const HBAR_END = 48;
const TOP = 12;
const BOTTOM = 28;
/** hbar: one ranked name per band. */
const BAND = 32;
/** Width of one character of the axis text, to thin labels and cut long names. */
const CHAR = 7;

/** A clean step (1, 2 or 5 × 10ⁿ, at least 1: values are whole centimes or counts). */
function niceStep(raw: number): number {
  if (raw <= 1) return 1;
  const magnitude = 10 ** Math.floor(Math.log10(raw));
  const f = raw / magnitude;
  return (f <= 1 ? 1 : f <= 2 ? 2 : f <= 5 ? 5 : 10) * magnitude;
}

/** Ticks from a nice minimum (0 or below) to a nice maximum (0 or above), about four steps. */
function ticksOf(values: number[]): number[] {
  const lo = Math.min(0, ...values);
  const hi = Math.max(0, ...values);
  const step = niceStep((hi - lo) / 4);
  const first = Math.floor(lo / step) * step;
  const last = Math.max(Math.ceil(hi / step) * step, first + step);
  const ticks: number[] = [];
  for (let tick = first; tick <= last; tick += step) ticks.push(tick);
  return ticks;
}

/** A bar from the baseline `base` to `end` along one axis, 4 px rounded at the data end only
 * (square on the baseline). `vertical`: x is the band start and w its width; else y and h. */
function barPath(vertical: boolean, cross: number, thickness: number, base: number, end: number): string {
  const length = Math.abs(end - base);
  const r = Math.min(RADIUS, thickness / 2, length);
  const dir = end < base ? -1 : 1;
  const a = cross;
  const b = cross + thickness;
  if (vertical) {
    return `M${a},${base} V${end - dir * r} Q${a},${end} ${a + r},${end} H${b - r} Q${b},${end} ${b},${end - dir * r} V${base} Z`;
  }
  return `M${base},${a} H${end - dir * r} Q${end},${a} ${end},${a + r} V${b - r} Q${end},${b} ${end - dir * r},${b} H${base} Z`;
}

function cut(text: string, width: number): string {
  const max = Math.max(3, Math.floor(width / CHAR));
  return text.length > max ? `${text.slice(0, max - 1)}…` : text;
}

/** Below this band (in viewBox units) bars touch: no gap is left between them. */
const MIN_GAPPED_BAND = 3;

/**
 * One series of bars in `--chart-1` (dataviz rules: a single series needs no legend, the title
 * names it). `bar` draws values along time, `hbar` ranks names. Values show on hover or focus
 * (a tooltip, value first), every bar also carries a `<title>`, and a visually hidden table
 * repeats the data for screen readers. The chart is one Tab stop: the arrow keys move between
 * the bars in the order they are drawn (in Arabic the chart mirrors, time runs from the right and
 * → goes back in time), Home and End reach the first and last. Bars stay at least one unit thick
 * however many the period has.
 */
export function BarChart({
  title,
  rows,
  kind,
  formatValue,
}: {
  title: string;
  rows: BarRow[];
  kind: "bar" | "hbar";
  formatValue: (value: number) => string;
}) {
  const { i18n } = useTranslation();
  const rtl = i18n.dir() === "rtl";
  const tipId = useId();
  const [active, setActive] = useState<number | null>(null);
  const [focused, setFocused] = useState<number | null>(null);
  // Roving tabindex: the bar that takes Tab (the last one reached).
  const [current, setCurrent] = useState(0);
  const barRefs = useRef<(SVGGElement | null)[]>([]);
  const ticks = ticksOf(rows.map((r) => r.value));
  const lo = ticks[0] ?? 0;
  const hi = ticks.at(-1) ?? 1;
  const vertical = kind === "bar";
  const n = Math.max(rows.length, 1);
  const H = vertical ? 260 : TOP + n * BAND + BOTTOM;
  // The value axis: vertical bars grow up from `base`; horizontal ones grow toward the end side.
  const plotStart = vertical ? AXIS : NAMES;
  const endPad = vertical ? END : HBAR_END;
  const plotWidth = W - plotStart - endPad;
  const plotHeight = H - TOP - BOTTOM;
  /** A distance from the start edge → x in the viewBox (mirrored in Arabic). */
  const fromStart = (d: number) => (rtl ? W - d : d);
  const valueX = (v: number) => fromStart(plotStart + ((v - lo) / (hi - lo)) * plotWidth);
  const valueY = (v: number) => TOP + plotHeight - ((v - lo) / (hi - lo)) * plotHeight;
  const band = vertical ? plotWidth / n : BAND;
  const thickness = Math.max(1, Math.min(MAX_BAR, band < MIN_GAPPED_BAND ? band : band - GAP));
  const tabStop = Math.min(current, rows.length - 1);
  const moveTo = (index: number) => {
    const next = Math.max(0, Math.min(rows.length - 1, index));
    setCurrent(next);
    barRefs.current[next]?.focus();
  };
  const onBarKey = (event: KeyboardEvent<SVGGElement>, index: number) => {
    // Vertical bars run along the reading direction (mirrored in Arabic); ranked ones downwards.
    const forward = vertical ? (rtl ? "ArrowLeft" : "ArrowRight") : "ArrowDown";
    const back = vertical ? (rtl ? "ArrowRight" : "ArrowLeft") : "ArrowUp";
    const target: Record<string, number> = {
      [forward]: index + 1,
      [back]: index - 1,
      Home: 0,
      End: rows.length - 1,
    };
    const next = target[event.key];
    if (next === undefined) return;
    event.preventDefault();
    moveTo(next);
  };
  // Category labels under vertical bars: every k-th one, so that they never overlap.
  const longest = Math.max(1, ...rows.map((r) => r.label.length));
  const every = vertical ? Math.max(1, Math.ceil((longest * CHAR + 8) / band)) : 1;

  const bars = rows.map((row, i) => {
    if (vertical) {
      const start = plotStart + i * band + (band - thickness) / 2;
      const x = rtl ? W - start - thickness : start;
      const base = valueY(0);
      const end = valueY(row.value);
      return {
        row,
        path: barPath(true, x, thickness, base, end),
        cx: x + thickness / 2,
        cy: Math.min(base, end),
      };
    }
    const y = TOP + i * BAND + (BAND - thickness) / 2;
    const base = valueX(0);
    const end = valueX(row.value);
    return { row, path: barPath(false, y, thickness, base, end), cx: end, cy: y };
  });
  const tip = active === null ? null : bars[active];

  return (
    <figure className="flex flex-col gap-2">
      <figcaption className="font-semibold">{title}</figcaption>
      <div className="relative">
        <svg
          role="group"
          aria-label={title}
          viewBox={`0 0 ${W} ${H}`}
          className="h-auto w-full overflow-visible"
          fontSize={FONT}
        >
          {ticks.map((tick) =>
            vertical ? (
              <g key={tick}>
                <line
                  x1={fromStart(plotStart)}
                  x2={fromStart(W - endPad)}
                  y1={valueY(tick)}
                  y2={valueY(tick)}
                  stroke="var(--line)"
                  strokeWidth={1}
                />
                <text
                  x={fromStart(plotStart - 8)}
                  y={valueY(tick)}
                  dominantBaseline="middle"
                  textAnchor={rtl ? "start" : "end"}
                  fill="var(--ink-2)"
                  direction="ltr"
                  className="tabular-nums"
                >
                  {formatValue(tick)}
                </text>
              </g>
            ) : (
              <g key={tick}>
                <line
                  x1={valueX(tick)}
                  x2={valueX(tick)}
                  y1={TOP}
                  y2={H - BOTTOM}
                  stroke="var(--line)"
                  strokeWidth={1}
                />
                <text
                  x={valueX(tick)}
                  y={H - BOTTOM + 18}
                  textAnchor="middle"
                  fill="var(--ink-2)"
                  direction="ltr"
                  className="tabular-nums"
                >
                  {formatValue(tick)}
                </text>
              </g>
            ),
          )}
          {bars.map(({ row, path }, i) => {
            const label = `${row.label}: ${formatValue(row.value)}`;
            const hit = vertical
              ? {
                  x: rtl ? W - plotStart - (i + 1) * band : plotStart + i * band,
                  y: TOP,
                  w: band,
                  h: plotHeight,
                }
              : { x: rtl ? endPad : plotStart, y: TOP + i * BAND, w: plotWidth, h: BAND };
            return (
              // Focusable so that the keyboard reaches every value; the tooltip follows focus.
              <g
                key={`${i}-${row.label}`}
                ref={(node) => {
                  barRefs.current[i] = node;
                }}
                tabIndex={i === tabStop ? 0 : -1}
                role="img"
                aria-label={label}
                aria-describedby={active === i ? tipId : undefined}
                onKeyDown={(event) => onBarKey(event, i)}
                onPointerEnter={() => setActive(i)}
                onPointerLeave={() => setActive((a) => (a === i ? null : a))}
                onFocus={() => {
                  setCurrent(i);
                  setActive(i);
                  setFocused(i);
                }}
                onBlur={() => {
                  setActive((a) => (a === i ? null : a));
                  setFocused(null);
                }}
                className="outline-none"
              >
                <title>{label}</title>
                {/* the hit target is the whole band, bigger than the bar */}
                <rect
                  x={hit.x}
                  y={hit.y}
                  width={hit.w}
                  height={hit.h}
                  fill={active === i ? "var(--surface-2)" : "transparent"}
                  stroke={focused === i ? "var(--focus)" : "none"}
                  strokeWidth={focused === i ? 3 : 0}
                  rx={RADIUS}
                />
                <path d={path} fill="var(--chart-1)" />
                {vertical ? (
                  i % every === 0 ? (
                    <text
                      x={hit.x + band / 2}
                      y={H - BOTTOM + 18}
                      textAnchor="middle"
                      fill="var(--ink-2)"
                      direction="ltr"
                    >
                      {row.label}
                    </text>
                  ) : null
                ) : (
                  <text
                    x={fromStart(NAMES - 8)}
                    y={TOP + i * BAND + BAND / 2}
                    dominantBaseline="middle"
                    textAnchor="end"
                    direction={rtl ? "rtl" : "ltr"}
                    fill="var(--ink)"
                  >
                    {/* U+2068 FIRST STRONG ISOLATE … U+2069 POP DIRECTIONAL ISOLATE: a French name
                        in an Arabic chart keeps its "…" at its own end */}
                    {`\u2068${cut(row.label, NAMES - 16)}\u2069`}
                  </text>
                )}
              </g>
            );
          })}
          {/* the baseline */}
          {vertical ? (
            <line
              x1={fromStart(plotStart)}
              x2={fromStart(W - endPad)}
              y1={valueY(0)}
              y2={valueY(0)}
              stroke="var(--ink-2)"
              strokeWidth={1}
            />
          ) : (
            <line
              x1={valueX(0)}
              x2={valueX(0)}
              y1={TOP}
              y2={H - BOTTOM}
              stroke="var(--ink-2)"
              strokeWidth={1}
            />
          )}
        </svg>
        {tip ? (
          <div
            id={tipId}
            role="tooltip"
            className="pointer-events-none absolute z-10 flex flex-col rounded-control border border-line bg-surface px-3 py-2 text-sm shadow-[0_12px_32px_rgba(28,25,23,.18)]"
            style={{
              insetInlineStart: `${((rtl ? W - tip.cx : tip.cx) / W) * 100}%`,
              top: `${(tip.cy / H) * 100}%`,
              translate: `${rtl ? "50%" : "-50%"} calc(-100% - 8px)`,
            }}
          >
            <bdi dir="ltr" className="font-bold tabular-nums">
              {formatValue(tip.row.value)}
            </bdi>
            <span className="text-ink-2">{tip.row.label}</span>
          </div>
        ) : null}
      </div>
      <table className="sr-only">
        <tbody>
          {rows.map((row, i) => (
            <tr key={`${i}-${row.label}`}>
              <th scope="row">{row.label}</th>
              <td>{formatValue(row.value)}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </figure>
  );
}
