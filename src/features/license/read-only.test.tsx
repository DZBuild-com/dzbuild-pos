import { screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it } from "vitest";
import { i18n, setLanguage } from "@/lib/i18n";
import type { LicenseStatus } from "@/lib/ipc/types";
import { cashierSession, ownerSession } from "@/test/fixtures";
import {
  activeStatus,
  expiredStatus,
  otherDeviceStatus,
  trialStatus,
  unlicensedStatus,
} from "@/test/fixtures-m9";
import { missingKeys, mockCommands, renderWithProviders } from "@/test/render";
import { LicenseBanner } from "./LicenseBanner";
import { LicenseSettings } from "./LicenseSettings";
import { PosLicenseGate } from "./PosLicenseGate";

function withStatus(
  status: LicenseStatus,
  extra: Record<string, (args: Record<string, unknown>) => unknown> = {},
) {
  return mockCommands({ license_status: () => status, ...extra });
}

describe("read-only mode on screen", () => {
  it("the banner warns three days before the end and says what read-only mode keeps", async () => {
    await setLanguage("fr");
    withStatus(trialStatus(5));
    const far = renderWithProviders(<LicenseBanner />);
    await waitFor(() => expect(far.container).toBeEmptyDOMElement());
    far.unmount();

    withStatus(trialStatus(3));
    const near = renderWithProviders(<LicenseBanner />);
    expect(await screen.findByText("Essai : encore 3 jours.")).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Passer à la licence à vie" })).toBeInTheDocument();
    near.unmount();

    withStatus(expiredStatus);
    renderWithProviders(<LicenseBanner />);
    expect(await screen.findByText(/Mode consultation/)).toBeInTheDocument();
    expect(screen.getByText(/La période d'essai de ce PC est terminée/)).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Activer une licence" })).toHaveAttribute(
      "href",
      "/settings/license",
    );
  });

  it("the banner stays away from a valid licence, the demo shop and a failed call, and has no link on the sign-in screen", async () => {
    await setLanguage("fr");
    withStatus(activeStatus);
    const active = renderWithProviders(<LicenseBanner />);
    await waitFor(() => expect(active.container).toBeEmptyDOMElement());
    active.unmount();
    withStatus({ ...activeStatus, state: "demo" });
    const demo = renderWithProviders(<LicenseBanner />);
    await waitFor(() => expect(demo.container).toBeEmptyDOMElement());
    demo.unmount();
    mockCommands({});
    const failed = renderWithProviders(<LicenseBanner />);
    await waitFor(() => expect(failed.container).toBeEmptyDOMElement());
    failed.unmount();
    withStatus(unlicensedStatus);
    renderWithProviders(<LicenseBanner withLink={false} />);
    expect(await screen.findByText(/Mode consultation/)).toBeInTheDocument();
    expect(screen.queryByRole("link")).not.toBeInTheDocument();
  });

  it("the banner names the previous PC when the licence belongs to another one", async () => {
    await setLanguage("fr");
    withStatus(otherDeviceStatus);
    renderWithProviders(<LicenseBanner />);
    expect(await screen.findByText(/DZPOS-0A1B-2C3D-4E5F-6G73/)).toBeInTheDocument();
  });

  it("the till shows a read-only card instead of selling", async () => {
    await setLanguage("fr");
    withStatus(expiredStatus);
    const cashier = renderWithProviders(
      <PosLicenseGate session={cashierSession}>
        <p>la caisse</p>
      </PosLicenseGate>,
    );
    expect(await screen.findByRole("heading", { name: "Caisse en mode consultation" })).toBeInTheDocument();
    expect(screen.queryByText("la caisse")).not.toBeInTheDocument();
    expect(screen.getByText("Demandez au propriétaire d'activer la licence de ce PC.")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Activer une licence" })).not.toBeInTheDocument();
    cashier.unmount();
    renderWithProviders(
      <PosLicenseGate session={ownerSession}>
        <p>la caisse</p>
      </PosLicenseGate>,
    );
    expect(await screen.findByRole("button", { name: "Activer une licence" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Retour au menu" })).toBeInTheDocument();
  });

  it("the till sells with a licence, and while the status is unknown", async () => {
    await setLanguage("fr");
    withStatus(trialStatus(2));
    const licensed = renderWithProviders(
      <PosLicenseGate session={cashierSession}>
        <p>la caisse</p>
      </PosLicenseGate>,
    );
    expect(await screen.findByText("la caisse")).toBeInTheDocument();
    licensed.unmount();
    mockCommands({});
    renderWithProviders(
      <PosLicenseGate session={cashierSession}>
        <p>la caisse</p>
      </PosLicenseGate>,
    );
    expect(await screen.findByText("la caisse")).toBeInTheDocument();
  });

  it("Settings › Licence shows the licence and lets the owner remove it with the PIN", async () => {
    await setLanguage("fr");
    const pins: unknown[] = [];
    const calls = withStatus(trialStatus(5), {
      license_remove: ({ pin }) => {
        pins.push(pin);
        return unlicensedStatus;
      },
    });
    renderWithProviders(<LicenseSettings session={ownerSession} />);
    expect(await screen.findByText("Licence de ce PC")).toBeInTheDocument();
    expect(screen.getByText("Essai")).toBeInTheDocument();
    expect(screen.getByText("Supérette El Amel")).toBeInTheDocument();
    expect(screen.getByText(/5 jours restants/)).toBeInTheDocument();
    expect(screen.getByTestId("device-code")).toBeInTheDocument();
    await userEvent.click(screen.getByRole("button", { name: "Retirer la licence de ce PC" }));
    const dialog = await screen.findByRole("dialog");
    await userEvent.type(screen.getByLabelText("Votre code PIN"), "2580");
    await userEvent.click(screen.getByRole("button", { name: "Retirer la licence" }));
    await waitFor(() => expect(calls).toContain("license_remove"));
    expect(pins).toEqual(["2580"]);
    await waitFor(() => expect(dialog).not.toBeInTheDocument());
  });

  it("a cashier sees the licence but cannot remove it", async () => {
    await setLanguage("fr");
    withStatus(activeStatus);
    renderWithProviders(<LicenseSettings session={cashierSession} />);
    expect(await screen.findByText("À vie")).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Retirer la licence de ce PC" })).not.toBeInTheDocument();
  });
});

describe.each(["ar", "fr", "en"] as const)("read-only mode in %s", (lang) => {
  it("the banner, the till's card and Settings › Licence have every text", async () => {
    await setLanguage(lang);
    missingKeys.clear();
    withStatus(expiredStatus);
    renderWithProviders(
      <>
        <LicenseBanner />
        <PosLicenseGate session={ownerSession}>
          <p>till</p>
        </PosLicenseGate>
        <LicenseSettings session={ownerSession} />
      </>,
    );
    expect(await screen.findByText(i18n.t("license:till.title"))).toBeInTheDocument();
    expect(await screen.findByText(i18n.t("license:settings.title"))).toBeInTheDocument();
    expect([...missingKeys]).toEqual([]);
  });
});
