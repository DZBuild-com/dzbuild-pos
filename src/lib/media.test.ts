import { describe, expect, it, vi } from "vitest";

vi.mock("@tauri-apps/api/core", () => ({
  convertFileSrc: (p: string) => `http://asset.localhost/${encodeURIComponent(p)}`,
}));

import { mediaUrl } from "@/lib/media";

describe("mediaUrl", () => {
  it("serves only names under media/ and refuses anything else", () => {
    const name = "products/01923f4e-8a1b-7c2d-9e3f-4a5b6c7d8e9f-1790593200000.jpg";
    expect(mediaUrl("C:\\Users\\a\\AppData\\Roaming\\dz.pos", name, true)).toBe(
      `http://asset.localhost/${encodeURIComponent("C:\\Users\\a\\AppData\\Roaming\\dz.pos\\media\\products\\01923f4e-8a1b-7c2d-9e3f-4a5b6c7d8e9f-1790593200000.thumb.jpg")}`,
    );
    expect(mediaUrl("C:\\data", "../pos.db")).toBeNull();
    expect(mediaUrl("C:\\data", null)).toBeNull();
  });
});
