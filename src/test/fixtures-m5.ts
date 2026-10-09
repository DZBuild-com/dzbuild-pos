import type { ClientStatus, LanStatus, Notice, TillInfo, TillZ } from "@/lib/ipc/types";

export const lanStandaloneFixture: LanStatus = { role: "standalone", hostId: null, host: null, client: null };

export const lanHostStatusFixture: LanStatus = {
  role: "host",
  hostId: "0190a000-0000-7000-8000-0000000000a1",
  host: {
    listening: true,
    port: 7345,
    addresses: ["192.168.1.10"],
    tillsTotal: 2,
    tillsOnline: 1,
    noticesOpen: 1,
    pairingOpenUntil: null,
    error: null,
  },
  client: null,
};

export const clientStatusFixture: ClientStatus = {
  hostName: "Supérette El Amel",
  hostAddress: "192.168.1.10:7345",
  link: "online",
  pending: 0,
  rejected: 0,
  oldestPendingAt: null,
  behindBy: 0,
  appliedSeq: 120,
  lastContactAt: 1_790_000_000_000,
  skewMs: 800,
  lastError: null,
  entriesRefused: 0,
  datapackGap: null,
};

export const lanTillStatusFixture: LanStatus = {
  role: "client",
  hostId: "0190a000-0000-7000-8000-0000000000a1",
  host: null,
  client: clientStatusFixture,
};

export const tillsFixture: TillInfo[] = [
  {
    terminalId: "0190a000-0000-7000-8000-0000000000b2",
    code: "C2",
    name: "Caisse 2",
    online: true,
    lastSeenAt: 1_790_000_000_000,
    lastAddress: "192.168.1.21",
    appVersion: "0.5.0",
    schemaVersion: 6,
    outboxPending: 0,
    skewMs: 1_200,
    lastReplayedSeq: 88,
    revoked: false,
    pairedAt: 1_789_000_000_000,
    journalIntact: true,
    noticesOpen: 0,
    pinsFeed: false,
  },
  {
    terminalId: "0190a000-0000-7000-8000-0000000000b3",
    code: "C3",
    name: "Caisse 3",
    online: false,
    lastSeenAt: 1_789_990_000_000,
    lastAddress: "192.168.1.22",
    appVersion: "0.5.0",
    schemaVersion: 6,
    outboxPending: 4,
    skewMs: 7_200_000,
    lastReplayedSeq: 40,
    revoked: false,
    pairedAt: 1_789_000_000_000,
    journalIntact: false,
    noticesOpen: 2,
    pinsFeed: false,
  },
];

export const tillZFixture: TillZ[] = [
  {
    sessionId: "0190a000-0000-7000-8000-0000000000c1",
    zNumber: "Z2026-C3-000012",
    businessDate: "2026-09-28",
    closedAt: 1_789_990_000_000,
    closedByName: "Amine",
    netTtc: 4_512_000,
    variance: -5_000,
    grandTotal: 98_000_000,
    offlinePending: 4,
  },
];

export const noticesFixture: Notice[] = [
  {
    id: "0190a000-0000-7000-8000-0000000000d1",
    kind: "oversold",
    terminalId: "0190a000-0000-7000-8000-0000000000b3",
    terminalCode: "C3",
    subject: "0190a000-0000-7000-8000-0000000000e1",
    details: { productName: "Lait Candia 1 L", onHand: -2000 },
    createdAt: 1_789_995_000_000,
    acknowledgedAt: null,
    acknowledgedBy: null,
  },
];
