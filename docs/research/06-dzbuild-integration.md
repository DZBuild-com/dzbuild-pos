# 06 — DZBuild (dzbuild.dev) platform, OAuth/API, and the recommended POS integration

> Research date: 2026-09-28. Every factual claim below links to its source. Anything I could not verify is marked **(unverified)**. Endpoints and claims marked **PROPOSED** are my design, not something DZBuild offers today.
>
> Method: I fetched the dzbuild.dev docs site in full (`/llms-full.txt`, 141 KB), the OpenAPI 3.1 file (`/openapi/dzbuild-v1.json`, 585 KB, parsed programmatically), the agent skill, robots/sitemaps, the dzbuild.com merchant docs (Markdown twins), pricing, about, API changelog, GitHub org, and probed every `/.well-known/*` discovery URL on dzbuild.com, dzbuild.dev, api.dzbuild.app and mcp.dzbuild.com.

---

## 0. TL;DR: the facts that shape the design

1. **DZBuild is a large Algerian e-commerce SaaS**, not a POS vendor. It is a hosted store builder built around cash on delivery (COD), with 102+ couriers, landing pages, WhatsApp order messages, and Arabic/French UIs. It claims **14,498 stores**, was founded in 2024, and has paid plans at 1,000, 2,500 and 5,500+ DZD per month. It has **no POS or in-store checkout product** today. ([dzbuild.com/fr](https://dzbuild.com/fr/), [llms.txt](https://dzbuild.com/llms.txt), [pricing](https://dzbuild.com/fr/pricing))
2. **A public developer platform already exists** at dzbuild.dev. It launched on **2026-09-26**, two days before this research, and includes:
   - OAuth 2.0 authorization code + PKCE (S256 only);
   - one non-expiring `dzpk_live_` install token per store;
   - a REST API at `https://api.dzbuild.app/v1` with 19 app scopes;
   - HMAC-signed webhooks and signed `dz_launch` JWTs;
   - an OpenAPI 3.1 spec, `llms.txt` and an agent SKILL.md. ([changelog](https://dzbuild.dev/changelog.md))
3. **DZBuild runs three separate OAuth servers.** I found two of them only through discovery documents:
   | Server | Discovery | Client type | Refresh tokens | Documented? |
   |---|---|---|---|---|
   | **App platform**: `dzbuild.com/oauth/apps/*` | none (no `.well-known`) | **confidential only** (`client_secret` required) | **no** | yes (dzbuild.dev) |
   | **OIDC "Sign in with DZBuild"**: `dzbuild.com/oauth/oidc/*` | `https://dzbuild.com/.well-known/openid-configuration` | confidential only | **no** (auth code only) | **no public docs found** |
   | **MCP connector AS**: `mcp.dzbuild.com` | `https://mcp.dzbuild.com/.well-known/oauth-authorization-server` | **public clients allowed** (`none`), Dynamic Client Registration | **yes** | only as the AI assistant add-on |
4. **A Windows desktop POS cannot use the app platform directly as it stands today:**
   - redirect URIs must be **`https` only**, so there is no loopback and no custom scheme;
   - a `client_secret` is **mandatory** at the token endpoint;
   - the security rules require secrets and install tokens to stay **server side**, "never in a browser, a mobile app".
   A desktop app can hold neither, so today it needs a small DZBuild-run **"POS Link" broker**. Long term, DZBuild should add a **first-party public-client flow**: RFC 8252 loopback, refresh-token rotation and device flow. The mcp.dzbuild.com server already shows DZBuild can run one. ([security](https://dzbuild.dev/security.md), [oauth](https://dzbuild.dev/oauth.md))
5. **Several API gaps matter for a POS:**
   - no `updated_since` filter on products, and the orders `since` filter checks the *created* date only;
   - no product, stock or customer webhooks, only order events;
   - images can be added only from a public URL, with no file upload;
   - `POST /v1/orders` always takes prices from the catalogue and requires wilaya, commune and phone, so it does not model an in-store sale;
   - stock writes are "manual adjustments" that land in the undo history;
   - the limits are 120 req/min per install and 10 image uploads per minute per store;
   - plan caps apply: Pro allows 300 active products, and the old Free plan allowed only 5.

   Section 6 proposes concrete endpoints to close each gap.
6. **The POS must be fully offline-first.** Linking a DZBuild account is optional. The POS never needs an activation server; linking only adds catalog publishing, online-order intake, cloud backup and a unified dashboard.

---

## 1. What DZBuild is

### 1.1 Company and product

| Item | Finding | Source |
|---|---|---|
| Positioning | "Algerian e-commerce SaaS platform that lets merchants create an online store in minutes, in Arabic or French, with cash-on-delivery (COD) checkout" | [dzbuild.com/llms.txt](https://dzbuild.com/llms.txt) |
| Tagline (FR) | "L'unique plateforme e‑commerce derrière toute réussite" | [dzbuild.com/fr](https://dzbuild.com/fr/) |
| Founded | 2024 | [llms.txt](https://dzbuild.com/llms.txt), [about](https://dzbuild.com/about) |
| Founder | A search result lists Youcef Zemmar as "Senior Engineer & Founder of DZBuild" (youcef.dzbuild.com). The page did not resolve when I fetched it. **(partially verified: search snippet only)** | web search result |
| Scale claims | "14,498 boutiques algériennes construisent avec DZBuild" (home and about pages); "Over 10,000 stores created (July 2026)" (llms.txt) | [fr home](https://dzbuild.com/fr/), [about](https://dzbuild.com/about), [llms.txt](https://dzbuild.com/llms.txt) |
| Mission (AR) | "تمكين أي تاجر جزائري من إطلاق متجر إلكتروني احترافي في دقائق، بدون برمجة" | [about](https://dzbuild.com/about) |
| Support | support@dzbuild.com, 24/7 in Arabic and French, live chat (Intercom on the site) | [llms.txt](https://dzbuild.com/llms.txt), home HTML |
| Status page | status.dzbuild.app ("DZBuild Status") | [llms.txt](https://dzbuild.com/llms.txt), fetched |
| Reviews | Trustpilot 4.5/5 from 19 reviews; the company replies to reviews | [Trustpilot](https://fr.trustpilot.com/review/dzbuild.com) |
| Social | Instagram @dz_build, TikTok @dzbuild.com, YouTube @DZBuild-off, X @dzbuild, Facebook, GitHub DZBuild-com | [fr home](https://dzbuild.com/fr/) |
| Domains in use | dzbuild.com (site and dashboard), dzbuild.dev (developer docs), dzbuild.app (store subdomains, API gateway `api.dzbuild.app`, CDN `cdn.dzbuild.app`, status). The webhook rules also name `dzbuild.me`, `dzme.app`, `minacef.com` and `twarc.net` as DZBuild domains. | [webhooks](https://dzbuild.dev/webhooks.md) |

### 1.2 Target users

Individual online sellers ("vendeuse en ligne"), brands, wholesalers ("grossiste"), digital-product sellers, and fashion, cosmetics and electronics shops. ([fr home](https://dzbuild.com/fr/))

The Pickup-from-Store add-on is "built for Algerian merchants who already have a physical shop, depot or office" ([pickup-from-store](https://dzbuild.com/docs/addons/pickup-from-store)). This is the POS target: **DZBuild merchants who also run a physical shop**.

### 1.3 Plans (they matter for POS sync limits)

| Plan | Price | Products | Team members | Notes | Source |
|---|---|---|---|---|---|
| Free | — | **5 active** | 0 | **No longer offered to new merchants.** Granted only on request. **30 orders/month**, after which the storefront goes dark | [plans/free](https://dzbuild.com/docs/plans/free) |
| Trial | free | Pro features | — | Every new account gets a **3-day Pro trial** | [plans/free](https://dzbuild.com/docs/plans/free) |
| Pro | 1,000 DA/month (10,000/year) | up to 300 | 1 | "API avancée" | [pricing](https://dzbuild.com/fr/pricing) |
| Illimité (Unlimited) | 2,500 DA/month (25,000/year) | unlimited | 3 | Webhooks add-on and Pickup-from-Store need Unlimited+ | [pricing](https://dzbuild.com/fr/pricing), [addons](https://dzbuild.com/docs/addons/overview) |
| Entreprise | from 5,500 DA/month | unlimited | 25 included (+200 DA each, up to 200) | Merchant API keys and the AI assistant add-on are Enterprise-only; 99.9% SLA | [pricing](https://dzbuild.com/fr/pricing), [team](https://dzbuild.com/docs/operations/team) |

The OpenAPI `POST /v1/products` description states the product caps: "Free 5, Pro 300, Unlimited and Enterprise no limit; drafts do not count" ([openapi](https://dzbuild.dev/openapi/dzbuild-v1.json)).

**Consequence for the POS:** a POS with 2,000 SKUs linked to a **Pro** store can publish only 300 of them as *active*. Everything else must stay local, or be pushed as `draft`. The POS therefore needs a per-product **"Publier en ligne"** toggle and must surface the plan cap clearly.

Payments to DZBuild are made by CIB/Edahabia through Slick-Pay, by international card (Visa/Mastercard/PayPal/Apple Pay, billed in USD), or by BaridiMob/CCP transfer with a receipt upload ([cib-edahabia](https://dzbuild.com/docs/payments/cib-edahabia), [fr home](https://dzbuild.com/fr/)).

### 1.4 Platform features the POS should interoperate with

- **Orders:** COD, statuses `pending → confirmed → processing → shipped → delivered`, plus `cancelled` and `returned`. An Advanced Orders/CRM add-on adds 12 sub-statuses: no answer, busy line, postponed, and others. Orders carry a traffic-source icon. **There is no CSV/Excel order export in the dashboard; the closest option is the Google Sheets add-on.** ([selling/orders](https://dzbuild.com/docs/selling/orders))
- **Stock:** three modes (product-level, per-variant-option, per-combination). A store setting controls when stock is deducted: at confirmation (the default) or on order arrival. Cancel and return restore stock. The Stock Management add-on is available on every plan. ([stock-management](https://dzbuild.com/docs/addons/stock-management), [selling/products](https://dzbuild.com/docs/selling/products))
- **Products:** price, compare price, cost, SKU, barcode, weight and dimensions, variants (colour/size groups with price adjustments), combinations (at most 300 returned), images (up to 20 per product through the connector), quantity offers ("pack of 2"), min/max quantity, and custom input fields ("addons"). ([openapi](https://dzbuild.dev/openapi/dzbuild-v1.json), [add-ons](https://dzbuild.com/docs/addons/overview))
- **Import/export:** the dashboard has a bulk product import, and a Shopify Import add-on. Export goes only through Google Sheets ("export now"), which writes products as id, name, price, old price, stock, category, has-variants, variants, status and optional SKU. **Variant-level stock is never exported.** ([google-sheets](https://dzbuild.com/docs/operations/google-sheets), [multi-store](https://dzbuild.com/docs/operations/multi-store))
- **Pickup from Store:** a third delivery option (home, desk, pickup). Pickup is free and pickup orders are never sent to couriers. Unlimited plan and up. ([pickup-from-store](https://dzbuild.com/docs/addons/pickup-from-store))
- **Team:** one owner plus members with a checkbox permission matrix. "Only the store owner can install an app." ([team](https://dzbuild.com/docs/operations/team), [concepts](https://dzbuild.dev/concepts.md))
- **Multi-store:** each store has its own plan, products, orders, API keys and add-ons, with up to 50 stores per account. "There is no cross-store endpoint." ([multi-store](https://dzbuild.com/docs/operations/multi-store))
- **Mobile:** an Android app (`com.dzbuild.app`) that wraps the dashboard, iOS through Add to Home Screen, and push alerts for new orders. ([mobile-app](https://dzbuild.com/docs/operations/mobile-app))
- **AI:**
  - DZBuild Copilot runs inside the dashboard.
  - The **AI assistant add-on** is an MCP custom connector at `https://mcp.dzbuild.com/mcp`, Enterprise plan only. It can create and update products, images and variants, generate landing pages, and read orders and analytics. Orders are read-only through it.
  - A ChatGPT add-on page also exists.
  ([copilot](https://dzbuild.com/docs/copilot/overview), [add-ons](https://dzbuild.com/docs/addons/overview), `https://mcp.dzbuild.com/` banner text)
- **dzship:** a free, MIT-licensed Algerian shipping API at `https://freeship.dzbuild.com`. It requires no signup, supports 99 couriers (Yalidine family, ZR Express family, Maystro, NOEST, Ecotrack network, and others), allows 200 parcels/hour per IP, and passes courier credentials per request without storing them. A POS that is **not** linked to DZBuild could still create COD parcels through dzship. ([github.com/DZBuild-com/dzship](https://github.com/DZBuild-com/dzship))
- **Other open-source work:** `ownz`, a self-hostable link-in-bio builder (TypeScript, AGPL-3.0), and `dzbuild-app-starter` (Node.js, MIT). ([github.com/DZBuild-com](https://github.com/DZBuild-com))

### 1.5 Languages

- **Merchant site and dashboard:** Arabic (default, RTL) and French (`/fr/`). The llms.txt lists "Arabic + French interfaces, RTL support". ([llms.txt](https://dzbuild.com/llms.txt))
- **Developer docs:** English, Arabic (`/ar/`) and French (`/fr/`). Each language has its own `llms.txt`, which I confirmed returns HTTP 200.
- **App listings:** app review requires descriptions in all three languages. ([review-guidelines](https://dzbuild.dev/review-guidelines.md))
- **Review emails:** sent "in French when your DZBuild account language is French, in Arabic otherwise."
- **Storefront language:** `ar` or `fr`, exposed as `Store.language`. ([openapi](https://dzbuild.dev/openapi/dzbuild-v1.json))

---

## 2. Branding (extracted from live CSS and assets)

| Element | Value | Where found |
|---|---|---|
| Logo mark | A yellow circle with two white smiling shopping bags | `https://dzbuild.dev/img/logo-mark.png` (viewed) |
| Wordmark | `https://cdn.dzbuild.app/dzbuild-logo-480x128.svg`, whose only explicit fill is **`#FFC300`** | dzbuild.com home HTML, SVG parsed |
| Theme colour (marketing) | **`#f59e0b`** (amber-500) | `<meta name="theme-color">` on dzbuild.com |
| Marketing accents | `--color--accent: #e8940a` / `#f7a921`; secondary accent `#00acca`; dark text `#2b2b46`; body grey `#6a6a7a` | home inline CSS, `dz-lts.css` |
| Dev docs palette (light) | primary `#b45309` (amber-700), background `#fbfaf7`, ink `#1c1917`, hairline `#e7e2d8`, `--dz-accent: #f59e0b` | `styles.e86d4a13.css` on dzbuild.dev |
| Dev docs palette (dark) | primary `#fbbf24`, background `#0b1120`, panel `#111a2e`, hairline `#253047`, muted `#94a3b8` | same |
| Fonts | Marketing: **General Sans Variable** (Latin) + **Cairo** (Arabic). Dev docs: **Inter** + **Cairo**, and Cairo first when `lang=ar`. Code: **JetBrains Mono** | CSS `font-family` declarations |
| Social card | `https://dzbuild.dev/img/dzbuild-social-card.png` | dzbuild.dev meta |
| Naming rule for apps | A third-party app name "must not contain 'dzbuild'". A first-party POS would need DZBuild to exempt itself. | [review-guidelines](https://dzbuild.dev/review-guidelines.md) |

**Recommendation for the POS UI:**
- Primary: amber `#F59E0B`; brand yellow `#FFC300` for the logo and splash.
- Deep ink `#1C1917` or `#0B1120` for dark mode.
- Warm off-white `#FBFAF7` as the light background.
- Cairo for Arabic and Inter for Latin script. Both are open fonts (SIL OFL) and must be **bundled offline** in the installer, never loaded from Google Fonts at runtime.

---

## 3. What DZBuild exposes today (verified inventory)

### 3.1 The developer platform (dzbuild.dev)

**Every page is available as Markdown**: `/llms.txt`, `/llms-full.txt`, `<page>.md`, `/skills/dzbuild-apps/SKILL.md`, `/agents/AGENTS.md` and `/openapi/dzbuild-v1.json`. The robots.txt explicitly welcomes AI crawlers (`Content-Signal: search=yes, ai-input=yes, ai-train=yes`). ([llms.txt](https://dzbuild.dev/llms.txt), [robots.txt](https://dzbuild.dev/robots.txt))

The **sitemap pages** are: `/`, `/api-reference`, `/build-with-ai`, `/changelog`, `/concepts`, `/errors`, `/faq`, `/getting-started`, `/intro`, `/oauth`, `/rate-limits`, `/review-guidelines`, `/scopes`, `/security`, `/webhooks` and `/whatsapp`. Each also exists under `/ar/` and `/fr/`. ([sitemap.xml](https://dzbuild.dev/sitemap.xml))

**Discovery:** `https://dzbuild.dev/.well-known/openid-configuration` and `/.well-known/oauth-authorization-server` both return **404**. The app platform has **no RFC 8414 metadata**.

#### 3.1.1 Credentials

The developer console is at `https://dzbuild.com/dashboard/developer`. One account can register up to 10 apps.

| Credential | Format | Handling |
|---|---|---|
| `client_id` | `dzapp_` + 20 hex characters | public |
| client secret | `dzas_` + 48 hex characters | shown once; server side only |
| signing secret | 64 hex characters | signs webhooks and `dz_launch` JWTs |

([getting-started](https://dzbuild.dev/getting-started.md))

#### 3.1.2 OAuth install flow (exact contract)

| Step | Endpoint |
|---|---|
| Authorize | `GET https://dzbuild.com/oauth/apps/authorize` |
| Consent (internal) | `POST /oauth/apps/approve`, `/oauth/apps/deny` |
| Token | `POST https://dzbuild.com/oauth/apps/token` (form-encoded; `client_secret` in the form, or HTTP Basic) |
| API | `https://api.dzbuild.app/v1/...` |

**Authorize parameters:**
- `response_type=code`, `client_id`, `redirect_uri` and `state` are required. `state` must be 1–1024 characters.
- `redirect_uri` is matched exactly.
- `scope` is optional and space-separated, at most 512 characters. If omitted, all registered scopes are granted.
- `code_challenge` (exactly 43 base64url characters) and `code_challenge_method=S256` are required. `plain` is refused.

**Code:** 64 hex characters, valid for 10 minutes, single-use. A failed exchange still spends the code.

**Token response:**
```json
{ "access_token":"dzpk_live_…", "token_type":"Bearer", "scope":"orders:read products:read",
  "store_id":141, "install_id":57,
  "stores":[{"store_id":141,"store_name":"Boutique Amel","install_id":57,"access_token":"dzpk_live_…"},
            {"store_id":152,"store_name":"Amel Kids","install_id":58,"access_token":"dzpk_live_…"}] }
```

- **No `expires_in` and no `refresh_token`.** A token works until the merchant uninstalls the app (after which calls return 401) or reinstalls it (which replaces the token).
- One consent covers **up to 10 stores**.
- Only the **store owner** can install. Team members see no stores on the consent screen.
- There is **no revoke endpoint** for apps.

**Redirect URI rules:**
- **`https` only**, 1–5 URIs, each at most 512 characters.
- No userinfo, no `#fragment`, no `*` wildcard.
- The console "refuses `http`", so local testing needs an https tunnel.

**OAuth rate limits:** authorize 30 per 300 s, approve 10 per 600 s, token 60 per 300 s.

Sources: [oauth](https://dzbuild.dev/oauth.md), [getting-started](https://dzbuild.dev/getting-started.md), [security](https://dzbuild.dev/security.md), [concepts](https://dzbuild.dev/concepts.md).

#### 3.1.3 App lifecycle and plans

- **App statuses:** `draft` and `in_review` run in **test mode** (installable only on the developer's own stores), then `approved`, `rejected` or `suspended`.
- **Plans:** any plan can install an app. The developer can set `min_plan`; stores below it get `403 app_plan_required`. ([concepts](https://dzbuild.dev/concepts.md))
- **Submission checklist:**
  - a logo in PNG, JPEG or WebP, at least 128×128 px and at most 1 MB;
  - descriptions in EN, AR and FR, 20–500 characters each;
  - at least one redirect URI;
  - an https launch URL;
  - a support email;
  - at least one scope;
  - a verified webhook, if one is set;
  - at least one test install.
- **Editing an approved app** sends it back to `in_review`. ([review-guidelines](https://dzbuild.dev/review-guidelines.md))

#### 3.1.4 The 19 app scopes

The app scopes are:

| Resource | Read | Write / other |
|---|---|---|
| analytics | `analytics:read` | — |
| customers | `customers:read` | — |
| delivery | — | `delivery:send` |
| landing pages | `landing_pages:read` | `landing_pages:write` |
| orders | `orders:read` | `orders:write` |
| pixels | `pixels:read` | `pixels:write` |
| products | `products:read` | `products:write` |
| promo codes | `promos:read` | `promos:write` |
| shipping | `shipping:read` | `shipping:write` |
| store | `store:read` | `store:write` |
| WhatsApp | `whatsapp:read` | `whatsapp:send` |

Rules:
- Apps cannot request `ai:generate`, `usage:read`, `webhooks:read` or `webhooks:write`.
- Writes on orders, products and landing pages need **both** the read and the write scope, because the response reads the record back. ([scopes](https://dzbuild.dev/scopes.md))

#### 3.1.5 API conventions

- **Base:** `https://api.dzbuild.app/v1`.
- **Envelope:** `{"data":…, "meta":{"request_id","api_version"}}` on success and `{"error":{"code","message"},"meta":…}` on failure. `X-Request-Id` is echoed back.
- **Pagination:** cursor-based, with `items`, `next_cursor` and `has_more`. `limit` defaults to 50 and is capped at 200.
- **Idempotency:** **`Idempotency-Key` is mandatory** on POST, PATCH and DELETE (at most 64 characters of `[A-Za-z0-9_:.-]`).
  - The response is stored for 24 h and replayed with `Idempotency-Replay: 1`.
  - Reusing a key with a different request returns `422 idempotency_key_reuse`.
  - 4xx responses are stored; 5xx and 429 responses are not.
- **Rate limits:**
  - **120 req/min per install** in fixed calendar-minute windows. This bucket is checked first.
  - Then the store's shared budget. Install tokens get the Enterprise budget of 600/min, and the gateway also caps each store at 600/min.
  - Image upload: 10/min per store, 3 concurrent.
  - Courier send/link/test/rate-sync: 6/min, 2 concurrent.
  - Over the limit: `429` with `Retry-After`.
- **Payment-required responses:** `402 no_credit` (empty WhatsApp wallet) and `402 quota_exceeded`.
- **CORS:** "No CORS preflight support; `OPTIONS` returns `401`" ([API changelog v1.7](https://dzbuild.com/api-docs/changelog)). **The desktop app must call the API from its native layer (Rust or Node), not from WebView `fetch`.**
- **Endpoints closed to app tokens:** `/v1/keys`, `/v1/webhooks` and `/v1/changes`.

Sources: [rate-limits](https://dzbuild.dev/rate-limits.md), [errors](https://dzbuild.dev/errors.md), [api-reference](https://dzbuild.dev/api-reference.md).

#### 3.1.6 Webhooks

- **Setup:** one https URL per app on port 443, with a public hostname that is not on a DZBuild domain or `workers.dev`. It is verified from the console.
- **Events:**
  - `order.created`, `order.confirmed`, `order.processing`, `order.shipped`, `order.delivered`, `order.cancelled` and `order.returned`. These require `orders:read`.
  - `app.uninstalled`, sent always.
  - `webhook.verify`.
- **Latency:** DZBuild polls stores once a minute, so expect `order.created` within 1–2 minutes.
- **Signature:** `X-DZ-Signature: t=<ts>,v1=<hex HMAC-SHA256(secret, t + "." + raw_body)>`, with a 5-minute window.
- **Delivery:**
  - at least once; dedupe on the envelope `id`;
  - up to 5 attempts, waiting 60 s, 5 min, 30 min and 2 h between them;
  - the endpoint is disabled after 10 consecutive failures.
- **Order payload fields:** `id`, `store_id`, `order_number`, `store_seq`, `status`, `payment_status`, `payment_method`, `delivery_type`, `desk_name`, `customer_name`, `customer_phone`, `customer_email`, `wilaya_id`, `wilaya_name` (Arabic), `commune`, `address`, `subtotal`, `shipping_cost`, `discount`, `total`, `promo_code`, `customer_notes`, `tracking_number`, `delivery_company`, `created_at` and `updated_at`.
- **Line items:** `product_id`, `variant_id`, `product_name`, `variant_name`, `sku`, `price`, `quantity`, `total` and `source`.

([webhooks](https://dzbuild.dev/webhooks.md))

**There are no product, stock or customer events.** The merchant-side Webhooks add-on also covers only order events ([api-docs webhooks](https://dzbuild.com/api-docs/webhooks/overview)).

#### 3.1.7 Launch link

The merchant's **Open** button redirects to the app's https launch URL with `dz_launch`, an HS256 JWT signed with the signing secret.
- Claims: `iss=dzbuild`, `aud=client_id`, `sub` (user id), `store_id`, `install_id`, `is_owner`, `iat`, `exp=iat+300` and `jti`.
- Verification: check `alg`, `iss`, `aud` and `exp`, and refuse a `jti` seen in the last 5 minutes.

([security](https://dzbuild.dev/security.md))

**POS use:** the launch URL can be a small https page that tries the `dzbuild-pos://open?store_id=…` deep link and otherwise offers the installer download.

#### 3.1.8 WhatsApp API

- **Templates:** six Meta-approved order templates: `received`, `confirmed`, `shipped_home`, `shipped_desk`, `delivery_failed` and `desk_ready`. `shipped` is an alias that picks the right one.
- **Languages:** AR and FR.
- **Cost:** one credit per message from the store's wallet, and each template can be sent at most once per order.
- **Error codes:** `402 no_credit`, `403 addon_not_active`, `409 already_sent`, and the 422 codes `invalid_number`, `suppressed`, `template_not_approved`, `empty_param`, `unknown_template` and `invalid_language`.

([whatsapp](https://dzbuild.dev/whatsapp.md))

**POS use:** after an online order is prepared in the shop, send `shipped` or `desk_ready` from the POS.

### 3.2 Full `/v1` endpoint inventory, with the value of each group for the POS

This comes from the OpenAPI file, info version "1.5", `servers: https://api.dzbuild.app`. The file's `dzOAuth` scheme still says "Not available yet: arrives with the developer platform". It is **stale** compared with the changelog, which says the platform launched on 2026-09-26.

| Group | Endpoints (scope) | POS relevance |
|---|---|---|
| Identity | `GET /v1/whoami` (no scope), `GET /v1/ping` (no token) | link health check; `ping` for connectivity |
| Store | `GET/PATCH /v1/store`, `/v1/store/design`, `/v1/store/design/fields`, `/v1/store/home-sections`, `POST /v1/store/theme`, `/fast-checkout-theme`, `/variant-style`, `GET /v1/themes` (store:read/write) | **read** store name, logo, `language`, theme colours and `public_url` for receipts and QR codes linking to the online shop |
| Categories | `GET/POST /v1/categories`, `POST /reorder`, `GET/PATCH/DELETE /v1/categories/{id}` (products:*) | map POS families to online categories. **The API cannot set a category image.** |
| Products | `GET/POST /v1/products`, `GET/PATCH/DELETE /v1/products/{id}`, `/addons`, `/images` (add from **public URL**), `/images/{image_id}`, `/offers`, `/quantity-rules`, `/stock` GET/POST, `PUT /variants` (products:*) | **core of catalog sync** |
| Orders | `GET/POST /v1/orders`, `GET/PATCH /v1/orders/{id}`, `POST /cancel`, `POST /send-to-delivery` (delivery:send), `POST /{id}/whatsapp` | **online-order intake** into the POS |
| Customers | `GET /v1/customers` (filter by exact `phone`/`email`), `/{id}`, `/{id}/orders` (read only) | customer lookup by phone at the till; fraud_score and is_banned flags |
| Shipping | `GET /v1/wilayas`, `/v1/wilayas/{id}/communes`, `/v1/shipping/{coverage,providers,rates,settings}` and writes | reference data for addresses (58 or 69 wilayas) |
| Promo codes | `GET/POST/PATCH/DELETE /v1/promo-codes` | optionally share promo codes between POS and online |
| Analytics | `GET /v1/analytics?range=today\|yesterday\|7d\|30d\|this_month\|last_month\|this_year\|custom` | combined "online + shop" dashboard. Revenue fields are hidden unless the key belongs to the owner. |
| Pixels / Landing pages | `/v1/pixels`, `/v1/landing-pages/*` | not needed for the POS |
| WhatsApp | `GET /v1/whatsapp/{templates,balance,messages}`, `POST /v1/orders/{id}/whatsapp` | notify buyers |
| Closed to apps | `/v1/keys`, `/v1/webhooks`, `/v1/changes` (+undo), `/v1/usage`, `/v1/quotas`, `POST /v1/landing-pages/generate` | — |
| Assistant connection | `GET /v1/connection`, `POST /v1/connection/active-store` | used by the MCP connector; returns 404 for app tokens |

Key schemas, as the OpenAPI file defines them:

- **Product** (full):
  - `pricing{price, compare_price, cost_price}` in DZD;
  - `inventory{sku, barcode, track_stock, stock_quantity, low_stock_alert}`;
  - `shipping{weight,height,width,length,do_insurance}`;
  - `status` (`active|draft|archived`), `featured`, `has_variants`;
  - `images[]`, `variants[]`, and `combinations[]` (**at most 300**, with a `combinations_truncated` flag);
  - `created_at` and `updated_at` as `YYYY-MM-DD HH:MM:SS`.
- **Variant option:**
  - `id`, `value`, `color_code`, `price_adjustment` (DZD, may be negative);
  - `stock` (null = unlimited), `sku`, `image_id`, `is_active`.
  - **Options have `sku` but no `barcode` field.**
- **Combination:** `id`, `sku`, `stock`, `is_active`, `options{group→option}`. **No barcode.**
- **Stock** (`GET/POST /v1/products/{id}/stock`):
  - The mode is `product`, `option` or `combination`.
  - A POST carries `items[]` (1–500). Each item names a target and exactly one of `set` (0–9,999,999), `delta` or `unlimited`.
  - The API describes it as "a manual adjustment: it does not touch order stock bookkeeping". It "is recorded in the store's change history and can be undone."
- **Order create** (`POST /v1/orders`):
  - `customer{name, phone, wilaya_id (1–58 or 1–69), commune, address?, email?}` is required.
  - `delivery{type: home|desk|pickup|digital, desk_id?, desk_name?}`.
  - `items[]{product_id, quantity 1–9999, variants[]}`, at most 50 lines.
  - `discount` (DZD, capped), `payment_method` (`cod|free_digital|digital_payment`), `notes`.
  - **Line prices always come from the catalogue.** Any price or shipping value in the body is ignored.
  - Stock moves on the **first confirmation**. The order counts toward plan order caps and fires notifications and `order.created`.
- **Order status moves:**
  - `pending` → `confirmed`, `processing` or `cancelled`;
  - `confirmed` → `processing`, `shipped` or `cancelled`;
  - `processing` → `shipped` or `cancelled`;
  - `shipped` → `delivered` or `returned`;
  - `delivered` → `returned`.
  - Entering `confirmed`, `processing`, `shipped` or `delivered` for the first time takes the stock. Moving to `cancelled` or `returned` puts it back.
- **Timestamps:** "store server local time" / "Africa/Algiers local time" in `YYYY-MM-DD HH:MM:SS` with **no offset**. Algeria is UTC+01:00 with no DST, so the POS should parse these as `+01:00`. Webhook envelopes do carry an offset.

### 3.3 Hidden OIDC provider on dzbuild.com (discovered, undocumented)

`GET https://dzbuild.com/.well-known/openid-configuration` returns **200**:

```json
{"issuer":"https://dzbuild.com",
 "authorization_endpoint":"https://dzbuild.com/oauth/oidc/authorize",
 "token_endpoint":"https://dzbuild.com/oauth/oidc/token",
 "userinfo_endpoint":"https://dzbuild.com/oauth/oidc/userinfo",
 "jwks_uri":"https://dzbuild.com/oauth/oidc/jwks.json",
 "response_types_supported":["code"], "grant_types_supported":["authorization_code"],
 "subject_types_supported":["public"], "id_token_signing_alg_values_supported":["RS256"],
 "scopes_supported":["openid","profile","email"],
 "token_endpoint_auth_methods_supported":["client_secret_basic","client_secret_post"],
 "code_challenge_methods_supported":["S256"],
 "claims_supported":["sub","iss","aud","exp","iat","auth_time","nonce","email","email_verified","name","given_name","family_name"]}
```

- The JWKS publishes one RSA key, with `kid` `dc429b00b96b405d`.
- `userinfo` returns `401 {"error":"invalid_token"}` without a token.
- **I found no public documentation, no client registration path and no mention of it on dzbuild.dev (unverified who uses it).**
- It is **confidential-client only** and has **no refresh tokens**. Like the app platform, it cannot serve a desktop app without changes.
- It is the natural place for **"Se connecter avec DZBuild"**: identity only, with `sub`, `email` and `name`.

### 3.4 MCP connector authorization server (mcp.dzbuild.com)

`GET https://mcp.dzbuild.com/.well-known/oauth-authorization-server`:

```json
{"issuer":"https://mcp.dzbuild.com","authorization_endpoint":"https://mcp.dzbuild.com/authorize",
 "token_endpoint":"https://mcp.dzbuild.com/token","registration_endpoint":"https://mcp.dzbuild.com/register",
 "scopes_supported":["dzbuild:mcp"],"response_types_supported":["code"],"response_modes_supported":["query"],
 "grant_types_supported":["authorization_code","refresh_token"],
 "token_endpoint_auth_methods_supported":["client_secret_basic","client_secret_post","none"],
 "revocation_endpoint":"https://mcp.dzbuild.com/token","code_challenge_methods_supported":["plain","S256"]}
```

- The protected-resource metadata (RFC 9728) is at `/.well-known/oauth-protected-resource` (and `/mcp`), with `resource_name: "DZBuild Store Connector"`.
- `https://mcp.dzbuild.com/mcp` answers `401` with `WWW-Authenticate: Bearer … resource_metadata="…"`. This is the standard MCP authorization pattern.
- The store key behind the connector "is created on our servers and never displayed". "Only the store owner can connect a store". Disconnecting cuts access "within about a minute". ([add-ons](https://dzbuild.com/docs/addons/overview))

**Why this matters:** DZBuild **already operates a public-client-capable OAuth 2.x server**, with Dynamic Client Registration, `token_endpoint_auth_method=none`, refresh tokens and a revocation endpoint. The desktop POS needs the same shape. Two weaknesses are worth fixing if it is reused:
- it still advertises `plain` PKCE;
- `revocation_endpoint` equals `token_endpoint`.

I did **not** register a client or test the loopback redirect acceptance, so both are **(unverified)**.

### 3.5 Merchant keys and public keys (for completeness)

- **Merchant platform key** (`Authorization: Bearer dzpk_live_<id>.<48-hex secret>`):
  - "API keys can only be issued for stores on an active Enterprise plan" and must be pilot-enrolled.
  - At most 3 active keys per store.
  - Default scopes exclude `whatsapp:*` for keys minted before v1.6.
  - ([api-docs/intro](https://dzbuild.com/api-docs/intro), [authentication](https://dzbuild.com/api-docs/authentication), [API changelog](https://dzbuild.com/api-docs/changelog))
- **Public key** (`Authorization: DZ-Public dzpub_live_…` plus a nonce and an HMAC signature):
  - accepts only `POST /v1/signups` and `POST /v1/events` (identity-less analytics events);
  - is inert until DZBuild support activates it.
  - ([public-key/events](https://dzbuild.com/api-docs/public-key/events))
- **The POS must never ask a merchant to paste a merchant API key.** The security rules explicitly forbid apps from asking for one ([security](https://dzbuild.dev/security.md)), and merchant keys are Enterprise-only anyway.

---

## 4. Gap analysis: POS needs vs the DZBuild API today

| # | POS need | Today | Impact | Fix (sections 5–6) |
|---|---|---|---|---|
| G1 | Desktop app links an account | App OAuth needs an **https redirect** and a **client_secret**; tokens must stay server side | Cannot be done purely in the desktop app | Broker (Track A) or first-party public client (Track B) |
| G2 | Long-lived but revocable session on a PC | Install tokens **never expire** and there is **no refresh and no revoke endpoint** | A stolen PC means a permanent store token until the merchant uninstalls | Short-lived access plus rotating refresh, bound to the device |
| G3 | Receive online orders in real time | Webhooks need a **public https endpoint**; a shop PC is behind NAT | The desktop cannot receive webhooks | Broker relays them (SSE, WebSocket or long-poll), or the POS polls `GET /v1/orders` |
| G4 | Delta-sync the catalog | `GET /v1/products` has **no `updated_since`**, and the list is cached for 30 s at the edge | A full re-scan costs 1 call per 200 products (list) plus 1 per product (detail) | `updated_since` plus a change feed |
| G5 | Delta-sync orders | The `since` filter checks **created date only**; status changes need webhooks | Status changes are missed without webhooks | `updated_since` on orders |
| G6 | Stock changes from POS sales | `POST /products/{id}/stock` works one product per call. It is logged as a "manual adjustment" in the **undo history** and does not touch order bookkeeping | 100 sales/hour creates noisy history and burns the 120/min budget | Batch inventory endpoint with `reason=pos_sale` |
| G7 | Record in-store sales online (unified reporting) | `POST /v1/orders` forces catalogue prices, requires phone/wilaya/commune, counts toward caps and fires notifications and WhatsApp | In-store tickets cannot be recorded without faking COD orders | New `POS sales` resource |
| G8 | Barcodes on variants | `barcode` exists only at product level; options and combinations have only `sku` | A multi-size shoe cannot scan per size online | Add `barcode` to options and combinations |
| G9 | Push local product photos | Images can be added **only from a public https URL**, 10/min per store | The POS would have to host images | Pre-signed upload endpoint |
| G10 | Know the store's stock-deduction mode | Setting exists in the dashboard; **not exposed** in the API (unverified; not present in the `Store` schema) | Risk of double deduction | Expose `stock_deduction` on `GET /v1/store` |
| G11 | Multi-store owner, one POS per shop | One token per store; consent covers up to 10 stores | OK. The POS keeps a store↔shop mapping | — |
| G12 | Staff | Only the owner can install; team members can't | The owner links once per PC; POS staff use local PINs | Device authorization flow (owner approves from phone) |
| G13 | Plan caps | Pro allows 300 active products; the legacy Free plan allowed 5 products and 30 orders/month | Publishing gets refused with `400 bad_request` | POS shows the cap and publishes selectively |
| G14 | Cloud backup of the POS DB | Nothing exists | — | New backup endpoints |
| G15 | Map POS ids ↔ DZBuild ids idempotently | No `external_id`; SKU search is exact only | Hard to recover the mapping after a local DB loss | `external_id` on products, variants and customers, plus upsert |
| G16 | Discovery | No RFC 8414 metadata for the app platform | Endpoints must be hard-coded | Publish `/.well-known/oauth-authorization-server` |

---

## 5. Recommended integration design

### 5.1 Principles

1. **Offline-first, link optional.**
   - The POS installs and runs forever with no account, no activation and no internet.
   - Linking DZBuild is a feature ("Connecter ma boutique en ligne"), not a gate.
   - Every sync path is asynchronous through a local **outbox** and survives days offline.
2. **Free, license-free.**
   - There is no license server, and the POS never phones home to validate a license.
   - Updates are signed; auto-update is optional and can be disabled.
   - DZBuild linking adds value (sync and backup) but never unlocks core POS features.
3. **Least privilege.** Request only the scopes needed per feature, and use incremental consent when the user enables a feature.
4. **The desktop holds no secret that can't be revoked.** Nothing permanent and store-wide lives on the shop PC.

### 5.2 Track A: works today, with a DZBuild-run "POS Link" broker

DZBuild registers a normal app on its own platform, for example `client_id=dzapp_…`, named something like "Caisse" per the naming rule, or exempted as first-party. It also runs a tiny broker service, for example `https://pos-link.dzbuild.dev`. The broker is:
- an **OAuth confidential client** of `dzbuild.com/oauth/apps/*`. It holds the `client_secret`, the signing secret and the install tokens (encrypted at rest), as the security rules require.
- an **OAuth 2.1 authorization server for the desktop** (a public client, RFC 8252 loopback redirect, PKCE S256). It issues **short-lived broker access tokens and rotating refresh tokens bound to the device**.
- an **API relay**: it forwards `/v1/*` calls with the store's install token, and adds batching, a cache, and `updated_since` emulation.
- a **webhook sink**: it verifies `X-DZ-Signature`, dedupes by `id`, and queues events per store for the desktop. The desktop pulls them with `GET /events?cursor=` (long-poll) or an SSE stream.

```mermaid
sequenceDiagram
  autonumber
  participant POS as Desktop POS (Windows)
  participant BR as Browser (system default)
  participant LK as POS Link broker (DZBuild)
  participant DZ as dzbuild.com OAuth (apps)
  participant API as api.dzbuild.app
  POS->>POS: gen PKCE_d (verifier/challenge), state_d; start loopback 127.0.0.1:{port}
  POS->>BR: open https://pos-link.dzbuild.dev/authorize?client_id=dzpos-desktop&redirect_uri=http://127.0.0.1:{port}/cb&code_challenge=…&state=state_d
  BR->>LK: GET /authorize (desktop leg)
  LK->>LK: gen PKCE_s, state_s; bind to desktop request
  LK->>BR: 302 → https://dzbuild.com/oauth/apps/authorize?client_id=dzapp_…&redirect_uri=https://pos-link.dzbuild.dev/dz/callback&scope=store:read products:read products:write orders:read orders:write&state=state_s&code_challenge=…&code_challenge_method=S256
  BR->>DZ: merchant logs in, picks store(s), approves
  DZ->>BR: 302 → https://pos-link.dzbuild.dev/dz/callback?code=…&state=state_s
  BR->>LK: callback
  LK->>DZ: POST /oauth/apps/token (code, verifier_s, client_secret)
  DZ-->>LK: stores[] with dzpk_live_ install tokens (kept server-side, encrypted)
  LK->>BR: 302 → http://127.0.0.1:{port}/cb?code=broker_code&state=state_d
  BR->>POS: loopback hit; POS shows "Vous pouvez fermer cet onglet"
  POS->>LK: POST /token (broker_code, verifier_d, device public key)
  LK-->>POS: access_token (10–15 min), refresh_token (rotating), stores[]
  POS->>LK: GET /v1/products?updated_since=… (Bearer broker token)
  LK->>API: GET /v1/products (Bearer dzpk_live_…)
```

**Pros:**
- Uses only the documented platform.
- Complies with "secrets and tokens server side".
- Gives the desktop revocable, expiring tokens.
- Solves webhooks (G3) and can emulate `updated_since` (G4).

**Cons:**
- DZBuild must operate the broker.
- The broker holds a token to every linked store, so it is a high-value target (use a KMS, strict isolation and audit logs).

### 5.3 Track B: the target, a first-party native client on DZBuild's own authorization server

Because the POS is a **DZBuild first-party product**, DZBuild can extend its own authorization server instead of adding a middleman. The simplest route is to generalise the existing `mcp.dzbuild.com` public-client server, or to add public-client support to `dzbuild.com/oauth`. The authorization server should:

1. **Publish RFC 8414 metadata** at `https://dzbuild.com/.well-known/oauth-authorization-server`.
2. **Register the POS as a public client** (`token_endpoint_auth_method: none`) with:
   - redirect URIs `http://127.0.0.1/oauth/callback` and `http://[::1]/oauth/callback`, with **any port accepted** at request time per RFC 8252 §7.3;
   - optionally `dzbuild-pos:/oauth/callback` (private-use URI scheme, RFC 8252 §7.1, reverse-domain form `dev.dzbuild.pos:/oauth/callback`).
   - **Loopback is preferred on Windows**, because any app can register a custom scheme there.
3. **Require PKCE S256** and drop `plain`, per the OAuth 2.1 draft and RFC 9700.
4. **Issue short-lived access tokens** (JWT, 10–15 min) and **rotating refresh tokens** with reuse detection. The idle timeout should be 30 days and the absolute lifetime 180 days, so a shop that is offline for weeks re-links easily.
5. **Offer a revocation endpoint** (RFC 7009) that is distinct from the token endpoint.
6. **Offer the Device Authorization Grant** (RFC 8628) as the second path. The till shows a code and QR, and the **owner approves from the DZBuild Android app or dashboard**. This fits Algerian shops, where staff run the till and the owner is elsewhere, and "only the store owner can install".
7. Optionally, **DPoP** (RFC 9449): sender-constrain tokens to a per-device key held with Windows DPAPI or CNG, so a copied refresh token is useless on another PC.
8. Optionally, **resource indicators** (RFC 8707) with `resource=https://api.dzbuild.app`.

```mermaid
sequenceDiagram
  autonumber
  participant POS as Desktop POS
  participant BR as System browser
  participant AS as dzbuild.com OAuth (first-party, public client)
  participant API as api.dzbuild.app
  POS->>POS: verifier (32 random bytes → 43 chars b64url), challenge=S256, state, nonce; listen 127.0.0.1:0
  POS->>BR: open AS/authorize?response_type=code&client_id=dzbuild-pos-windows&redirect_uri=http://127.0.0.1:53187/oauth/callback&scope=openid profile offline_access store:read products:read products:write inventory:write orders:read orders:write&state&nonce&code_challenge&code_challenge_method=S256&ui_locales=ar
  BR->>AS: login (+2FA) → consent → pick store(s)
  AS->>BR: 302 http://127.0.0.1:53187/oauth/callback?code&state&iss
  BR->>POS: loopback; verify state + iss (RFC 9207); close listener
  POS->>AS: POST /oauth/token grant_type=authorization_code, code, redirect_uri, code_verifier, client_id [+ DPoP proof]
  AS-->>POS: access_token (15 min), refresh_token (rotating), id_token, stores[]
  POS->>POS: refresh_token → Windows Credential Manager; access_token in RAM only
  POS->>API: calls with Bearer/DPoP
  Note over POS,AS: Later: grant_type=refresh_token (rotates); on unlink: POST /oauth/revoke then delete credential
```

### 5.4 Desktop implementation rules (for either track)

- **Browser:**
  - Use the **system browser** only (`ShellExecute` or `open::that`, Tauri `opener`, Electron `shell.openExternal`).
  - **Never use an embedded WebView** for login (RFC 8252 §8.12). The user's saved password and 2FA stay in their real browser, and the POS never sees credentials.
- **Loopback listener:**
  - Bind to `127.0.0.1` (not `localhost`, not `0.0.0.0`) on an ephemeral port.
  - Accept exactly one request on `/oauth/callback`, then shut down.
  - Time out after 10 minutes, matching DZBuild's consent validity.
  - Answer with a static, localized page: "Connexion réussie — vous pouvez fermer cet onglet" / "تم الربط بنجاح".
- **Validation:** check `state` in constant time. If the server sends `iss` (RFC 9207), check it too. With `openid`, validate the ID token signature against the JWKS, plus `aud`, `nonce` and `exp`.
- **Token storage on Windows:**
  - Store the **refresh token only** in **Windows Credential Manager** (generic credential, target `DZBuild POS/<account sub>/<device id>`, persistence `LOCAL_MACHINE` or enterprise per policy).
  - Rust: the `keyring` crate uses Credential Manager on Windows. Electron: `safeStorage` (DPAPI) plus a file.
  - Credential Manager blobs have a size limit (`CRED_MAX_CREDENTIAL_BLOB_SIZE`, 5×512 bytes; the limit is from memory, so verify it in the Win32 docs). Keep refresh tokens **opaque and short**. If a token is larger, encrypt it with DPAPI in `%LOCALAPPDATA%` and keep only the key reference in Credential Manager.
  - Keep **access tokens in memory only**. Never write any token to the SQLite DB, logs, crash dumps or exports and backups.
- **Track A exception:** if Track A cannot issue refresh tokens and the desktop temporarily receives a non-expiring `dzpk_live_` token, which **violates** the platform's server-side rule, store it the same way. Also add a **"Déconnecter"** button that sends the owner to the DZBuild dashboard to uninstall, since there is no revoke API (G2). **This is not recommended.**
- **Refreshing:**
  - Refresh proactively at 80% of the token lifetime, and again on resume from sleep and on connectivity return.
  - Serialize refreshes with a mutex, because rotation means two parallel refreshes would trigger reuse detection.
  - On `invalid_grant`, mark the link "Reconnexion requise" but **keep selling offline**. Sync resumes after re-link.
- **Unlink:** revoke the refresh token (Track B) or tell the broker to drop the device (Track A), delete the credential, and keep local data. Optionally purge the synced mapping tables.
- **Multi-store:**
  - One consent can cover up to 10 stores, and each gets its own token.
  - The POS asks which **shop (point de vente)** maps to which DZBuild store.
  - A multi-shop owner can link different PCs to different stores.
- **Owner-only:**
  - The link wizard says up front: "Seul le propriétaire de la boutique DZBuild peut autoriser la connexion".
  - Staff use **local POS accounts and PINs**, which are unrelated to DZBuild team members.
- **HTTP client:**
  - Native HTTP only, because of CORS (§3.1.5).
  - Use TLS 1.2+ with the OS trust store. Certificate pinning is optional; if used, pin the public key with a backup pin, and don't pin a leaf certificate.
  - Send a `User-Agent: DZBuildPOS/<version> (Windows <build>)` header and an `X-Request-Id`.
- **Clock:** POS PCs often have wrong clocks, which breaks JWT `exp` checks and webhook windows. Use the server `Date` header or the `GET /v1/ping` `time` field to compute a skew and apply it.
- **Idempotency keys:**
  - Derive them deterministically from local immutable ids, for example `pos:{device_id}:{outbox_id}` (at most 64 characters of `[A-Za-z0-9_:.-]`).
  - Reuse the key on every retry of the same outbox item.
  - After a 4xx that was fixed, generate a new key, because stored 4xx responses replay for 24 h.
- **Backoff:** honour `Retry-After` and `error.retry_after`. Budget about 100 of the 120 req/min for sync and keep headroom for interactive calls (a customer lookup at the till).
- **Error mapping:**

  | Response | POS behaviour |
  |---|---|
  | `401` | "Lien expiré/révoqué", stop sync |
  | `403 app_*` | Show the message ("mise à niveau requise: Pro") |
  | `402` | Show "Recharger le portefeuille WhatsApp" |
  | `409 stock_mode_mismatch` | Re-read the product's stock mode |
  | `422 idempotency_key_reuse` | Bug alarm |

### 5.5 Sync model (offline-first)

**Local tables** (in addition to POS core):
- `dz_link(store_id, install_id, scopes, linked_at, status)`
- `dz_map(entity, local_id, dz_id, dz_parent_id, last_synced_hash, last_synced_at)` — `entity` is `product`, `option`, `combination`, `category` or `customer`
- `outbox(id, entity, op, payload, idem_key, attempts, next_attempt_at, last_error)`
- `inbox_events(event_id PRIMARY KEY, received_at)` — for dedupe

| Data | Direction | Mechanism today | Notes |
|---|---|---|---|
| Store profile (name, logo, colours, `public_url`, language) | DZ → POS | `GET /v1/store` on link and daily | Receipt header; QR "Commandez en ligne" to `public_url` |
| Categories | POS → DZ (the POS owns the taxonomy); optionally DZ → POS on first link | `POST/PATCH /v1/categories` | Category images are set only in the dashboard |
| Products | **POS → DZ** for products flagged "Publier en ligne"; DZ → POS import on first link | `POST /v1/products` then `PUT /variants`, `POST /images` (from public URL), `POST /stock`; `PATCH` on change | Respect plan caps; push extra products as `draft`; hash fields to skip no-op PATCHes |
| Prices | POS → DZ | `PATCH /v1/products/{id}` (`price`, `compare_price`, `cost_price`) | Online price may differ (an "online price list" option) |
| Stock | **POS is the source of truth for physical stock**. The online-available figure is computed and pushed; online sales come back as orders | `POST /v1/products/{id}/stock` with `set` (periodic reconcile) or `delta` (real time) per target | Apply the **buffer rule** from DZBuild's own blog: never expose the last N units online ([blog](https://dzbuild.com/blog/multi-channel-selling-algeria-one-inventory-every-platform)) |
| Online orders | **DZ → POS** | Poll `GET /v1/orders?since=<created>` every 1–2 min when online, plus webhooks via the Track A relay | Show a "Commandes en ligne" inbox; the preparer picks items, prints a label and confirms |
| Order status | POS → DZ | `PATCH /v1/orders/{id}` (confirmed → processing → shipped), `POST /send-to-delivery`, `POST /cancel` | DZBuild deducts stock on confirmation. The POS must **not** also push a stock delta for that order (echo suppression: tag the local movement `source=dz_order:{id}`) |
| Customers | DZ → POS (read-only) | `GET /v1/customers?phone=` on demand at the till | Show `fraud_score` / `is_banned` for credit-sale risk |
| Buyer notifications | POS → DZ | `POST /v1/orders/{id}/whatsapp` (`shipped`, `desk_ready`) | Costs one wallet credit each |
| In-store sales totals | POS → DZ | **Not possible cleanly today (G7)** | Keep local until `pos/sales` exists (§6) |
| Analytics | DZ → POS | `GET /v1/analytics?range=today` | Combined "Boutique + En ligne" dashboard |
| Shipping reference | DZ → POS | `GET /v1/wilayas`, `/communes` (cached) | Also ship the 58/69 wilaya list in the installer for offline use |
| Parcels without DZBuild | POS → courier | **dzship** (`freeship.dzbuild.com`) | For unlinked shops that sell via Facebook or phone |

**Echo suppression and double deduction:**
1. An online order is confirmed and DZBuild decrements its online stock.
2. The POS ingests the order and decrements local physical stock with a movement tagged `origin=dz`.
3. The outbox skips pushing that movement back.
4. A nightly reconcile runs `set` = local available − buffer.

The POS must read the store's deduction mode (confirmation vs arrival, G10). Until DZBuild exposes it, the POS should ask the merchant during link setup.

**Rate budget example:**
- **Initial publish of 300 products with 2 images each:** about 300 creates, plus up to 300 variant PUTs and 300 stock POSTs. That is about 900 calls, or **8–9 minutes at 110 req/min**.
- **Images:** 600 uploads are capped at 10/min, so **about 60 minutes**. Run them in the background with a progress bar and let the POS keep selling.
- **Steady state:** coalesce stock deltas per product per 30 s window, so a busy till with 200 tickets/hour stays well under the limit.

### 5.6 Webhooks and events for a desktop

- A desktop cannot host a public https endpoint (G3). The broker (Track A) or a first-party **event relay** (Track B) receives signed webhooks.
- The relay verifies `X-DZ-Signature` over the raw body with a ±300 s window and dedupes by `id`. It then exposes one of:
  - `GET /v1/events?cursor=…&wait=25` (long-poll; the most robust choice through shop proxies and flaky 4G routers), or
  - `GET /v1/events/stream` (Server-Sent Events).
- **Useful events:** `order.created`, `order.confirmed`, `order.cancelled`, `order.returned`, `app.uninstalled`. With the new events proposed in §6: `product.updated`, `inventory.level_changed` (from online edits), `store.plan_changed` and `link.revoked`.
- **On `app.uninstalled` or `link.revoked`:** stop sync, show a banner, keep local data, and delete tokens. The platform requires deleting store data held on the server within 30 days. The POS's own local data belongs to the merchant and is not affected.

### 5.7 What "free service for DZBuild" means in practice

- **The POS itself:** free, with no plan requirement (`min_plan=free`). Install tokens work on every plan ([concepts](https://dzbuild.dev/concepts.md)).
- **Plan limits still apply** to what the store accepts: 300 active products on Pro, and landing pages and WhatsApp credits are paid.
- **Growth loop:** POS users without DZBuild see a "Créez votre boutique en ligne" call to action that opens the register page with UTM `utm_source=dzbuild-pos`.
- **Attribution:** once linked, the POS could send identity-less usage events through `POST /v1/events` with a public key, but only with explicit opt-in consent ([public-key/events](https://dzbuild.com/api-docs/public-key/events)).

---

## 6. PROPOSED: endpoints, scopes and claims DZBuild should add for the POS

These are design proposals for the DZBuild team. Nothing in this section exists today.

### 6.1 Authorization server (first-party)

| Endpoint | Purpose |
|---|---|
| `GET https://dzbuild.com/.well-known/oauth-authorization-server` | RFC 8414 metadata (plus the existing OIDC doc) |
| `GET https://dzbuild.com/oauth/authorize` | Authorization code + PKCE S256; accepts registered **public** clients with loopback redirect (any port) |
| `POST https://dzbuild.com/oauth/token` | `authorization_code`, **`refresh_token`** (rotation and reuse detection), `urn:ietf:params:oauth:grant-type:device_code` |
| `POST https://dzbuild.com/oauth/device_authorization` | RFC 8628: returns `device_code`, `user_code` (8 characters, e.g. `DZKM-4Q7P`), `verification_uri=https://dzbuild.com/device`, `verification_uri_complete`, `interval` |
| `POST https://dzbuild.com/oauth/revoke` | RFC 7009 (separate from the token endpoint) |
| `GET https://dzbuild.com/oauth/oidc/userinfo` | Already exists |
| `GET https://dzbuild.com/dashboard/connected-devices` (UI) | Owner sees and revokes POS devices (name, last seen, IP, version) |

**Client registration (static, first-party):**
```json
{
  "client_id": "dzbuild-pos-windows",
  "client_name": "DZBuild Caisse (Windows)",
  "application_type": "native",
  "token_endpoint_auth_method": "none",
  "grant_types": ["authorization_code", "refresh_token", "urn:ietf:params:oauth:grant-type:device_code"],
  "redirect_uris": ["http://127.0.0.1/oauth/callback", "http://[::1]/oauth/callback"],
  "require_pkce": true, "pkce_methods": ["S256"],
  "dpop_bound_access_tokens": true,
  "access_token_ttl": 900, "refresh_token_idle_ttl": 2592000, "refresh_token_max_ttl": 15552000
}
```

**New scopes:**

| Scope | Grants |
|---|---|
| `openid`, `profile`, `email` | Identity (existing OIDC) |
| `offline_access` | Refresh token |
| `inventory:read` / `inventory:write` | Stock levels and batch adjustments, separate from `products:write`, so a till cannot rename products |
| `pos:sales:write` | Record in-store sales and refunds |
| `pos:sales:read` | Read them back (dashboard, other tills) |
| `locations:read` / `locations:write` | Shops, warehouses and pickup points |
| `backups:read` / `backups:write` | Encrypted POS backups (no longer requested: the cloud backup was removed in 1.0.0, free edition) |
| `devices:self` | Register this device, heartbeat, read its own settings |
| `events:read` | Pull the event feed (long-poll or SSE) |

**Access token (JWT, RS256/ES256, `typ: at+jwt` per RFC 9068). Example claims:**
```json
{
  "iss": "https://dzbuild.com",
  "sub": "84211",
  "aud": "https://api.dzbuild.app",
  "client_id": "dzbuild-pos-windows",
  "scope": "store:read products:read products:write inventory:write orders:read orders:write pos:sales:write events:read",
  "stores": [141, 152],
  "store_roles": {"141": "owner", "152": "owner"},
  "device_id": "dev_7f3a0c9e2b",
  "cnf": {"jkt": "0ZcOCORZNYy-DWpqq30jZyJGHTN0d2HglBV3uiguA4I"},
  "plan": {"141": "pro", "152": "unlimited"},
  "iat": 1790560000, "exp": 1790560900, "jti": "c8e1…"
}
```

**ID token:** the existing claim set (`sub`, `iss`, `aud`, `exp`, `iat`, `auth_time`, `nonce`, `email`, `email_verified`, `name`, `given_name`, `family_name`), plus `locale` (`ar-DZ`/`fr-DZ`) and `picture`.

**Token response:**
```json
{
  "access_token": "eyJ…", "token_type": "DPoP", "expires_in": 900,
  "refresh_token": "dzrt_2b6…", "scope": "…", "id_token": "eyJ…",
  "stores": [{"store_id":141,"store_name":"Boutique Amel","plan":"pro","role":"owner","public_url":"https://amel.dzbuild.app"}]
}
```

### 6.2 REST API additions (`https://api.dzbuild.app/v1`)

Existing conventions stay unchanged: the envelope, cursor pagination and `Idempotency-Key`.

| Method and path | Scope | Purpose | Closes gap |
|---|---|---|---|
| `GET /v1/me` | openid | Account, owned stores, plans | G11 |
| `GET /v1/products?updated_since=ISO8601&include=variants,combinations,images&status=` | products:read | Delta catalog | G4 |
| `GET /v1/orders?updated_since=ISO8601&status=` | orders:read | Delta orders, including status changes | G5 |
| `GET /v1/changes/feed?cursor=` (read-only, app-accessible) or `GET /v1/events?cursor=&wait=25` | events:read | Unified change/event feed with long-poll for desktops | G3, G4 |
| `GET /v1/events/stream` (SSE) | events:read | Push alternative | G3 |
| `POST /v1/products/batch` | products:write | Upsert up to 100 products by `external_id` or `sku` in one call, returning an id map | G6, G15 |
| `external_id` on products, options, combinations, customers, categories (unique per store, at most 64 characters) | — | Idempotent POS↔DZ mapping | G15 |
| `barcode` on variant options and combinations | products:* | Per-size/colour EAN-13, UPC or internal codes | G8 |
| `POST /v1/inventory/adjustments/batch` | inventory:write | Up to 500 `{product_id\|external_id, target, id, delta\|set, reason: pos_sale\|pos_return\|restock\|count\|loss, ref}` across **many products**. Kept out of the merchant's undo history; appears in a stock-movements log | G6 |
| `GET /v1/inventory/levels?updated_since=&location_id=` | inventory:read | Levels across products | G4 |
| `GET/POST/PATCH /v1/locations` | locations:* | Shops, warehouses, pickup points (link to the Pickup-from-Store points) | multi-shop |
| `POST /v1/pos/sales` | pos:sales:write | Record a completed in-store sale; **not** a COD order (details below) | G7 |
| `POST /v1/pos/sales/{id}/refunds` | pos:sales:write | Full or partial refund; restocks by line | G7 |
| `GET /v1/pos/sales?from=&to=&location_id=` | pos:sales:read | Reporting | G7 |
| `POST /v1/pos/closures` | pos:sales:write | Upload the Z report (cash-drawer close): totals by payment method, over/short, cashier | dashboard |
| `POST /v1/media/uploads` | products:write | Pre-signed upload (`{filename, content_type, size, sha256}`), returning an `upload_url` and a `media_id` to attach to `POST /v1/products/{id}/images {media_id}` | G9 |
| `GET /v1/store` → add `stock_deduction: "on_confirm"\|"on_create"`, `timezone: "Africa/Algiers"`, `currency: "DZD"`, `plan_limits{active_products, orders_per_month}` | store:read | Correct stock logic and cap display | G10, G13 |
| `POST /v1/backups` → `{backup_id, upload_url(s)}`; `PUT` parts; `POST /v1/backups/{id}/complete`; `GET /v1/backups`; `GET /v1/backups/{id}/download` | backups:* | **Client-side-encrypted** POS database backup; DZBuild stores ciphertext only. No longer needed: the POS dropped the cloud backup in 1.0.0 (free edition) | G14 |
| `POST /v1/devices` / `POST /v1/devices/{id}/heartbeat` / `DELETE /v1/devices/{id}` | devices:self | Device registry: name, OS, app version, last seen; owner-visible and revocable | G2 |
| `POST /v1/orders/{id}/claim` | orders:write | A till claims an online order for preparation, to avoid two tills preparing it | multi-till |

`POST /v1/pos/sales`:
- A sale has lines (price as charged, discount, tax), payments (`cash`, `card_cib`, `edahabia`, `baridimob`, `credit`, `cheque`, `transfer`, `voucher`), `cashier_ref` and `location_id`.
- Stock is decremented immediately with `reason=pos_sale`.
- It does not count toward online order caps, triggers no buyer notifications, and appears in analytics as channel `pos`.

**Example: `POST /v1/pos/sales`**
```http
POST /v1/pos/sales HTTP/1.1
Host: api.dzbuild.app
Authorization: DPoP eyJ…
DPoP: eyJ…
Idempotency-Key: pos:dev_7f3a0c9e2b:sale:000018342
Content-Type: application/json

{
  "external_id": "T-2026-000018342",
  "location_id": 3,
  "occurred_at": "2026-09-28T11:42:07+01:00",
  "cashier_ref": "caissier-02",
  "customer": {"external_id": "C-0931", "phone": "0550123456"},
  "lines": [
    {"product_external_id": "P-1203", "combination_id": 88121, "sku": "SAC-CUIR-NOIR", "barcode": "6130000012345",
     "quantity": 1, "unit_price": 4500, "discount": 500, "tax_rate": 19}
  ],
  "totals": {"subtotal": 4500, "discount": 500, "tax": 0, "total": 4000},
  "payments": [{"method": "cash", "amount": 5000}, {"method": "change", "amount": -1000}],
  "channel": "pos"
}
```
→ `201 {"data": {"id": 99120, "external_id": "T-2026-000018342", "stock_applied": true}, "meta": {…}}`

**Example: `POST /v1/inventory/adjustments/batch`**
```json
{"items": [
  {"product_external_id": "P-1203", "target": "combination", "id": 88121, "delta": -1, "reason": "pos_sale", "ref": "T-2026-000018342"},
  {"sku": "CAFE-250G", "target": "product", "set": 42, "reason": "count", "ref": "INV-2026-09-28"}
]}
```

### 6.3 New webhook and event types

| Event | Payload | Why |
|---|---|---|
| `product.created` / `product.updated` / `product.deleted` | id, external_id, changed fields | Online edits reach the POS |
| `inventory.level_changed` | product/option/combination, old→new, reason, source (`dashboard`, `order`, `api`, `pos`) | Stock drift detection |
| `order.updated` | any field change (address, items) | Currently only status events exist |
| `customer.created` / `customer.updated` | id, phone, is_banned, fraud_score | Blacklist sync to tills |
| `store.plan_changed` | old/new plan, limits | Adjust the publish cap |
| `pos.sale.created` / `pos.sale.refunded` | sale summary | Other tills and the dashboard |
| `device.revoked` / `link.revoked` | device_id | Desktop stops immediately |

Delivery to desktops goes through `GET /v1/events?cursor=` (long-poll) or SSE. Server-to-server delivery keeps the existing `X-DZ-Signature` scheme.

### 6.4 Dashboard and product additions (non-API)

- A **"Caisse / POS" page in the DZBuild dashboard:**
  - linked devices, last sync and version;
  - in-store sales charts next to online;
  - Z-reports;
  - download links for the installer, with a SHA-256 hash and the code-signing certificate subject.
- **App listing** in the DZBuild app directory as a first-party ("verified by DZBuild") app, whose launch URL opens or downloads the POS.
- **Copilot / MCP:** expose POS sales to the existing AI assistant connectors through read-only tools such as "combien j'ai vendu en boutique aujourd'hui?".

---

## 7. Security and privacy checklist for the integration

- [ ] System-browser OAuth only, PKCE S256, `state` (plus `nonce` with OIDC), and a loopback bound to 127.0.0.1 with a one-shot listener and a timeout.
- [ ] No client secret in the binary. Anything shipped in an installer is public.
- [ ] Refresh token in Windows Credential Manager or DPAPI; access token in memory only; no tokens in logs, dumps, exports or backups.
- [ ] Token rotation with a single-flight refresh; `invalid_grant` becomes "re-link needed" without blocking sales.
- [ ] Device-bound tokens (DPoP or equivalent) once DZBuild supports them.
- [ ] Webhook verification on the raw body, a ±300 s window and dedupe by `id` (for the broker or relay).
- [ ] Deterministic `Idempotency-Key` per outbox item; never parallel retries of the same key (the docs warn that two concurrent requests with the same key can both run).
- [ ] Least-privilege scopes, with incremental consent per feature (catalog publishing, orders inbox, WhatsApp).
- [ ] Unlink = revoke + delete credential; server-held store data deleted within 30 days of `app.uninstalled` ([security](https://dzbuild.dev/security.md)).
- [ ] **Never ask for the merchant's DZBuild password or merchant API key.** No scraping or dashboard automation ([security](https://dzbuild.dev/security.md)).
- [ ] Backups are encrypted client-side with a key derived from a user passphrase (Argon2id). DZBuild stores ciphertext only.
- [ ] Code-sign the installer and updates; verify update signatures before install.
- [ ] **Personal data:** customer phone, name and address are personal data. Algeria's personal-data law (Law 18-07 of 10 June 2018, supervised by the ANPDP) likely applies to cloud sync of customer files. **(unverified; legal review required)**

---

## 8. Open questions for the DZBuild team

1. **Track A or Track B first?** Track A (broker) works with today's platform. Track B (first-party public client plus refresh tokens) is cleaner and is recommended as the end state. The MCP server suggests most of Track B's building blocks already exist.
2. **Can the POS be listed as a first-party app despite the "must not contain 'dzbuild'" naming rule?** Suggested names: "DZBuild Caisse" / "DZBuild POS" / "كاشير DZBuild".
3. **Should in-store sales be sent to DZBuild at all (G7)?** It would enable one dashboard, but it means building `/v1/pos/sales`.
4. **Should POS publishing respect plan caps silently (push as `draft`) or up-sell?**
5. **Should the stock-deduction mode, timezone and plan limits be exposed on `GET /v1/store`?** This is cheap to add and removes guesswork.
6. **Should the owner approve tills from the Android app with the device flow?** The Android app has push notifications, so "Approve this cash register?" is a good UX.
7. **Where would backups be stored, and who holds the keys?** The recommendation is zero-knowledge.
8. **Offline license:** confirm the POS will **never** require activation. The recommendation is yes, never.

---

## 9. Sources

**DZBuild developer platform (dzbuild.dev)**
- https://dzbuild.dev/ · https://dzbuild.dev/llms.txt · https://dzbuild.dev/llms-full.txt · https://dzbuild.dev/ar/llms.txt · https://dzbuild.dev/fr/llms.txt
- https://dzbuild.dev/intro.md · https://dzbuild.dev/getting-started.md · https://dzbuild.dev/concepts.md · https://dzbuild.dev/oauth.md · https://dzbuild.dev/scopes.md
- https://dzbuild.dev/webhooks.md · https://dzbuild.dev/whatsapp.md · https://dzbuild.dev/rate-limits.md · https://dzbuild.dev/errors.md · https://dzbuild.dev/api-reference.md
- https://dzbuild.dev/security.md · https://dzbuild.dev/review-guidelines.md · https://dzbuild.dev/changelog.md · https://dzbuild.dev/faq.md · https://dzbuild.dev/build-with-ai.md
- https://dzbuild.dev/skills/dzbuild-apps/SKILL.md · https://dzbuild.dev/agents/AGENTS.md · https://dzbuild.dev/openapi/dzbuild-v1.json
- https://dzbuild.dev/robots.txt · https://dzbuild.dev/sitemap.xml
- https://dzbuild.dev/.well-known/openid-configuration (404) · https://dzbuild.dev/.well-known/oauth-authorization-server (404)

**Discovery and live endpoints (probed 2026-09-28)**
- https://dzbuild.com/.well-known/openid-configuration (200, OIDC) · https://dzbuild.com/oauth/oidc/jwks.json · https://dzbuild.com/oauth/oidc/userinfo (401)
- https://dzbuild.com/.well-known/oauth-authorization-server (404)
- https://mcp.dzbuild.com/.well-known/oauth-authorization-server · https://mcp.dzbuild.com/.well-known/oauth-protected-resource · https://mcp.dzbuild.com/mcp (401 with resource_metadata) · https://mcp.dzbuild.com/
- https://api.dzbuild.app/v1/ping (200) · https://dzbuild.com/oauth/apps/authorize (302 → /login when logged out)

**DZBuild merchant site and docs**
- https://dzbuild.com/fr/ · https://dzbuild.com/fr/pricing · https://dzbuild.com/about · https://dzbuild.com/llms.txt · https://dzbuild.com/robots.txt · https://dzbuild.com/sitemap-docs.xml
- https://dzbuild.com/api-docs/intro · https://dzbuild.com/api-docs/authentication · https://dzbuild.com/api-docs/changelog · https://dzbuild.com/api-docs/webhooks/overview · https://dzbuild.com/api-docs/public-key/events
- https://dzbuild.com/docs/intro · https://dzbuild.com/docs/selling/products · https://dzbuild.com/docs/selling/orders · https://dzbuild.com/docs/addons/stock-management · https://dzbuild.com/docs/addons/pickup-from-store
- https://dzbuild.com/docs/copilot/overview · https://dzbuild.com/docs/addons/overview · https://dzbuild.com/docs/operations/multi-store · https://dzbuild.com/docs/operations/team
- https://dzbuild.com/docs/operations/mobile-app · https://dzbuild.com/docs/operations/google-sheets · https://dzbuild.com/docs/plans/free · https://dzbuild.com/docs/payments/cib-edahabia
- https://dzbuild.com/blog/multi-channel-selling-algeria-one-inventory-every-platform
- Branding assets: https://dzbuild.dev/img/logo-mark.png · https://cdn.dzbuild.app/dzbuild-logo-480x128.svg · https://dzbuild.dev/img/dzbuild-social-card.png · https://dzbuild.com/assets/dz-home2/css/dz-lts.css · https://dzbuild.dev/assets/css/styles.e86d4a13.css

**Third-party and ecosystem**
- https://github.com/DZBuild-com · https://github.com/DZBuild-com/dzbuild-app-starter · https://github.com/DZBuild-com/dzship
- https://fr.trustpilot.com/review/dzbuild.com · https://play.google.com/store/apps/details?id=com.dzbuild.app (listing did not render via fetch) · https://status.dzbuild.app

**Standards referenced for the proposed design** (normative references; not re-read for this report)
- RFC 8252, OAuth 2.0 for Native Apps: https://www.rfc-editor.org/rfc/rfc8252
- RFC 7636, PKCE: https://www.rfc-editor.org/rfc/rfc7636
- OAuth 2.1 draft: https://datatracker.ietf.org/doc/draft-ietf-oauth-v2-1/
- RFC 9700, OAuth 2.0 Security BCP: https://www.rfc-editor.org/rfc/rfc9700
- RFC 8628, Device Authorization Grant: https://www.rfc-editor.org/rfc/rfc8628
- RFC 7009, Token Revocation: https://www.rfc-editor.org/rfc/rfc7009
- RFC 8414, AS Metadata: https://www.rfc-editor.org/rfc/rfc8414
- RFC 9728, Protected Resource Metadata: https://www.rfc-editor.org/rfc/rfc9728
- RFC 9449, DPoP: https://www.rfc-editor.org/rfc/rfc9449
- RFC 9068, JWT Access Tokens: https://www.rfc-editor.org/rfc/rfc9068
- RFC 9207, AS Issuer Identification: https://www.rfc-editor.org/rfc/rfc9207
- RFC 8707, Resource Indicators: https://www.rfc-editor.org/rfc/rfc8707
