import { render, waitFor } from "@testing-library/react";
import { emit } from "@tauri-apps/api/event";
import { describe, expect, it } from "vitest";
import { mockCommands } from "@/test/render";
import invoiceFr from "@/documents/__fixtures__/invoice.fr.json";
import { PrintHostApp, pageRule } from "./PrintHostApp";

const job = {
  jobId: "j1",
  lang: "fr",
  page: { template: "document", model: invoiceFr },
  marginMm: 12,
  sizeMm: [210, 297],
  demoMark: null,
};

describe("print host page", () => {
  it("renders the pending job and says so with its height", async () => {
    const rendered: unknown[] = [];
    let served = false;
    mockCommands({
      print_host_next: () => {
        if (served) return null;
        served = true;
        return job;
      },
      print_host_rendered: (args) => {
        rendered.push(args);
        return null;
      },
    });
    const { container } = render(<PrintHostApp />);
    await waitFor(() => expect(rendered).toHaveLength(1));
    expect(rendered[0]).toMatchObject({ jobId: "j1", result: { ok: true } });
    expect(container.textContent).toContain("FA2026-000045");
    // The sheet and margins of WebView2's settings, as a CSS rule too (the page numbers' room).
    expect(container.querySelector("style")?.textContent).toBe("@page{size:210mm 297mm;margin:12mm}");
    expect(container.firstElementChild).toHaveClass("print-root");
    expect(document.documentElement.dir).toBe("ltr");
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("prints the demo shop's mark on its pages (finding 39)", async () => {
    const rendered: unknown[] = [];
    let served = false;
    mockCommands({
      print_host_next: () => {
        if (served) return null;
        served = true;
        return { ...job, jobId: "demo", demoMark: "DÉMO – sans valeur fiscale" };
      },
      print_host_rendered: (args) => {
        rendered.push(args);
        return null;
      },
    });
    const { container } = render(<PrintHostApp />);
    await waitFor(() => expect(rendered).toHaveLength(1));
    expect(container.querySelector(".doc-demo-mark")?.textContent).toBe("DÉMO – sans valeur fiscale");
  });

  it("pulls again when woken and reports a template that fails", async () => {
    const rendered: Array<{ result: { ok: boolean } }> = [];
    let calls = 0;
    mockCommands({
      print_host_next: () => {
        calls += 1;
        return calls === 2 ? { jobId: "bad", lang: "fr", page: { template: "document", model: {} } } : null;
      },
      print_host_rendered: (args) => {
        rendered.push(args as { result: { ok: boolean } });
        return null;
      },
    });
    render(<PrintHostApp />);
    await waitFor(() => expect(calls).toBe(1));
    await emit("print-host-wake", { jobId: "bad" });
    await waitFor(() => expect(rendered).toHaveLength(1));
    expect(rendered[0]?.result.ok).toBe(false);
    await new Promise((resolve) => setTimeout(resolve, 50));
    expect(rendered).toHaveLength(1);
  });
});

describe("the job's page rule", () => {
  it("gives a receipt its margins only, its length known once rendered", () => {
    expect(pageRule({ marginMm: 3, sizeMm: null })).toBe("@page{margin:3mm}");
    expect(pageRule({ marginMm: 8, sizeMm: [210, 148] })).toBe("@page{size:210mm 148mm;margin:8mm}");
  });
});
