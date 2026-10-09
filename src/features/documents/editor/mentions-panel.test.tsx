import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderWithProviders } from "@/test/render";
import { mentionsCompleteFixture } from "@/test/fixtures-m3";
import { MentionsPanel } from "./MentionsPanel";

describe("mandatory mentions panel", () => {
  it("sends the activity and the AE card to the documents' identity card", () => {
    renderWithProviders(
      <MentionsPanel
        check={{ ...mentionsCompleteFixture, sellerMissing: ["activity", "rc"] }}
        docType="invoice"
        customerId="c-amine"
      />,
    );
    expect(screen.getByRole("link", { name: "Compléter l'identité des documents" })).toHaveAttribute(
      "href",
      "/settings/documents#documents-identity",
    );
    // The RC itself is typed on the shop's screen: both screens are offered.
    expect(screen.getByRole("link", { name: "Compléter la boutique" })).toHaveAttribute(
      "href",
      "/settings/shop",
    );
  });

  it("offers only the screen that holds what is missing", () => {
    const { unmount } = renderWithProviders(
      <MentionsPanel
        check={{ ...mentionsCompleteFixture, sellerMissing: ["activity"] }}
        docType="invoice"
        customerId={null}
      />,
    );
    expect(screen.getByRole("link", { name: "Compléter l'identité des documents" })).toBeInTheDocument();
    expect(screen.queryByRole("link", { name: "Compléter la boutique" })).not.toBeInTheDocument();
    unmount();
    renderWithProviders(
      <MentionsPanel
        check={{ ...mentionsCompleteFixture, sellerMissing: ["nis"] }}
        docType="invoice"
        customerId={null}
      />,
    );
    expect(screen.getByRole("link", { name: "Compléter la boutique" })).toBeInTheDocument();
    expect(
      screen.queryByRole("link", { name: "Compléter l'identité des documents" }),
    ).not.toBeInTheDocument();
  });
});
