import type { Licence, LicenseStatus } from "@/lib/ipc/types";

/**
 * A device code as the screens show it, with a valid check symbol (the screens never
 * check it, the Rust side does).
 */
export const DEVICE_CODE = "DZPOS-7K3M-QX9R-2HWD-P4T3";

const base: LicenseStatus = {
  state: "unlicensed",
  canWrite: false,
  refusal: "license_required",
  deviceCode: DEVICE_CODE,
  licence: null,
  daysLeft: null,
  warn: false,
  previousDevice: null,
  seller: { seller: "DZBuild", whatsapp: "213550123456", phone: "", url: "https://dzbuild.com/pos" },
  qr: { size: 21, modules: "10".repeat(220) + "1" },
};

export const trialLicence: Licence = {
  id: "0199a0b0-0000-7000-8000-00000000a001",
  kind: "trial",
  issuedAt: 1_790_000_000_000,
  expiresAt: 1_790_604_800_000,
  customer: { name: "Supérette El Amel", phone: "0550 12 34 56" },
  activatedAt: 1_790_000_100_000,
};

export const unlicensedStatus: LicenseStatus = base;

export const activeStatus: LicenseStatus = {
  ...base,
  state: "active",
  canWrite: true,
  refusal: null,
  licence: { ...trialLicence, kind: "lifetime", expiresAt: null },
};

export function trialStatus(daysLeft: number): LicenseStatus {
  return {
    ...base,
    state: "active",
    canWrite: true,
    refusal: null,
    licence: trialLicence,
    daysLeft,
    warn: daysLeft <= 3,
  };
}

export const expiredStatus: LicenseStatus = {
  ...base,
  state: "expired",
  refusal: "license_expired",
  licence: trialLicence,
  daysLeft: 0,
};

export const otherDeviceStatus: LicenseStatus = {
  ...base,
  state: "otherDevice",
  refusal: "license_wrong_device",
  previousDevice: "DZPOS-0A1B-2C3D-4E5F-6G73",
};

export const tamperedStatus: LicenseStatus = { ...base, state: "tampered", refusal: "license_tampered" };
