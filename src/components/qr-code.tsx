import { qrPath } from "@/documents/parts";
import type { QrMatrix } from "@/lib/ipc/types";

/** A QR code drawn from the backend's module matrix: one SVG path, a 4-module quiet zone, black on
 * white in both themes (a phone reads it off the screen), crisp at any size. */
export function QrCode({ matrix, size = 176, label }: { matrix: QrMatrix; size?: number; label: string }) {
  const view = matrix.size + 8;
  return (
    <svg
      role="img"
      aria-label={label}
      width={size}
      height={size}
      viewBox={`-4 -4 ${view} ${view}`}
      shapeRendering="crispEdges"
      className="shrink-0 rounded-control bg-white"
    >
      <path d={qrPath(matrix)} fill="#000" />
    </svg>
  );
}
