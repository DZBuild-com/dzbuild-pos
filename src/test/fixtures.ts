import type {
  AppInfo,
  AuditRow,
  BackupInfo,
  BackupStatus,
  Company,
  Permission,
  PermissionGroupInfo,
  Role,
  Session,
  UpdateStatus,
  User,
  UserTile,
} from "@/lib/ipc/types";

export const OWNER_ROLE_ID = "00000000-0000-7000-8000-000000000001";
export const MANAGER_ROLE_ID = "00000000-0000-7000-8000-000000000002";
export const CASHIER_ROLE_ID = "00000000-0000-7000-8000-000000000003";
export const STOREKEEPER_ROLE_ID = "00000000-0000-7000-8000-000000000004";

export const ALL_PERMISSIONS: Permission[] = [
  "sales.create",
  "sales.discount",
  "sales.price_override",
  "sales.void_line",
  "sales.hold",
  "sales.return",
  "sales.reprint",
  "sales.view_all",
  "cash.open",
  "cash.close",
  "cash.paid_in_out",
  "cash.view_expected",
  "cash.view_history",
  "products.view",
  "products.edit",
  "products.price",
  "products.import",
  "products.labels",
  "stock.view",
  "stock.adjust",
  "stock.count",
  "stock.transfer",
  "purchases.view",
  "purchases.manage",
  "suppliers.manage",
  "customers.view",
  "customers.manage",
  "credit.view",
  "credit.collect",
  "credit.limit_override",
  "pharmacy.register",
  "documents.invoice",
  "documents.quote",
  "documents.delivery",
  "documents.credit_note",
  "reports.sales",
  "reports.stock",
  "reports.profit",
  "reports.cash",
  "reports.tax",
  "settings.company",
  "settings.users",
  "settings.hardware",
  "settings.data",
  "settings.dzbuild",
  "audit.view",
];

export const CASHIER_PERMISSIONS: Permission[] = [
  "sales.create",
  "sales.hold",
  "sales.reprint",
  "cash.open",
  "cash.close",
  "cash.paid_in_out",
  "products.view",
  "stock.view",
  "customers.view",
  "credit.view",
  "credit.collect",
];

export const ownerSession: Session = {
  userId: "u-owner",
  username: "amine.benali",
  displayName: "Amine Benali",
  roleId: OWNER_ROLE_ID,
  roleNameAr: "المالك",
  roleNameFr: "Propriétaire",
  roleNameEn: "Owner",
  isOwner: true,
  lang: "fr",
  permissions: ALL_PERMISSIONS,
  discountLimitBp: 10_000,
  terminalId: "t-1",
  terminalCode: "C1",
  startedAt: 1_790_596_800_000,
};

export const cashierSession: Session = {
  ...ownerSession,
  userId: "u-cashier",
  username: "yacine",
  displayName: "Yacine",
  roleId: CASHIER_ROLE_ID,
  roleNameAr: "أمين الصندوق",
  roleNameFr: "Caissier",
  roleNameEn: "Cashier",
  isOwner: false,
  permissions: CASHIER_PERMISSIONS,
  discountLimitBp: 0,
};

/** The built-in storekeeper role (pos-core `storekeeper_permissions`): products, stock, purchases
 * and the stock reports, without costs (`reports.profit`), sales or cash. */
export const storekeeperSession: Session = {
  ...ownerSession,
  userId: "u-storekeeper",
  username: "sofiane",
  displayName: "Sofiane Hadj",
  roleId: STOREKEEPER_ROLE_ID,
  roleNameAr: "أمين المخزن",
  roleNameFr: "Magasinier",
  roleNameEn: "Storekeeper",
  isOwner: false,
  permissions: [
    "products.view",
    "products.edit",
    "products.import",
    "products.labels",
    "stock.view",
    "stock.adjust",
    "stock.count",
    "stock.transfer",
    "purchases.view",
    "purchases.manage",
    "suppliers.manage",
    "reports.stock",
  ],
  discountLimitBp: 0,
};

/** The built-in manager role: everything but the shop, users, data and DZBuild settings. */
export const managerSession: Session = {
  ...ownerSession,
  userId: "u-manager",
  username: "karima",
  displayName: "Karima Saadi",
  roleId: MANAGER_ROLE_ID,
  roleNameAr: "المسيّر",
  roleNameFr: "Gérant",
  roleNameEn: "Manager",
  isOwner: false,
  permissions: ALL_PERMISSIONS.filter(
    (p) => !["settings.company", "settings.users", "settings.data", "settings.dzbuild"].includes(p),
  ),
};

export const ownerTile: UserTile = {
  id: "u-owner",
  displayName: "Amine Benali",
  roleId: OWNER_ROLE_ID,
  roleNameAr: "المالك",
  roleNameFr: "Propriétaire",
  roleNameEn: "Owner",
  lang: "fr",
};

export const settingsValues: Record<string, unknown> = {
  "ui.theme": "system",
  "ui.login_lang": "fr",
  "security.auto_lock_minutes": 0,
  "shop.activity": "general",
  "numbering.restart_yearly": true,
  // M1 and M2 rules at their backend defaults (pos-core `settings::REGISTRY`).
  "stamp_duty.mode": "charge_customer",
  "stock.negative_policy": "warn",
  "sales.cash_rounding": 0,
  "cash.variance_threshold": 50_000,
  "cash.blind_close": false,
  "sales.max_change": 0,
  "payments.cheque_requires_customer": false,
  "stock.expired_lots": "warn",
  "stock.expiry_alert_days": 30,
  "promotions.soldes_windows": [],
  "products.internal_ean_prefix": "200",
  "company.nin": "",
  "company.social": {},
  // M6 backups, updates and support at their backend defaults.
  "backup.keep_daily": 14,
  "backup.keep_weekly": 8,
  "backup.keep_monthly": 12,
  "backup.on_close": true,
  "updates.channel": "stable",
  "updates.auto_check": true,
  "updates.auto_download": true,
  "support.contact": { url: "https://dzbuild.com/pos/support" },
};

export const packFixture = {
  nifLengths: [15, 18],
  wilayas: [
    { code: "16", nameAr: "الجزائر", nameFr: "Alger", nameEn: "Algiers", since: 1974, splitFrom: null },
    { code: "31", nameAr: "وهران", nameFr: "Oran", nameEn: "Oran", since: 1974, splitFrom: null },
  ],
};

export const companyFixture: Company = {
  nameAr: "",
  nameFr: "Supérette El Amel",
  nameEn: "",
  legalForm: null,
  capital: null,
  address: null,
  wilaya: "16",
  phone: null,
  email: null,
  rc: null,
  nif: null,
  nis: null,
  ai: null,
  rib: null,
  taxRegime: "ifu",
  priceMode: "ttc",
  receiptHeaderAr: null,
  receiptHeaderFr: null,
  receiptHeaderEn: null,
  receiptFooterAr: null,
  receiptFooterFr: null,
  receiptFooterEn: null,
  defaultDocLang: "fr",
  hasLogo: false,
  updatedAt: 1_790_596_800_000,
};

export const rolesFixture: Role[] = [
  {
    id: OWNER_ROLE_ID,
    code: "owner",
    nameAr: "المالك",
    nameFr: "Propriétaire",
    nameEn: "Owner",
    isSystem: true,
    isOwner: true,
    permissions: ALL_PERMISSIONS,
    userCount: 1,
    activeUserCount: 1,
  },
  {
    id: CASHIER_ROLE_ID,
    code: "cashier",
    nameAr: "أمين الصندوق",
    nameFr: "Caissier",
    nameEn: "Cashier",
    isSystem: true,
    isOwner: false,
    permissions: CASHIER_PERMISSIONS,
    userCount: 0,
    activeUserCount: 0,
  },
];

export const catalogFixture: PermissionGroupInfo[] = [
  { group: "sales", permissions: ALL_PERMISSIONS.filter((p) => p.startsWith("sales.")) },
  { group: "audit", permissions: ["audit.view"] },
];

export const userFixture: User = {
  id: "u-owner",
  username: "amine.benali",
  displayName: "Amine Benali",
  roleId: OWNER_ROLE_ID,
  roleNameAr: "المالك",
  roleNameFr: "Propriétaire",
  roleNameEn: "Owner",
  isOwner: true,
  lang: "fr",
  active: true,
  discountLimitBp: 10_000,
  lastLoginAt: 1_790_596_800_000,
  createdAt: 1_790_596_800_000,
};

export const backupFixture: BackupInfo = {
  fileName: "pos-20260928-130000-000-manual.db",
  path: "C:\\Users\\shop\\AppData\\Roaming\\dev.dzbuild.pos\\backups\\pos-20260928-130000-000-manual.db",
  at: 1_790_596_800_000,
  size: 524_288,
  kind: "manual",
};

/** A healthy backup state: no failure, a recent backup, no damage, the second copy fine. */
export const backupStatusFixture: BackupStatus = {
  lastAuto: null,
  newestAt: 1_790_593_200_000,
  stale: false,
  databaseDamaged: false,
  mirrorError: null,
};

/** Up to date: checked, nothing offered, nothing downloaded, no drawer open. */
export const updateStatusFixture: UpdateStatus = {
  currentVersion: "0.6.0",
  channel: "stable",
  autoCheck: true,
  autoDownload: true,
  lastCheckAt: 1_790_593_200_000,
  lastCheckError: null,
  offer: null,
  download: { kind: "idle" },
  blockers: [],
  installing: false,
};

export const auditRowFixture: AuditRow = {
  seq: 1,
  id: "a-1",
  at: 1_790_596_800_000,
  userId: "u-owner",
  terminalId: "t-1",
  action: "setup.completed",
  entityType: "company",
  entityId: "1",
  payload: { lang: "fr" },
  prevHash: "0".repeat(64),
  hash: "f".repeat(64),
  userName: "Amine Benali",
};

export const appInfoFixture: AppInfo = {
  name: "DZBuild POS",
  version: "0.1.0",
  identifier: "dev.dzbuild.pos",
  paths: {
    dataDir: "C:\\data",
    dbPath: "C:\\data\\pos.db",
    backupsDir: "C:\\data\\backups",
    logsDir: "C:\\data\\logs",
    hardwareConfig: "C:\\data\\hardware.json",
    media: "C:\\data\\media",
    localConfig: "C:\\data\\local.json",
    demo: false,
  },
  schemaVersion: 1,
  os: "windows 10.0.26200 (x86_64)",
  terminalCode: "C1",
  webviewVersion: "141.0.3537.71",
  coldStartMs: 1840,
};
