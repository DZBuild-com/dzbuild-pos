// Generates every DZBuild POS brand asset (SVG + transparent RGBA PNG) in this folder.
// Run from anywhere, offline: `node assets/brand/gen-logo.mjs`. Output is deterministic.
//
// The mark is the DZBuild mark (yellow circle, smiling shopping bag) whose bottom edge
// becomes a torn receipt. Text is set with the vendored Inter font through fontkit
// (real GPOS kerning + the cv05 tailed "l" of the parent DZBuild wordmark) and written
// as outlined paths, so no SVG depends on an installed font.
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { openSync } from "fontkit";
import { Resvg } from "@resvg/resvg-js";

const HERE = dirname(fileURLToPath(import.meta.url));

// ---------------------------------------------------------------------------
// Brand tokens: the one place for colours, font and proportions.
// ---------------------------------------------------------------------------
const COLOR = { yellow: "#FFC300", ink: "#1C1917", white: "#FFFFFF" };

const FONT = openSync(join(HERE, "fonts", "Inter-ExtraBold.ttf"));
const FONT_FEATURES = ["kern", "cv05"]; // cv05 = tailed "l", as in the parent wordmark
const WORD_TRACKING = -0.05; // em; tight like the parent DZBuild wordmark
const POS_TRACKING = 0.01; // em; a hair open so the short all-caps word breathes in the pill
// Extra space (em) between two glyphs, by glyph name (cv05 maps "l" to "l.ss02"). Under the
// tight tracking the tail of "l" almost touches the bowl of "d"; this restores the clearance
// ratio the font designer intended.
const PAIR_SPACING = { "l.ss02 d": 0.015 };

// Pill, in multiples of the "DZBuild" cap height (radius: fraction of the pill height).
const PILL = { height: 1.18, cap: 0.56, padX: 0.36, gap: 0.3, radius: 0.27 };

// Vertical logo: square canvas, equal padding, mark above the lockup (gap in cap heights).
const VERTICAL = { size: 1024, pad: 64, gap: 0.5 };
// Horizontal logo: fixed height, width fits; cap = share of the mark diameter, gap in caps.
const HORIZONTAL = { height: 320, pad: 24, cap: 0.38, gap: 0.5 };

// Colour of each lockup part per surface.
const ON_LIGHT = { word: COLOR.yellow, pill: COLOR.ink, pillText: COLOR.white };
const ON_DARK = { word: COLOR.yellow, pill: COLOR.yellow, pillText: COLOR.ink };

// Mark stroke widths and receipt perforation dots in the 512 design box. The icon set
// keeps every stroke and dot >= 16 so the face survives the 32/48 px Windows icon sizes.
const MARK_STROKES = { handle: 16, smile: 13, perforation: { r: 4, count: 7, pitch: 17 } };
const ICON_STROKES = { handle: 20, smile: 18, perforation: { r: 8, count: 5, pitch: 26 } };

const MARK_BOX = 512; // design box of the mark
const MARK_DIAMETER = 480; // circle diameter inside that box (r = 240)

// ---------------------------------------------------------------------------
// Mark
// ---------------------------------------------------------------------------
function zigzag(x0, x1, yTop, depth, teeth) {
  const w = (x1 - x0) / teeth;
  const pts = [];
  for (let i = teeth; i >= 0; i--) {
    const x = x0 + i * w;
    pts.push(`L${x.toFixed(1)} ${yTop}`);
    if (i > 0) pts.push(`L${(x - w / 2).toFixed(1)} ${yTop + depth}`);
  }
  return pts.join(" ");
}

// Bag-receipt inside the 512 box, slightly tapered like the DZBuild bag.
function bagReceipt(strokes) {
  const topY = 196, botY = 392, r = 18;
  const tl = 168, tr = 344; // top corners
  const bl = 150, br = 362; // bottom corners (wider)
  const body =
    `M${tl + r} ${topY} L${tr - r} ${topY} Q${tr} ${topY} ${tr + 2} ${topY + r} ` +
    `L${br} ${botY} ` +
    zigzag(bl, br, botY, 16, 8) +
    ` L${tl - 2} ${topY + r} Q${tl} ${topY} ${tl + r} ${topY} Z`;
  const handle = `M214 ${topY + 2} V178 a42 42 0 0 1 84 0 V${topY + 2}`;
  return [
    `<path d="${handle}" fill="none" stroke="${COLOR.white}" stroke-width="${strokes.handle}" stroke-linecap="round"/>`,
    `<path d="${body}" fill="${COLOR.white}"/>`,
    `<circle cx="222" cy="262" r="12" fill="${COLOR.yellow}"/>`,
    `<circle cx="290" cy="262" r="12" fill="${COLOR.yellow}"/>`,
    `<path d="M206 298 Q256 340 306 298" fill="none" stroke="${COLOR.yellow}" stroke-width="${strokes.smile}" stroke-linecap="round"/>`,
    ...perforation(strokes.perforation),
  ];
}

// Row of dots across the receipt, centred on the bag axis (x = 256).
function perforation({ r, count, pitch }) {
  const first = 256 - ((count - 1) * pitch) / 2;
  return Array.from(
    { length: count },
    (_, i) => `<circle cx="${fmt(first + i * pitch)}" cy="352" r="${r}" fill="${COLOR.yellow}"/>`,
  );
}

// Mark elements in the 512 box, joined with the given line indent.
function markContent(strokes, indent = "  ") {
  return [
    `<circle cx="256" cy="256" r="${MARK_DIAMETER / 2}" fill="${COLOR.yellow}"/>`,
    ...bagReceipt(strokes),
  ].join(`\n${indent}`);
}

// The mark scaled so its circle has `diameter` and is centred on (cx, cy).
function placedMark(cx, cy, diameter) {
  const s = diameter / MARK_DIAMETER;
  const tx = cx - (MARK_BOX / 2) * s;
  const ty = cy - (MARK_BOX / 2) * s;
  return `<g transform="translate(${fmt(tx)} ${fmt(ty)}) scale(${fmt(s, 6)})">
    ${markContent(MARK_STROKES, "    ")}
  </g>`;
}

// ---------------------------------------------------------------------------
// Type: shaped with fontkit, emitted as outlines
// ---------------------------------------------------------------------------

// Shapes `text` so its cap height is `capPx`. Returns ink metrics relative to the
// ink's left edge (x = 0) and the baseline (y = 0), plus `pathAt` to emit the outline.
function setLine(text, capPx, trackingEm) {
  const scale = capPx / FONT.capHeight;
  const run = FONT.layout(text, FONT_FEATURES);
  const placed = [];
  let pen = 0;
  run.glyphs.forEach((glyph, i) => {
    const pos = run.positions[i];
    placed.push({ glyph, x: pen + pos.xOffset, y: pos.yOffset });
    pen += pos.xAdvance;
    const next = run.glyphs[i + 1];
    if (next) {
      const pair = PAIR_SPACING[`${glyph.name} ${next.name}`] ?? 0;
      pen += (trackingEm + pair) * FONT.unitsPerEm;
    }
  });
  const boxes = placed.map(({ glyph, x, y }) => {
    const b = glyph.path.bbox;
    return { minX: b.minX + x, maxX: b.maxX + x, minY: b.minY + y, maxY: b.maxY + y };
  });
  const minX = Math.min(...boxes.map((b) => b.minX));
  const maxX = Math.max(...boxes.map((b) => b.maxX));
  const maxY = Math.max(...boxes.map((b) => b.maxY));
  const minY = Math.min(...boxes.map((b) => b.minY));
  return {
    width: (maxX - minX) * scale,
    top: -maxY * scale, // negative: above the baseline
    bottom: -minY * scale,
    pathAt(inkLeft, baseline) {
      return placed
        .map(({ glyph, x, y }) =>
          glyph.path
            .transform(scale, 0, 0, -scale, inkLeft + (x - minX) * scale, baseline - y * scale)
            .toSVG(),
        )
        .join("");
    },
  };
}

// "DZBuild" + "POS" pill on one line. Origin: wordmark ink left edge, wordmark baseline.
function lockup(cap, palette) {
  const word = setLine("DZBuild", cap, WORD_TRACKING);
  const pos = setLine("POS", cap * PILL.cap, POS_TRACKING);
  const pillH = cap * PILL.height;
  const pillW = pos.width + 2 * cap * PILL.padX;
  const pillX = word.width + cap * PILL.gap;
  const pillY = -cap / 2 - pillH / 2; // pill centred on the cap band of the wordmark
  const top = Math.min(word.top, pillY);
  const bottom = Math.max(word.bottom, pillY + pillH);
  return {
    width: pillX + pillW,
    top,
    bottom,
    height: bottom - top,
    render(x, baseline) {
      const posBaseline = baseline - cap / 2 + (cap * PILL.cap) / 2; // POS centred on its cap band
      return [
        `<path d="${word.pathAt(x, baseline)}" fill="${palette.word}"/>`,
        `<rect x="${fmt(x + pillX)}" y="${fmt(baseline + pillY)}" width="${fmt(pillW)}" height="${fmt(pillH)}" rx="${fmt(pillH * PILL.radius)}" fill="${palette.pill}"/>`,
        `<path d="${pos.pathAt(x + pillX + (pillW - pos.width) / 2, posBaseline)}" fill="${palette.pillText}"/>`,
      ].join("\n  ");
    },
  };
}

// ---------------------------------------------------------------------------
// Layouts
// ---------------------------------------------------------------------------
function svgDoc(width, height, body, viewBox = `0 0 ${width} ${height}`) {
  return `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}" viewBox="${viewBox}">
  <title>DZBuild POS</title>
  ${body}
</svg>
`;
}

function markSvg() {
  return svgDoc(MARK_BOX, MARK_BOX, markContent(MARK_STROKES));
}

// App icon: circle = 88% of the canvas, heavier strokes for small sizes.
function appIconSvg(size = 1024) {
  const box = MARK_DIAMETER / 0.88;
  const o = MARK_BOX / 2 - box / 2;
  return svgDoc(size, size, markContent(ICON_STROKES), `${fmt(o)} ${fmt(o)} ${fmt(box)} ${fmt(box)}`);
}

// Vertical: mark above the lockup; square content box with equal padding on all sides,
// so the mark diameter is whatever height the full-width lockup leaves.
function verticalSvg(palette) {
  const { size, pad, gap } = VERTICAL;
  const content = size - 2 * pad;
  const cap = content / lockup(1, palette).width; // lockup scales linearly with cap
  const row = lockup(cap, palette);
  const diameter = content - gap * cap - row.height;
  const rowTop = pad + diameter + gap * cap;
  const body = [
    placedMark(size / 2, pad + diameter / 2, diameter),
    row.render(size / 2 - row.width / 2, rowTop - row.top),
  ].join("\n  ");
  return svgDoc(size, size, body);
}

// Horizontal: mark on the start side; the pill (= wordmark cap band) centred on the mark.
function horizontalSvg(palette) {
  const { height, pad } = HORIZONTAL;
  const diameter = height - 2 * pad;
  const cap = HORIZONTAL.cap * diameter;
  const row = lockup(cap, palette);
  const exact = 2 * pad + diameter + HORIZONTAL.gap * cap + row.width;
  const width = Math.ceil(exact);
  const x0 = pad + (width - exact) / 2; // spread the rounding evenly on both sides
  const cy = height / 2;
  const body = [
    placedMark(x0 + diameter / 2, cy, diameter),
    row.render(x0 + diameter + HORIZONTAL.gap * cap, cy + cap / 2),
  ].join("\n  ");
  return svgDoc(width, height, body);
}

// ---------------------------------------------------------------------------
// Output
// ---------------------------------------------------------------------------
function fmt(n, digits = 2) {
  return String(Number(n.toFixed(digits)));
}

function writeSvg(name, svg) {
  writeFileSync(join(HERE, name), svg);
}

// Transparent RGBA PNG; `zoom` multiplies the SVG's own width and height.
function writePng(name, svg, zoom = 1) {
  const png = new Resvg(svg, {
    fitTo: { mode: "zoom", value: zoom },
    font: { loadSystemFonts: false }, // everything is outlined; no font lookup
  })
    .render()
    .asPng();
  writeFileSync(join(HERE, name), png);
}

const mark = markSvg();
writeSvg("dzbuild-pos-mark.svg", mark);
writePng("dzbuild-pos-mark.png", mark);
writePng("dzbuild-pos-mark@1024.png", mark, 2);

writePng("app-icon.png", appIconSvg());

for (const [suffix, palette] of [["", ON_LIGHT], ["-on-dark", ON_DARK]]) {
  const vertical = verticalSvg(palette);
  writeSvg(`dzbuild-pos-logo${suffix}.svg`, vertical);
  writePng(`dzbuild-pos-logo${suffix}.png`, vertical);

  const horizontal = horizontalSvg(palette);
  writeSvg(`dzbuild-pos-logo-horizontal${suffix}.svg`, horizontal);
  writePng(`dzbuild-pos-logo-horizontal${suffix}.png`, horizontal);
}

console.log("brand assets written to", HERE);
