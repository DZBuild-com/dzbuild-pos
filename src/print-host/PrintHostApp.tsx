import { Component, type ReactNode, useEffect, useLayoutEffect, useRef, useState } from "react";
import { DocumentPage, pageDirection } from "@/documents/page";
import { printHost } from "@/lib/ipc/print-host";
import type { HostJob } from "@/lib/ipc/types";

/** Catches a template that throws: the job is answered as failed instead of waiting for the timeout. */
class Guard extends Component<{ onError: (e: Error) => void; children: ReactNode }, { failed: boolean }> {
  override state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  override componentDidCatch(error: Error) {
    this.props.onError(error);
  }
  override render() {
    return this.state.failed ? null : this.props.children;
  }
}

/** The job's `@page` rule: the size and margin of WebView2's print settings for it (Rust
 * `PageSetup`), so the sheet is the same whichever of the two Chromium honours, and the margin
 * boxes (the page numbers) always have the bottom margin to print in. A receipt has no size: its
 * length is known once rendered. */
export function pageRule(job: Pick<HostJob, "marginMm" | "sizeMm">): string {
  const size = job.sizeMm ? `size:${job.sizeMm[0]}mm ${job.sizeMm[1]}mm;` : "";
  return `@page{${size}margin:${job.marginMm}mm}`;
}

const frame = () => new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));

/** Fonts loaded, every image decoded and two frames laid out: what WebView2 prints is final. */
async function settled(root: HTMLElement) {
  // Laying the page out starts the loads of the fonts it uses; `fonts.ready` waits for them after.
  root.getBoundingClientRect();
  await frame();
  await document.fonts?.ready;
  await Promise.all(
    Array.from(root.querySelectorAll("img")).map((img) =>
      typeof img.decode === "function" ? img.decode().catch(() => undefined) : undefined,
    ),
  );
  await frame();
  await frame();
}

/** The hidden print window's page (spec §9.2): it pulls the one job Rust offers, on load and on
 * each `print-host-wake`, renders its template on white paper and says when it is laid out. */
export function PrintHostApp() {
  const [job, setJob] = useState<HostJob | null>(null);
  const root = useRef<HTMLDivElement>(null);
  /** The job whose template threw: it is reported once, as a failure. */
  const failed = useRef<string | null>(null);

  useEffect(() => {
    let alive = true;
    const pull = async () => {
      try {
        const next = await printHost.next();
        if (alive && next) setJob(next);
      } catch (error) {
        // Rust times the job out and frees the host; the page only logs why it got nothing.
        console.error("print host: no job", error);
      }
    };
    // Unsubscribed during the cleanup itself once the listener is in place (a later microtask
    // may find the event bridge already torn down), or as soon as it arrives after an unmount.
    let stop: (() => void) | undefined;
    printHost
      .onWake(() => void pull())
      .then((unlisten) => {
        if (alive) stop = unlisten;
        else unlisten();
      })
      .catch((error: unknown) => {
        // Without the wake-up the page still takes the job offered when it loads.
        console.error("print host: wake-up listener refused", error);
      });
    void pull();
    return () => {
      alive = false;
      stop?.();
    };
  }, []);

  useLayoutEffect(() => {
    if (!job) return;
    const { lang, dir } = pageDirection(job);
    document.documentElement.lang = lang;
    document.documentElement.dir = dir;
    document.documentElement.dataset.theme = "light";
    const element = root.current;
    if (!element) return;
    void settled(element)
      .then(() => {
        if (failed.current === job.jobId) return;
        return printHost.rendered(job.jobId, {
          ok: true,
          error: null,
          heightPx: Math.ceil(element.scrollHeight),
        });
      })
      .catch((error: unknown) => console.error("print host: rendered not delivered", error));
  }, [job]);

  if (!job) return null;
  return (
    <div ref={root} className="print-root">
      <style>{pageRule(job)}</style>
      <Guard
        key={job.jobId}
        onError={(error) => {
          failed.current = job.jobId;
          void printHost
            .rendered(job.jobId, { ok: false, error: error.message, heightPx: 0 })
            .catch((failure: unknown) => console.error("print host: failure not delivered", failure));
        }}
      >
        <DocumentPage page={job.page} />
      </Guard>
      {/* Finding 39: a page of the demo shop can never pass for a real document. Fixed on a
          sheet, so Chromium repeats it on every printed page; in the flow of a receipt, whose
          length is measured. */}
      {job.demoMark ? (
        <p
          className={
            job.page.template === "receipt" || job.page.template === "receipt_roll"
              ? "doc-demo-mark doc-demo-inline"
              : "doc-demo-mark"
          }
          role="note"
        >
          {job.demoMark}
        </p>
      ) : null}
    </div>
  );
}
