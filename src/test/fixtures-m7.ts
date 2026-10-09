import type {
  AttemptView,
  CloudNotice,
  FailedItem,
  OnlineOrder,
  OnlineOrderSummary,
  PublishRow,
  SyncStatus,
} from "@/lib/ipc/types";

export const unlinkedStatus: SyncStatus = {
  linked: false,
  linkStatus: null,
  storeName: null,
  publicUrl: null,
  online: false,
  paused: false,
  running: false,
  lastCycleAt: null,
  lastSuccessAt: null,
  pending: 0,
  failed: 0,
  entities: [],
  notReady: [],
  openNotices: 0,
  unseenOrders: 0,
  eventsLive: false,
  mainPc: null,
};

export const linkedStatus: SyncStatus = {
  ...unlinkedStatus,
  linked: true,
  linkStatus: "linked",
  storeName: "Boutique Amel",
  publicUrl: "https://amel.dzbuild.app",
  online: true,
  running: true,
  lastCycleAt: 1_790_596_800_000,
  lastSuccessAt: 1_790_596_800_000,
  pending: 3,
  entities: [
    { kind: "pos_sale", pending: 2, failed: 0, lastSentAt: 1_790_596_700_000 },
    { kind: "product_upsert", pending: 1, failed: 1, lastSentAt: null },
  ],
  notReady: ["pos.closures"],
  openNotices: 1,
};

/** A LAN till: the link and its settings are the main PC's. */
export const tillStatus: SyncStatus = { ...unlinkedStatus, mainPc: "Supérette El Amel" };

export const waitingAttempt: AttemptView = {
  id: "att_1",
  kind: "browser",
  state: { state: "waiting" },
  authorizeUrl: "https://dzbuild.com/oauth/authorize?client_id=dzbuild-pos-windows",
  device: null,
  expiresAt: 1_790_597_400_000,
};

export const chooseAttempt: AttemptView = {
  ...waitingAttempt,
  state: {
    state: "chooseStore",
    stores: [
      { storeId: "141", storeName: "Boutique Amel", plan: "pro", role: "owner", publicUrl: null },
      { storeId: "152", storeName: "Amel Cosmétiques", plan: "free", role: "owner", publicUrl: null },
    ],
  },
};

export const notReadyNotice: CloudNotice = {
  id: "n1",
  kind: "not_ready",
  code: "cloud_not_ready",
  subject: "pos.closures",
  details: {},
  count: 3,
  firstAt: 1_790_596_000_000,
  lastAt: 1_790_596_800_000,
  dismissed: false,
};

export const failedItem: FailedItem = {
  id: "f1",
  kind: "product_upsert",
  entityId: "p1",
  label: "Sac cuir",
  code: "validation_failed",
  details: { status: 422, message: "price must be positive" },
  attempts: 1,
  createdAt: 1_790_596_000_000,
};

export const publishRows: PublishRow[] = [
  {
    productId: "p1",
    name: "Lait Candia 1 L",
    reference: null,
    price: 11_000,
    publish: true,
    onlineStatus: "active",
    remoteId: "88121",
    hasImage: true,
    archived: false,
  },
  {
    productId: "p2",
    name: "Huile Elio 5 L",
    reference: "ELIO5",
    price: 95_000,
    publish: false,
    onlineStatus: "active",
    remoteId: null,
    hasImage: false,
    archived: false,
  },
];

export const orderSummary: OnlineOrderSummary = {
  id: "o1",
  orderNumber: "1042",
  remoteStatus: "confirmed",
  localState: "new",
  deliveryType: "pickup",
  paymentMethod: "cod",
  paymentStatus: null,
  customerName: "Samira B.",
  wilayaId: 16,
  total: 20_000,
  items: 1,
  claimedByMe: false,
  claimedElsewhere: false,
  claimConfirmed: false,
  remoteCreatedAt: 1_790_590_000_000,
  seen: false,
};

export const order: OnlineOrder = {
  id: "o1",
  remoteId: "9001",
  orderNumber: "1042",
  remoteStatus: "confirmed",
  localState: "new",
  deliveryType: "pickup",
  paymentMethod: "cod",
  paymentStatus: null,
  customerName: "Samira B.",
  customerPhone: "0550123456",
  wilayaId: 16,
  commune: "Bab Ezzouar",
  address: null,
  customerNotes: "après 17h",
  subtotal: 20_000,
  shippingCost: 0,
  discount: 0,
  total: 20_000,
  claimedByMe: false,
  claimedElsewhere: false,
  claimConfirmed: false,
  localDocumentId: null,
  localDocumentNumber: null,
  refuseReason: null,
  remoteCreatedAt: 1_790_590_000_000,
  lines: [
    {
      lineNo: 1,
      productId: "p1",
      productName: "Lait",
      variantName: null,
      sku: null,
      unitPrice: 9_000,
      qty: 2_000,
      total: 18_000,
      localPrice: 10_000,
    },
  ],
};
