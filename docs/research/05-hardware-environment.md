# 05: POS hardware and computing environment in Algeria, with integration details

*Research date: 2026-09-28. Scope: the hardware a free, offline-first Windows POS for Algerian shops has to drive, and the technical details needed to drive it reliably.*

> **How to read this file**
> - Every product, price and claim has a source URL. Anything I could not confirm is tagged **(unverified)**.
> - Prices are in Algerian dinars (DA / DZD), taken from Algerian e-shops and Ouedkniss snippets in Sept 2026. Prices move with the exchange rate, so treat them as ranges.
> - Ouedkniss pages are a JavaScript SPA and could not be fetched directly. Ouedkniss prices come from search-engine snippets of the listings.
> - Epson's online ESC/POS reference returned HTTP 403. The ESC/POS byte sequences below come from Epson's TM-T20II quick-reference PDF (mirrored by datona.cz), the python-escpos source, and the escpos-printer-db capability profiles.

---

## 0. Executive summary (what this means for the product)

1. **80 mm ESC/POS receipt printers made by Xprinter, Rongta, Smart POS and POSLUX dominate the Algerian market**, at about 11,000–20,000 DA. Epson (TM-T20/T82) is the premium option at 35,000–55,000 DA. All of them take ESC/POS over USB, LAN (TCP 9100), serial or Bluetooth, and all have an RJ11/RJ12 24 V cash-drawer port.
2. **Arabic can't be printed reliably in ESC/POS text mode.** Code-page numbers differ by brand (Epson: PC864 = 37, WPC1256 = 50; Xprinter XP-58: "Arabic" = 22, PC864 = 28), and the printers do no glyph shaping and no RTL. **The only robust approach is to render the receipt (or at least every Arabic line) to a 1-bit bitmap at 576 dots for 80 mm or 384 dots for 58 mm, and send it with `GS v 0` in bands.** Algerian competitors avoid the problem by printing through the Windows driver (GDI), which is also a raster path.
3. **Label printers (Xprinter XP-365B/233B/350B/370B/420B, Godex DT4x/G300, Smart POS S58)** cost about 14,500–42,500 DA. They speak **TSPL** (plus EPL/ZPL/DPL emulation on Xprinter). The most common local label sizes are **20×40, 40×30, 40×60, 45×58 and 50×80 mm**. Arabic on labels also has to be sent as a bitmap (TSPL `BITMAP`).
4. **Barcode scanners are USB-HID keyboard-wedge devices** (Henex, Riva, Smart POS, EliteECR, POSLUX) costing 5,800–23,500 DA, with more 2D/QR models every year. **Algerian PCs use a French AZERTY layout, often with an Arabic layout added**, so a scanner left in US mode types `&é"'(` instead of digits. The software has to be layout-proof: decode `KeyboardEvent.code` or scan codes, detect bursts under 50 ms, and treat the Enter suffix as end of scan.
5. **Scales**: label-printing scales (Rongta RLS1000/1100 at 87,000–105,000 DA, ACLAS LS6, Dibal D900) print **EAN-13 codes with prefix 20–29** carrying embedded weight or price. The POS must parse these with a configurable format and should be able to push its PLU list to the scale. Checkout scales on a serial port are rarer, but the **Toledo 8217** (9600 7E1, `W`) and **CAS ECR (ENQ/ACK/DC1)** protocols cover most of them.
6. **Cash drawers** (8,000–12,500 DA) open with a pulse through the printer (`ESC p 0 t1 t2`). USB drawer-trigger adapters (3,000 DA) also exist.
7. **Terminals**: Algerian all-in-one touch POS units are **3rd/4th-gen Core i3/i5 with 4–8 GB RAM and 128 GB (m)SATA SSD, 15–15.6", at about 85,000–150,000 DA**. Many come with a VFD or a second screen. **Refurbished 4th-gen desktops such as the Lenovo M93 sell for 18,500 DA**, and grey-market Windows 10/11 Pro keys for 1,500–2,000 DA. Plan for weak CPUs, 4 GB RAM and 1024×768 or 1366×768 touch screens.
8. **OS**: StatCounter (Aug 2026, browsing machines only) shows Windows 11 at 52.8%, Windows 10 at 46.0% and Windows 7 at 0.86% in Algeria. **Offline POS boxes are under-counted by this metric.** **Electron needs Windows 10+, and Electron 44 dropped 32-bit Windows builds.** Tauri 2 can target i686 and, with an embedded WebView2 bootstrapper, Windows 7. The WebView2/Edge line for Windows 7 stopped at v109.
9. **Connectivity is unreliable by design.** The state cut internet nationwide during the June 2023 baccalaureate (five cuts of 10 hours each). Power cuts are part of daily life (anecdotal, **(unverified)** with statistics), and vendors sell 650 VA UPS units (7,900 DA) specifically "for the small POS". **The product needs a crash-safe local DB (SQLite WAL + `synchronous=FULL`), online backups (`VACUUM INTO` or the backup API), and never SQLite over an SMB share.**
10. **Multi-till setups** in Algerian software are "monoposte ou multi-caisses" on a LAN: one server PC plus several tills, shared network printers and kitchen printers. The right architecture is **a client/server DB, or a local service that proxies the DB, with store-and-forward on each till**.
11. **Payment terminals (TPE, CIB/Edahabia)** are bank-provided and work standalone. Algerian POS software records the card payment by hand, and ECR integration is not documented anywhere I could find. **Employee clock-in devices (ZKTeco K40/K60 fingerprint and RFID)** are sold locally, and there are open libraries for them (TCP/UDP 4370).

---

## 1. Thermal receipt printers

### 1.1 Market and prices in Algeria (Sept 2026)

| Model | Width | Interfaces | Price (DA) | Seller / source |
|---|---|---|---|---|
| Xprinter XP-80T | 80 mm | USB + LAN | 10,000 (Ouedkniss), 12,500 | Ouedkniss snippet via search; [General IT list](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Xprinter XP-80T | 80 mm | USB + Bluetooth | 13,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Xprinter XP-80U | 80 mm | USB + LAN | 13,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Xprinter T80Q | 80 mm | n/a | 12,500 | [General IT home](https://general-dz.com/) |
| Xprinter 320B | 80 mm | n/a | 14,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Xprinter XP-A260M ("cuisine alarme") | 80 mm | USB + LAN + COM, buzzer for kitchen | 18,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Xprinter XP-D200N | 80 mm | USB (+BT/LAN variants) | 13,800 / 14,000 | [CapMicro](https://capmicrodz.com/imprimante-ticket-xprinter-d200n-usb-lan-bluetooth/), [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/), [Groupe SPV](https://groupespv.com/) |
| Xprinter Q806 ("le modèle le plus courant en Algérie") | 80 mm | n/a, 300 mm/s | 16,500 | [Almawarid hardware guide](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Xprinter Q806K | 80 mm | USB | 17,000 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/) |
| Rongta RP335 | 80 mm | USB + LAN | 12,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/); Ouedkniss snippet |
| Rongta RP332 | 80 mm | n/a | Ouedkniss listing (Blida) | [Ouedkniss](https://www.ouedkniss.com/imprimante-thermique-rongta-rp332-80mm-blida-algerie-d35907354?lang=fr) |
| Smart POS RP80180 | 80 mm | USB + Serial | 11,000 / 17,525 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/), [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/) |
| Smart POS SP-802 | 80 mm | USB + LAN | 11,000 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Smart POS SP-802W | 80 mm | Wi-Fi + USB + LAN | on request | [Almawarid guide](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Smart POS 300I | 80 mm | USB + Serial | 16,500 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Smart POS PR-331 | 80 mm | USB | 18,500 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/) |
| POSLUX Z800 | 80 mm | USB | 15,000 | [General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/) |
| Mobile Bluetooth (Xprinter XP-P801A/B, XP-323B, Smart SP 380/280, RPP-02A, PR58, POSLUX MH80) | 58 / 80 mm | BT / USB | 15,000–26,000 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/), [Groupe SPV](https://groupespv.com/), [Vida-DZ](https://vida-dz.com/product/imprimante-ticket-xprinter-xp-p801a-usb-bluetooth-avec-pochette/) |
| Epson TM-T20 / TM-T82 | 80 mm | USB+serial or Ethernet | 35,000–55,000 | [Almawarid 80mm guide](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/) |
| Sunmi (integrated in Android terminals) | 58 mm | built-in | bundled | [Almawarid 80mm guide](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/) |

Price bands from the Almawarid 2026 buying guide ([source](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/)):
- Xprinter XP-80T / Q80x: **12,000–20,000 DA**. The guide calls it "the de facto standard in Algerian commerce: automatic cutter, parts and support easy to find".
- Xprinter Q838 / A300: 18,000–28,000 DA (USB+LAN+BT).
- Epson TM-T20 / TM-T82: 35,000–55,000 DA ("more durable head, serious warranty").
- 80×80 paper roll: "a few hundred dinars". Observed at **200 DA** ([Groupe SPV](https://groupespv.com/)) and **250 DA** ([Equipos](https://equiposdz.com/fr/produits)).

**Brands seen in Algeria:** Xprinter (dominant), Rongta, Smart POS, POSLUX, Riva, Henex (scanners), EliteECR, HFPOS, NewPOS, OKPOS, Lenvii, Epson (premium), Sunmi (Android). **HPRT, Sewoo, Bixolon and Star** turned up in no Algerian listing I could reach **(unverified presence)**. Plan for them anyway, since they are all ESC/POS.

**Algerian distributors:** General IT (general-it.dz / general-dz.com, Algiers, ships to 58 wilayas), Prova (prova-dz.com), Equipos DZ (equiposdz.com, El Mohammadia), Groupe SPV (groupespv.com), HGOS DZ, CapMicro (Béjaïa), Lido Computer, Click Informatique, Innova DZ, Eurequat Algérie (identification/RFID integrator) ([Almawarid list](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)).

### 1.2 Typical specs (Xprinter XP-80T class)
- Direct thermal, **203 dpi (8 dots/mm), 72 mm printable width on 80 mm paper → 576 dots/line**. The 58 mm class gives 48 mm printable → **384 dots/line** ([General IT XP-80T spec](https://www.general-it.dz/product/imprimante-ticket-de-caisse-xp-80t-usb-lan-xprinter/); dot widths confirmed in [athar PR #96](https://github.com/abdallahelficky-wq/athar/pull/96)).
- Speed 200–300 mm/s, auto-cutter, ESC/POS emulation ([General IT](https://www.general-it.dz/product/imprimante-ticket-de-caisse-xp-80t-usb-lan-xprinter/), [CapMicro D200N](https://capmicrodz.com/imprimante-ticket-xprinter-d200n-usb-lan-bluetooth/)).
- **RJ11 cash drawer port driven at 24 V** ([CapMicro D200N](https://capmicrodz.com/imprimante-ticket-xprinter-d200n-usb-lan-bluetooth/)).
- Font A is 12×24, so 48 columns on 80 mm and 32 columns on 58 mm. Font B gives 64 columns on 80 mm **(standard ESC/POS figures, verify per model)**.
- **Compatibility warning from an Algerian competitor:** Almawarid only prints 80 mm tickets and says 58 mm printers "will print a ticket cut off on the right" ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). **Our product should support 58 mm and 80 mm (and 76 mm impact/kitchen) layouts natively.**
- Almawarid also requires a Windows driver: "a model sold as 'ESC/POS only, no driver' is not for us". We should support **both** RAW ESC/POS and driver-based printing so these cheap printers still work.

### 1.3 How to connect (implementation)

| Interface | How to reach it from a Windows app | Notes |
|---|---|---|
| **USB (vendor driver installed)** | Windows spooler, `OpenPrinter` → `StartDocPrinter` with `DOC_INFO_1.pDatatype = "RAW"` → `WritePrinter` → `EndDocPrinter` ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/win32/printdocs/sending-data-directly-to-a-printer)) | The most compatible path. Works with any installed printer queue, including the "Generic / Text Only" driver. |
| **USB (no driver)** | Direct USB via WinUSB/libusb | Needs driver swapping (Zadig). **Not recommended** for end users. |
| **LAN** | Raw TCP to **port 9100** (e.g. `tcp://192.168.0.99:9100` in [node-thermal-printer](https://github.com/Klemen1337/node-thermal-printer)) | Best for multi-till and kitchen printers. Needs a fixed IP; Wi-Fi models need a DHCP reservation ([Almawarid](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/)). The Xprinter factory IP is usually printed on the self-test page (often 192.168.123.100, **unverified**). |
| **Serial (COM)** | `\\.\COM3`, 9600/19200/38400/115200 8N1 | Some Smart POS/Xprinter kitchen models still ship with COM ports ([General IT list](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/)). |
| **Bluetooth** | Classic SPP virtual COM port on Windows | Mainly for mobile printers. |

**Libraries** (all MIT unless noted):
- **Node:** [node-thermal-printer](https://github.com/Klemen1337/node-thermal-printer). Supports Epson, Star, Tanca, Daruma and Brother, TCP or `printer:Name` (driver), `printImage`, `openCashDrawer()`, `cut()`, `beep()`, QR/PDF417/Code128, and **PC864_ARABIC / WPC1256_ARABIC** code pages.
- **.NET:** [ESC-POS-.NET](https://github.com/lukevp/ESC-POS-.NET). Serial/USB/BT/Ethernet/Wi-Fi, raster images, Automatic Status Back (paper/cover/drawer events), Windows 7+.
- **PHP:** [escpos-php](https://github.com/mike42/escpos-php), tested on Xprinter XP-58/XP-80C/XP-90/**XP-365B**/XP-Q800, Rongta RP326/RP58/RP80, Epson TM-T20/T82/T88, and more. Has `graphics()`/`bitImage()` fallbacks for printers without the newer commands.
- **Python:** [python-escpos](https://github.com/python-escpos/python-escpos). Constants cited below.
- **Capability database** of per-model code-page tables: [receipt-print-hq/escpos-printer-db](https://github.com/receipt-print-hq/escpos-printer-db).
- **Rust (for a Tauri stack):** write RAW bytes through the `windows` crate's winspool APIs or a TCP socket. The byte protocol is small enough to own **(design recommendation)**.

### 1.4 ESC/POS command reference (bytes you actually need)

Sources: python-escpos constants ([source](https://raw.githubusercontent.com/python-escpos/python-escpos/master/src/escpos/constants.py)), and the Epson TM-T20II ESC/POS quick reference ([PDF](https://www.datona.cz/sites/default/files/support/TM-T20II_escape_sekvence.pdf)).

| Function | Command | Hex |
|---|---|---|
| Initialize | `ESC @` | `1B 40` |
| Select code page | `ESC t n` | `1B 74 n` (table differs per brand; see 1.5) |
| International char set | `ESC R n` | `1B 52 n` |
| **Cancel Chinese/Kanji double-byte mode** | `FS .` | `1C 2E`. Many Chinese-made printers boot in GB18030 mode and turn 8-bit code-page bytes into Chinese glyphs. Send this before any 8-bit text **(Epson-standard command; behaviour per model is (unverified))**. The [athar PR](https://github.com/abdallahelficky-wq/athar/pull/96) documents Arabic UTF-8 bytes printed as CJK. |
| Align L/C/R | `ESC a n` | `1B 61 00/01/02` |
| Bold on/off | `ESC E n` | `1B 45 01` / `1B 45 00` |
| Double width+height | `ESC ! 0x30` or `GS ! 0x11` | `1B 21 30` |
| Full cut / partial cut | `GS V 0` / `GS V 1` | `1D 56 00` / `1D 56 01`. Feed-and-cut: `1D 56 42 n` |
| **Drawer kick** | `ESC p m t1 t2` | `1B 70 00 19 FA` (pin 2, 50 ms on, 500 ms off). python-escpos uses `1B 70 00 32 32` (pin 2) and `1B 70 01 32 32` (pin 5). **m=0 is connector pin 2, m=1 is pin 5, and t1/t2 are ×2 ms** (TM-T20II ref). |
| Real-time drawer pulse | `DLE DC4 1 m t` | `10 14 01 00 05` (t=1–8 ×100 ms). Works even while the printer is busy. |
| Real-time status | `DLE EOT n` | `10 04 01` printer status (**bit 2 = drawer kick connector pin 3 → drawer open/closed sensor**, bit 3 = offline). `10 04 02` offline cause (cover open, paper end). `10 04 04` paper sensor (near-end). |
| Auto Status Back | `GS a n` | `1D 61 n` (the printer pushes status changes; used by ESC-POS-.NET) |
| Barcode height / width / HRI | `GS h n`, `GS w n` (2–6), `GS H n` | `1D 68 n`, `1D 77 n`, `1D 48 02` (HRI below) |
| Barcode EAN-13 | `GS k 67 n d…` | `1D 6B 43 0C <12 digits>` (function B, m=67 = "C") |
| Barcode CODE128 | `GS k 73 n {B data` | `1D 6B 49 n 7B 42 …` |
| **QR code** (GS ( k, cn=49) | model → size → EC → store → print | `1D 28 6B 04 00 31 41 32 00` (model 2) · `1D 28 6B 03 00 31 43 06` (module 6) · `1D 28 6B 03 00 31 45 31` (EC=M) · `1D 28 6B pL pH 31 50 30 <data>` (store; pL+pH·256 = len+3) · `1D 28 6B 03 00 31 51 30` (print) |
| **Raster image** | `GS v 0 m xL xH yL yH d…` | `1D 76 30 00 48 00 yL yH …` for 576 dots (72 bytes/row). Bit=1 prints black, MSB is the leftmost pixel. Epson marks it "obsolete" in favour of `GS ( L`, **but every clone supports it, so it is the safest choice**. |
| Column bit image (fallback) | `ESC * m nL nH d…` | `1B 2A 21 …` (24-dot double density) for very old firmware |
| Buzzer | vendor-specific | python-escpos `ESC B n t` = `1B 42 n t` (kitchen models such as XP-A260M) **(unverified per model)** |
| NV logo | `FS p` / `GS ( L` | Obsolete on Epson. Prefer sending the logo raster every time. |

### 1.5 The Arabic printing problem, and how to solve it

**Why text mode fails:**
- An ESC/POS printer "is pretty dumb — you can print the Arabic characters that the codepage provides and that is it, … right-to-left and glyph shaping simply not supported" ([EscPosEncoder issue](https://github.com/NielsLeenheer/EscPosEncoder/issues/4); search summary). Arabic letters take four contextual forms. Without shaping you get disconnected letters printed left-to-right ([B4X forum thread](https://www.b4x.com/android/forum/threads/esc-pos-thermal-printers-showing-incorrect-arabic-characters.144258/)).
- **Code-page numbers are not portable:**

| Code page | Epson / generic profile (`ESC t n`) | Xprinter XP-58 |
|---|---|---|
| PC720 (Arabic DOS) | 32 | n/a |
| PC864 (Arabic DOS, has presentation forms) | **37** | **28** |
| WPC1256 (Windows Arabic) | **50** | n/a |
| "Arabic" (vendor) | n/a | **22** |
| WCP1252 | 16 | 16 |
| PC437 | 0 | 0 |

Sources: [escpos-printer-db default profile](https://raw.githubusercontent.com/receipt-print-hq/escpos-printer-db/master/data/profile/default.yml), [Xprinter XP-58 table, escpos-printer-db issue #36](https://github.com/receipt-print-hq/escpos-printer-db/issues/36), and the Epson charcode pages list 37 = PC864 and 50 = WPC1256 ([Epson TECH.REFERENCE](https://download4.epson.biz/sec_pubs/pos/reference_en/charcode/supported_codepage.html), via search snippet).
- Users report the same text coming out "inverse and dashed" on an Epson TM-m30 after trying WPC1256, PC864 and PC720 ([esc_pos_utils issue #31](https://github.com/wwandreww/esc_pos_utils/issues/31)).
- Sending UTF-8 with no code page makes Chinese-firmware printers read the Arabic multi-byte sequences as CJK ([athar PR #96](https://github.com/abdallahelficky-wq/athar/pull/96)).

**How others solve it:**
1. **Raster/bitmap mode (recommended).** "The reliable path is to render a monochrome bitmap and send it as raster graphics" ([whizz-tech](https://whizz-tech.com/support/printers/thermal-printer-prints-question-marks/); search summary). The athar project renders the receipt body to an offscreen canvas at printer dot width (384 or 576) with `direction: "rtl"` and the browser's shaping engine. It **thresholds to 1-bit (no dithering, so small text stays sharp)** and sends **`GS v 0` bands of ≤200 dots high**, grouped into ≤256 KB chunks ([PR #96](https://github.com/abdallahelficky-wq/athar/pull/96)).
2. **Windows driver printing (GDI).** This is what Algerian desktop POS software does (Almawarid requires a Windows driver; [source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). The driver rasterizes, so Arabic prints correctly, but it is slower and depends on having the right driver. The B4X thread notes that the same printer printed Arabic correctly from Windows Notepad ([source](https://www.b4x.com/android/forum/threads/esc-pos-thermal-printers-showing-incorrect-arabic-characters.144258/)).
3. **Pre-shaping into PC864 presentation forms** (arabic_reshaper + python-bidi, or ICU converting to IBM864). This is fragile: the printer needs the right code page, lam-alef ligatures are limited, and the fonts are fixed-width ([search summary of Raspberry Pi / DaniWeb threads](https://www.raspberrypi.org/forums/viewtopic.php?t=190531)).
4. **HTML → WebView → image → ESC/POS**, the approach recommended in the B4X thread.

**Recommended pipeline for DZBuild POS (design):**
- Build the receipt as a layout model (lines, columns, alignment, sizes) and render it with the app's own text engine (Chromium/WebView2 canvas, or Skia/HarfBuzz in Rust) to a **576- or 384-pixel-wide 1-bit image**. Include a logo, RTL/LTR mixed lines, and French plus Arabic side by side.
- Send it as `GS v 0` bands of 24–255 rows, then `GS V` to cut and optionally `ESC p` to open the drawer.
- Offer a **"text mode" fallback** for speed on Latin-only receipts (CP858/WPC1252 for French accents, Euro and the "DA" suffix).
- Store a per-printer **profile**: paper width (58/80), dots/line, `GS v 0` vs `ESC *`, cut command, drawer pin, code-page map, density and a max band height. Ship presets for Xprinter, Rongta, Smart POS, POSLUX, Epson and Sunmi.
- Test each profile on the 12 cheapest local models. Raster buffers overflow on some cheap firmware, so band height must be configurable.

### 1.6 Other printing gotchas
- Windows updates can replace vendor drivers with generic class drivers that lack ESC/POS support ([whizz-tech](https://whizz-tech.com/support/printers/thermal-printer-prints-question-marks/)). Provide a "printer doctor" that detects the driver and prints a test page.
- Kitchen/bar printers (XP-A260M with buzzer and LAN, 18,500 DA) exist for restaurants ([General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/)). Support routing by product category to a LAN printer.
- Print head care: clean every 1–2 months with isopropyl alcohol and use quality paper ([Almawarid](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/)). Worth a maintenance reminder in the app.

---

## 2. Label and barcode printers

### 2.1 Market in Algeria

| Model | Tech | Interfaces | Price (DA) | Source |
|---|---|---|---|---|
| Xprinter XP-365B (20–80 mm, 203 dpi, 127 mm/s, direct thermal) | DT | USB (/+BT) | 14,500–18,900 | [Click-DZ via search](https://click-dz.com/xprinter-xp-365b/), [Lido Computer](https://www.lidocomputer.dz/products/imprimante-code-a-barre-xprinter-xp-365b), [Ouedkniss listing](https://www.ouedkniss.com/codes-a-barre-etiqueteuses-%D8%B7%D8%A7%D8%A8%D8%B9%D8%A9-%D8%A7%D9%84%D8%A8%D8%A7%D8%B1%D9%83%D9%88%D8%AF-%D8%A7%D9%84%D8%AD%D8%B1%D8%A7%D8%B1%D9%8A%D8%A9-xprinter-xp-365b-dar-el-beida-alger-algerie-d53634104?lang=fr) |
| Xprinter XP-233B (60 mm) | DT | n/a | 16,500 | [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Xprinter XP-420B | DT | USB / USB+BT (Riva) | 17,500–18,000 | [General IT](https://general-dz.com/), [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/) |
| Xprinter XP-237B, XP-410B, 350B, 370B, 320B | DT | n/a | 18,500–22,500 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/), [Groupe SPV](https://groupespv.com/) |
| Xprinter XP-DT427B | DT (4") | n/a | 28,000 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/) |
| Xprinter TT-424 | Thermal transfer (ribbon) | n/a | 28,500 | [Equipos](https://equiposdz.com/fr/produits) |
| Smart POS S58 | DT | USB + Serial | 17,500–19,500 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/), [General IT](https://general-dz.com/produit/imprimante-code-a-barre-s58-usb-serie-smartpos/) |
| Smart RP-80 (80 mm, network) | DT | LAN | 15,500 | [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Smart POS RP 400H (label/inventory, TT) | TT | n/a | 26,500 | Ouedkniss snippet |
| SPRT SP-L31 mobile 80 mm label | DT | BT | 31,900 | Ouedkniss snippet |
| Godex DT4x | DT | n/a | 35,000 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/) |
| Godex G300 | TT | n/a | 42,500 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/) |
| Zebra / TSC | TT/DT | n/a | on quote via integrators such as [Eurequat Algérie](https://www.eurequat-algerie.com/point-de-vente-pos/) **(no public price found)** | n/a |
| Seaory S26 badge (PVC card) printer | Dye-sub | n/a | 189,000 | [Prova](https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/) |

**Languages:** the XP-365B is "compatible with TSPL / EPL / DPL / ZPL emulation" and also ESC/POS ([search summary, Xprinter/TVCmall/Alibaba pages](https://www.xprintertech.com/xp-365b-xp-365bm-3-inch-label-printer)). Godex uses EZPL (plus emulations), Zebra uses ZPL/EPL, and TSC uses TSPL/TSPL2.

### 2.2 Common label sizes and consumables (Algeria)

| Size (mm) | Qty/roll | Price (DA) | Typical use | Source |
|---|---|---|---|---|
| **20×40** (PVC) | 2000 | 580 | shelf / small product | [General IT](https://general-dz.com/produit/etiquette-code-a-barre-pvc-20x40-2000-etiquette/) |
| 20×40 (thermal) | 2000 | 350 | product barcode | [Prova](https://prova-dz.com/product-category/consommable/rouleau-etiquette-thermique/) |
| **40×30** | n/a | n/a | the only format Almawarid supports for its labels | [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| 40×60 | 1000 | 360 | product/price | Prova |
| 45×58 | 800–2000 | 400 | scale-style / price | Prova |
| 50×80 | 500 | 450 | shelf/price tag | Prova |
| 60×100, 65×102 | 500 | 650–700 | shipping (Yalidine/ZR-style e-commerce labels) | Prova |
| 100×100, 120×100, 100×150 | 300–500 | 900–950 | shipping labels | Prova |
| Round 5 cm | n/a | 750 | stickers | Prova |
| Barcode labels (generic) | n/a | 280 | n/a | [Groupe SPV](https://groupespv.com/) |

**Product requirement:** a label designer with presets for 20×40 (1-up and 2-up on 40 mm stock), 30×20, 40×30, 40×60, 50×30, 58×40, 50×80 and 100×150, plus a custom size. Almawarid's single 40×30 restriction is a weakness we can beat.

### 2.3 TSPL essentials (TSC manual, which Xprinter clones follow)

From the TSC TSPL/TSPL2 programming manual ([PDF](https://shop.mediaform.de/media/wysiwyg/downloads/armilla/TSC_TSPL_TSPL2_Programming.pdf)) and the [Neodynamic example](https://www.neodynamic.com/articles/How-to-print-raw-TSC-TSPL-TSPL2-commands-from-Javascript/):

```
SIZE 40 mm,30 mm        ; label size (mm form supported since fw V6.27 EZ; inch form "SIZE 4,1")
GAP 2 mm,0 mm           ; gap between labels (use BLINE for black-mark stock)
DIRECTION 1
REFERENCE 0,0
DENSITY 8               ; 0-15 darkness
SPEED 4
CODEPAGE UTF-8          ; also 1256 (Arabic), 864, 8859-6 on genuine TSC firmware
CLS
TEXT 10,10,"3",0,1,1,"Prix: 350 DA"
BARCODE 10,60,"EAN13",60,1,0,2,2,"613123456789"   ; types include 128, EAN13, EAN13+2/+5, EAN8, UPCA, 39, ...
QRCODE 220,40,L,4,A,0,"https://dzbuild.dev/p/123"
BITMAP 200,120,<widthBytes>,<heightDots>,0,<raw bytes>   ; mode 0=OVERWRITE,1=OR,2=XOR
PRINT 1,1
```
- **The TSPL `CODEPAGE` table includes 864 (Arabic), 1256 (Arabic), 8859-6 (Arabic) and UTF-8** (TSC manual p.17). TrueType fonts can be downloaded (`DOWNLOAD`) on genuine TSC firmware. **Whether Xprinter clones honour UTF-8 and TTF shaping is (unverified), so render Arabic text as `BITMAP`.**
- In TSC `BITMAP` data, bits appear to be **0 = black dot, 1 = white** in the manual example (the inverse of ESC/POS) **(verify on hardware)**.
- Units: 203 dpi means **8 dots/mm**, so a 40 mm label is 320 dots wide.

**ZPL (Zebra) equivalent:** `^XA ^PW320 ^LL240 ^FO20,20^A0N,28,28^FDPrix 350 DA^FS ^FO20,60^BY2^BEN,60,Y,N^FD613123456789^FS ^XZ`. Arabic needs `^CI28` (UTF-8) plus a TrueType font and advanced layout (`^PA`) on Link-OS firmware **(unverified: the Zebra doc pages did not render)**. Simplest route: render to an image and send it with `^GFA`.

**EPL/DPL:** support for emulation is enough; generate TSPL as the primary language, and ZPL second for Zebra/Godex.

---

## 3. Barcode scanners

### 3.1 Market in Algeria

| Model | Type | Link | Price (DA) | Source |
|---|---|---|---|---|
| Riva 3206C / 3206 CCD | 1D (CCD) | wired USB | 5,800 | [Prova wired](https://prova-dz.com/product-category/materiel/douchette/lecteur-code-barre-avec-fils/), [Equipos](https://equiposdz.com/fr/produits) |
| Henex HC-3206 | 1D | wired | 5,800 | [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Henex HC-3206 2D QR | 2D | wired | 6,500 | [General IT](https://general-dz.com/) |
| Prova EV1000 | 1D/2D | wired | 6,500 | Prova |
| Henex HC 5208 | n/a | wired | 7,500 | Prova |
| EliteECR HC-6052 | n/a | wired | 9,700 | [Groupe SPV](https://groupespv.com/) |
| EliteECR HC-3206R | wireless | 2.4G | 9,500 | Groupe SPV |
| Smart POS SP-5066 | 1D | wireless | 9,800 | [Prova wireless](https://prova-dz.com/product-category/materiel/douchette/lecteur-code-barre-sans-fils/) |
| Riva 3206P | 2D | wireless | 11,000 | Prova |
| Henex HC-3206R 2D USB + BT | 2D | wireless/BT | 12,500 | Prova |
| Henex HC 6052 2D/3D QR (fixed/presentation) | 2D | USB | 12,500 | [Prova fixed](https://prova-dz.com/product-category/materiel/douchette/lecteur-code-barre-fixe/) |
| Smart SP-8602 (fixed, 1D+2D+QR, 10–160 mm) | 2D | USB | 12,500 | [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Henex HC5208R / RC / RCS (with cradle + display) | 1D | wireless | 13,500–19,800 | Prova |
| Henex 300R (1.44" screen, USB + Wi-Fi 100 m, 1D/2D, stores scans) | 2D | wireless | 14,000 | [Equipos](https://equiposdz.com/fr/produits/henex-300r-aff) |
| POSLUX Z822 | n/a | n/a | 14,500 | Equipos |
| Smart SP M40 mini BT | 1D | BT | 17,000 | Prova |
| Smart SP 8608G, YHD-9100D, Henex HC 7060/666/H8000, Smart SP PS80, Prova HBA 2010 (fixed/omni) | 1D–2D | USB | 17,000–19,500 | Prova |
| Smart POS 80 BT 2D | 2D | BT | 18,000 | Ouedkniss snippet |
| Henex H500R | 1D | wireless | 23,500 | Prova |

**Data collectors / PDAs:** POSLUX DT-92 data collector (16 MB, about 400,000 codes, Wi-Fi + USB) at 32,000 DA. POSLUX 35-6 (Android 6, 56 mm printer) at 74,500 DA. Lenvii Q6 (Android 14, 56 mm printer) at 92,000 DA. Rongta I2 (Android 13, 4/64 GB) at 137,500 DA ([Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). **Our product should import batch files from data collectors (CSV/TXT: barcode;qty) for stock counts, and later offer an Android companion.**

**Why 2D matters:** a 1D scanner reads factory EAN codes but can't read the QR labels Almawarid prints ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). If we print QR (for example DZBuild product links), advise 2D scanners. **Default to EAN-13/Code128 internal codes so that 1D scanners still work.**

### 3.2 Scanner behaviour and how to handle it

- **Keyboard wedge (USB HID keyboard)** is the default. The decoded data is typed as keystrokes "as if you typed the data using your keyboard" ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/pos/barcodescanner-configure)).
- **Suffix:** usually **Enter (CR 0x0D)**, sometimes Tab or CR+LF. Prefixes are rare ([barcodescanneremulator guide](https://barcodescanneremulator.dev/guides/hid-keyboard-wedge)).
- **Speed:** a scanner types about **one character per ~10 ms, so an EAN-13 arrives in ≤130 ms**. Keystrokes less than ~50 ms apart indicate a scanner ([search summary of kegi/sequence-listener, keyscannerjs, @wrurik/barcode-scanner](https://github.com/kegi/sequence-listener)).
- **The AZERTY problem (critical in Algeria):** if the scanner emulates a US keyboard on a French AZERTY PC, you get "weird characters instead of digits" ([Melkal support](https://melkal.supporthero.io/article/show/88741-lorsque-je-scanne-un-code-barre-je-vois-a-l-ecran-des-caracteres-bizarres-qui-s-affichent-a-la-place-des-chiffres-du-code-barre), [Hector KB](https://doc.hectorassetmanager.com/kb/fr/articles/comment-configurer-votre-scanneur-de-code-barres-avec-un-clavier-azerty/)). The AZERTY number row types `& é " ' ( - è _ ç à` unshifted. The vendor's fix is to scan a "French keyboard" configuration barcode from the manual.
- **The Arabic layout problem:** on Arabic (101) the number row still gives ASCII digits, but letters become Arabic (Q → ض, A → ش) ([kbdlayout.info KBDA1](http://kbdlayout.info/KBDA1/)). EAN codes survive, but **alphanumeric Code128/QR codes are corrupted whenever the cashier left Windows on the Arabic input language** (Alt+Shift).
- **Layout-proof decoding (design):** in the app's global key hook, read **`KeyboardEvent.code` (physical key, e.g. `Digit1`, `KeyA`) instead of `.key`**, and map it back through a US table whenever a burst under 50 ms is detected. This makes scans independent of the Windows layout (AZERTY/Arabic/QWERTY) and of scanner configuration. Keep a per-device setting ("scanner layout: US / FR / auto-detect via test barcode").
- **Focus:** wedge input goes to whichever field has focus. The POS should capture scans globally on the sale screen (hidden buffer) so a scan never lands in a quantity or price box.
- **Alternatives to wedge:** *USB-COM (CDC virtual serial)* mode, found on most Henex/Honeywell-class scanners **(verify per model)**, bypasses keyboard layouts entirely. *HID POS Scanner* mode works with Windows' built-in `Windows.Devices.PointOfService` driver, and Bluetooth scanners need SPP-SSI mode for WinRT ([Microsoft Learn](https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/pos/barcodescanner-configure)). Support wedge by default and serial as an option.
- Barcode standards context: the GS1 Algeria company prefix is **613** ([GS1 Algeria](https://www.gs1.dz/), [Wikipedia list](https://en.wikipedia.org/wiki/List_of_GS1_country_codes)). A search snippet of the Ministry of Commerce FAQ says barcodes became mandatory on products for human consumption from **29 March 2023** ([commerce.gov.dz FAQ](https://commerce.gov.dz/fr/questions-frequentes?page=47)) **(snippet only, verify)**.

---

## 4. Electronic scales

### 4.1 Label-printing scales (supermarkets, butchers, fruit and veg)

| Model | Specs | Price (DA) | Source |
|---|---|---|---|
| Rongta RLS1000 | 15/30 kg, dual display, label printer | 87,000 (Ouedkniss) | [Ouedkniss listing](https://www.ouedkniss.com/autre-balance-etiqueteuse-rongta-rls-1000-el-harrach-alger-algerie-d18480362), search snippet; also Mostaganem/Kouba listings |
| Rongta RLS1100 / 30 kg | n/a | 105,000 | [Prova](https://prova-dz.com/product-category/materiel/pos-2/balance/) |
| "TM-30A2" price-computing scale with printer | n/a | 95,000 | Prova |
| ACLAS LS6 (and other ACLAS) | PLU management, label editing, network | on quote | [General IT](https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-ls6/) |
| Dibal D900 | label scale | n/a | [SARL PRO PESAGE / Baxtran Algérie (Facebook video)](https://www.facebook.com/Baxtran.Algerie/videos/balance-%C3%A9tiqueteuse-dibal-d900/682954575869110/) |
| CAS, Mettler Toledo | n/a | **(no Algerian listing found)** | n/a |
| Innova POS DZ | sells Rongta RLS1000 | n/a | [Facebook](https://www.facebook.com/innovaposdz/videos/disponible-des-balances-%C3%A9tiqueteuse-%EF%B8%8F%EF%B8%8Frongta-rls-1000-%EF%B8%8F%EF%B8%8F-capacit%C3%A9-jusqua-10-000-/232657018539604/) |

**Rongta RLS1000/1100 specs** ([Rongta](https://www.rongtatech.com/rls1000-rls1100-barcode-label-weighing-scale-15kg-30kg_p37.html)): **Ethernet RJ45 (TCP/IP 100M), 1×RS232, optional Wi-Fi**; up to **10,000 PLUs**, 224 hotkeys; label width up to 56 mm; 203 dpi; barcodes ITF-2/5, Code128, **EAN-13**, GS1-128; PC software for PLU editing and label design. A Rongta manual on ManualsLib describes a "TCP/IP protocol interface specification … handshake flowchart and data packet format" ([ManualsLib](https://www.manualslib.com/manual/1509483/Rongta-Technology-Rls1000.html), search snippet). The spec itself was not fetched **(unverified details)**.

**ACLAS SDK:** `AclasSDK.dll` (Windows) and `AclasSDKConsole` (CLI) download PLU and note files to LH51, LS M3, TS and other scales, in LS2 barcode formats ([Scribd: Aclas Sync SDK V2.0](https://www.scribd.com/document/835641327/Aclas-Sync-SDK-V2-0-EN), search summary).

**Competitor features:** INNOVA DZ advertises "scale integration with article updates to prevent duplicate programming" ([innova-dz.com](https://www.innova-dz.com/Solutions-point-de-vente.html)). Almawarid does **not** support scales yet ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). **This is an opportunity.**

**Integration plan (design):**
1. **Parse scale barcodes at checkout (must-have).** See 4.2.
2. **PLU export:** CSV/XLSX export in the column layout expected by the Rongta/ACLAS/Dibal PC tools (v1). Then a direct push: Rongta TCP spec, ACLAS SDK (a 32/64-bit DLL called from a sidecar), and Dibal **(v2, needs the vendor docs)**.
3. Map DZBuild product ↔ PLU number (1–99999) ↔ hotkey.

### 4.2 EAN-13 embedded weight/price barcodes (prefix 20–29)

- GS1 reserves **prefixes 020–029 for restricted circulation** (in-store, variable-measure) numbers. Each GS1 member organisation defines its own national layout ([GS1 summary PDF](https://www.gs1.org/docs/barcodes/SummaryOfGS1MOPrefixes20-29.pdf); 403 on fetch, context via [Wikipedia EAN](https://en.wikipedia.org/wiki/International_Article_Number) and the GS1 Algeria site). **The Algerian national layout is (unverified)**, and in practice each scale is configured by the installer.
- Common layouts (all 13 digits, with the last digit being the EAN-13 check):

| Layout | Pattern | Example | Source |
|---|---|---|---|
| Weight, 5-digit PLU | `PP CCCCC WWWWW K` (weight in grams) | `20 10010 01750 K` = 1.750 kg | [Loyverse](https://help.loyverse.com/help/barcodes-with-embedded-weight) |
| Weight, 4-digit PLU + internal check | `PP CCCC X WWWWW K` (digit 7 = check) | `29 0828 ? 00274 K` = 0.274 kg | [Dotypos](https://manual.dotypos.com/eanpluavazenykod.html) |
| Price, 5-digit PLU | `PP CCCCC $$$$$ K` (price in DA) | `22 00123 00450 K` = 450 DA | common scale option **(format standard; prefix choice unverified for DZ)** |
| Price with 4-digit PLU + price check | `PP CCCC X $$$$$ K` | n/a | common in FR/EU scales **(unverified)** |

- **Parser requirements (design):** configurable list of prefixes (20–29, often 21/22/27/28/29). PLU length 4/5/6. Value = weight (g, 3 implied decimals), price (DA, 0 or 2 implied decimals) or count. Optional internal check digit. Validate the EAN-13 check digit. If the price is embedded, compute qty = price / unit price and flag discrepancies. **DZD prices fit 5 digits (max 99,999 DA) because centimes are not used in practice.** Allow 6-digit variants (and the EAN-13 "02" UPC-style) for high-value meat.

### 4.3 Checkout (price-computing) scales on serial

| Protocol | Serial settings | Commands | Response | Source |
|---|---|---|---|---|
| **Mettler Toledo 8217** | **9600 baud, 7 data bits, Even parity, 1 stop** | `W` weight, `Z` zero, `T` tare, `C` clear; 0.2 s command delay | `STX <weight> [N] CR` (regex `\x02\s*([0-9.]+)N?\r`); status `STX ?x CR` | [Odoo posbox hw_scale (AwesomeFoodCoops)](https://raw.githubusercontent.com/AwesomeFoodCoops/posbox-image/master/overwrite_before_init/home/pi/odoo/addons/hw_scale/controllers/main.py); 8217 used for NA and EU per [Mettler JavaPOS manual](https://www.mt.com/dam/000ret/2018-documents/MTScale-JPOS%20Windows%20User%20Manual%202019%2003%2029.pdf) (search snippet) |
| **Adam Equipment** | 4800 8N1 | `P\r\n` print/weight, `Z`, `T` | `…<n>kg` | same posbox source |
| **CAS (PD-II / ER / PDN)** | RS232/USB-serial; ECR type selectable (Type 2 on PD-II, **ECR Type 12** on ER Plus) | handshake **ENQ (0x05) → ACK (0x06) → DC1 (0x11) weight data / DC2 all data** | weight frame | [CAS PD-II manual (ManualsLib)](https://www.manualslib.com/manual/1086324/Cas-Pd-Ii.html), [Dotypos CAS ER](https://manual.dotypos.com/cert-pripojeniakonfiguracevahyrs232.html), [DarkPOS CAS setup](https://help.darkpos.com/hc/docs/articles/setup-cas-weighing-scales) (search snippets) |
| **SMA / NCI** | n/a | `W`, `S`, `Z`, `T` (different framing from 8217) | n/a | [emupos issue #21](https://github.com/ahmedalrifai/emupos/issues/21) |
| **Dialog-06** | n/a | used in legal-for-trade price transfer (EU) | n/a | Mettler (search snippet) |

**Design:** a scale driver module with pluggable protocols (Toledo 8217, CAS ECR types, Adam, SMA, and "continuous output" regex mode for cheap Chinese scales), auto-baud probing (2400/4800/9600, 7E1/8N1), and a live weight widget with stable/unstable status. USB scales usually show up as a CH340/PL2303 virtual COM port.

---

## 5. Cash drawers

| Model | Connector / notes | Price (DA) | Source |
|---|---|---|---|
| Riva PM (RV 330R), 33×36×9 cm, 4 notes + 8 coins | RJ11 to printer | 8,500–10,000 | [Prova](https://prova-dz.com/product-category/materiel/pos-2/tiroir-de-caisse/), [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/) |
| Riva Métal RV 410R | RJ11 | 12,500 | Prova |
| CDP 405 INOX | n/a | 11,500 | Prova |
| Smart POS compact 350×405×90 | **RJ11** | 9,000 | Prova |
| Flip-top (vertical) | n/a | 8,000 | Prova |
| TE-330 | n/a | 8,000 | [Groupe SPV](https://groupespv.com/) |
| POSLUX CD-405A | n/a | 9,900 | Groupe SPV |
| Smart POS P2 / P1 "RJ12" (plastic/metal/eco), Frequency P2 RJ12, SP 410, POSLUX inox | **RJ12** | n/a | [General IT drawers](https://www.general-it.dz/product-category/code-a-barre/tiroire-de-caisse/) |
| **USB drawer adapter** ("adaptateur tiroir caisse USB") | USB → RJ11 trigger | 3,000 | [General IT](https://general-dz.com/categorie-produit/informatique/) |
| Market range | n/a | **6,000–15,000** | [Almawarid](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/) |

- The drawer plugs into the **printer's RJ11/RJ12 kick port (24 V solenoid)**, not the PC ([CapMicro D200N 24 V](https://capmicrodz.com/imprimante-ticket-xprinter-d200n-usb-lan-bluetooth/); [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). **Check that the RJ11 cable matches the printer**, since pinouts differ ([Almawarid 80mm guide](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/)).
- Open command: `ESC p 0 25 250` (pin 2), with pin 5 as fallback (`ESC p 1 …`). Real-time: `DLE DC4 1 0 5`. Status: `DLE EOT 1` bit 2 reads the drawer-open sensor (pin 3), when the drawer has one (TM-T20II ref, above). **Use it to enforce "close the drawer before the next sale" and to log drawer-open events for anti-theft (drawer opened without a sale).**
- With Windows driver printing you can't send the kick unless the driver has a "open drawer before/after print" option. **Another reason to use a RAW channel** (Almawarid: automatic drawer opening only works in its Windows desktop app, not on Mac/web; [source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)).
- USB drawer adapters usually enumerate as a virtual COM port (open by writing any byte, or toggling DTR) or as HID **(unverified per model)**. Support "COM port kick" as an option.

---

## 6. Customer displays, second screens, touch terminals

### 6.1 Pole/VFD customer displays
- A typical unit is VFD, **2 lines × 20 characters**, serial or USB (self-powered), ESC/POS-compatible. It connects either to the PC or through the printer's pass-through ([search summary: ultracaisse / point2vente](https://www.point2vente.com/48-afficheurs-clients)).
- Algerian example: **COMPOS PL200**, VFD 2×20, serial, command sets **Epson ESC/POS, eMAX/Aedex, UTC, ADM787/788, CD5220, DSP-800** ([compos-dz.com](https://compos-dz.com/shop/pl200-afficheur-client-vfd-interne-48)). Many Algerian all-in-ones include a VFD, for example "I5/4G/128 15″ G53 **AFFICHEUR VFD** POSLUX" and "…AVEC AFFICHEUR CLIENT ET BATTERIE ENIGMA" ([General IT](https://www.general-it.dz/product-category/caisse-tactile-2/)).
- **Commands (implementation):**
  - Epson DM-D / ESC/POS display mode: `ESC @` (1B 40) init; `0x0C` clear; `US C 0` (1F 43 00) cursor off; `US $ x y` (1F 24 x y) move cursor **(US $ from Epson docs, not fetched, standard)**.
  - Bixolon/CD5220-style, as used in the Odoo OCA driver: `1F 43 00` cursor off, `0C` clear, **`1B 6C col row` (ESC l) move cursor**, 9600 baud, text transliterated to ASCII with `unidecode` ([OCA hw_customer_display 8.0](https://raw.githubusercontent.com/OCA/pos/8.0/hw_customer_display/controllers/main.py)).
  - CD5220 line mode: `ESC Q A <text> CR` (upper line), `ESC Q B <text> CR` (lower line) **(commonly documented, unverified here)**.
- **Arabic:** VFDs have fixed 5×7 fonts. Arabic is at best available via a PC864 font table, with no shaping, so **show French/Latin plus Western digits on VFDs** (e.g. "TOTAL 1 250,00 DA") **(design)**.

### 6.2 Second-screen customer display
- Dual-screen terminals are sold locally, for example the Lenvii AK156DG Android at **15.6" + 10.1"**, and HFPOS/OKPOS/NewPOS i5 units with a secondary display ([General IT](https://www.general-it.dz/product-category/caisse-tactile-2/)).
- Implementation: on Windows the second panel is just a second monitor. Open a borderless, full-screen window on `screen.getAllDisplays()[1]` showing the cart, total, change due, a QR code (DZBuild store / e-receipt) and ads/slideshow. **This path supports full Arabic.**

### 6.3 All-in-one touch POS terminals sold in Algeria

| Model (seller) | CPU | RAM | Storage | Screen | Extras | Price (DA) |
|---|---|---|---|---|---|---|
| Smart POS A3 (General IT) | Core i3 | 4 GB | 128 GB SSD | 15″ | n/a | n/a |
| POSLUX G53 (General IT) | Core i5 | 4 / 8 GB | 128 GB SSD | 15″ | VFD customer display | n/a |
| ENIGMA (General IT) | Core i3 | 4 GB | 128 GB mSATA | 15″ | customer display + battery | n/a |
| Smart POS SP-P2 (General IT) | Core i5 | 8 GB | 128 GB SSD | 15.6″ | n/a | n/a |
| HFPOS QA9 (General IT) | n/a | n/a | n/a | 15″ / 15.6″ | n/a | n/a |
| NewPOS CY-100SB / HFPOS N700 / OKPOS N500 / HFPOS PN156S (General IT) | **Core i5 3rd gen** | 8 GB | 128 GB | 15.6″ | Wi-Fi + LAN, display | n/a |
| "I5 3rd gen/8G/128 SSD 15″ with display" (General IT home) | i5 3rd gen | 8 GB | 128 GB | 15″ | display | **85,000** |
| Lenvii AK156DG L1600 (General IT) | ARM (Android) | 4 GB | 64 GB | 15.6″ + 10.1″ | Android | n/a |
| **HFPOS 698** (Equipos) | **Core i5 4th gen** | 8 GB | 128 GB SSD | 15.6″ | USB, RJ45, Wi-Fi, VGA, HDMI, aluminium; 6–12 month warranty | **88,000** |
| P2C-J100 (Groupe SPV) | n/a | n/a | n/a | n/a | n/a | 87,000 |
| Market band (Almawarid) | "i3 or i5, 4 GB minimum, SSD, Windows 10+" | n/a | n/a | 15″ | often ships with preinstalled (sometimes paid) POS software | **90,000–150,000** |

Sources: [General IT caisse tactile](https://www.general-it.dz/product-category/caisse-tactile-2/), [General IT home](https://general-dz.com/), [Equipos HFPOS 698](https://equiposdz.com/fr/produits/hfpos-698), [Groupe SPV](https://groupespv.com/), [Almawarid](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/). J1900 (quad-core Celeron 2.0 GHz, 15.6″) terminals exist in the francophone market ([allocaisse](https://www.allocaisse.com/caisse-tactile/164-caisse-tactile-pas-cher-pos68.html)), and "prices start from 24,500 DA for cash register solutions" in Algeria according to a search summary **(unverified item)**.

**Budget PC tills:** refurbished **Lenovo M93 at 18,500 DA** (Haswell/4th-gen class), Windows 10 Pro at **1,500 DA** and Windows 11 Pro at **2,000 DA** (evidently grey-market keys), and a 24″ monitor at 19,000 DA ([Equipos](https://equiposdz.com/fr/produits)).

**Implications (design):**
- **Performance floor:** 3rd/4th-gen i3/i5 or Celeron J1900, **4 GB RAM**, SATA/mSATA SSD 64–128 GB, integrated graphics. **Keep the POS's RAM under ~300–500 MB**, cold start under 3 s, no heavy animations or blur.
- **Screen:** 15″ at **1024×768** (older 4:3 panels) or **1366×768** (15.6″). Test the UI at 1024×768 with touch targets ≥ 44 px, and include an on-screen numpad and keyboard (AZERTY + Arabic).
- 4th-gen CPUs are **not officially supported by Windows 11** (Intel 8th gen+ required, from common knowledge), so many tills will run **Windows 10 22H2, which reached end of support on 14 Oct 2025** ([Microsoft lifecycle](https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro)) or unsupported Windows 11 installs. The good news: **Edge/WebView2 keeps updating on Windows 10 22H2 until at least October 2028** ([Microsoft Learn](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems)).
- Edge/WebView2 needs **SSE3** (dropped non-SSE3 CPUs around v126–128). J1900/Core-i CPUs are fine; very old Athlon/Pentium 4 boxes are not ([same source](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems)).
- Almawarid's baseline for its desktop app is "Windows 10+" ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)).

---

## 7. Windows versions, 32-bit, and the stack consequences

### 7.1 Windows share in Algeria (StatCounter, August 2026)
| Version | Share |
|---|---|
| Windows 11 | **52.76%** |
| Windows 10 | **46.02%** |
| Windows 7 | 0.86% |
| Windows 8.1 | 0.19% |
| Windows 8 | 0.12% |
| Windows XP | 0.04% |

Source: [StatCounter Windows version share, Algeria](https://gs.statcounter.com/windows-version-market-share/desktop/algeria). Desktop vs mobile traffic is 50.05% / 49.2% ([StatCounter](https://gs.statcounter.com/platform-market-share/desktop-mobile-tablet/algeria)). The desktop OS page shows macOS at 37% in Algeria ([StatCounter](https://gs.statcounter.com/os-market-share/desktop/algeria)), which is implausible for the retail base and suggests the sample is noisy or bot-affected.

**Caveat:** StatCounter measures **machines that browse the web**. An offline POS box running Win7 32-bit in a superette never shows up. Local anecdotes suggest Win7 lingers on old tills **(unverified; no hard data found)**. Recommendation: **Windows 10/11 x64 is the primary target. Win7/32-bit is a best-effort "legacy build" only if the chosen framework allows it cheaply.**

### 7.2 Framework compatibility facts
| Framework | Minimum Windows | 32-bit (x86) | Source |
|---|---|---|---|
| **Electron ≥ 23** | **Windows 10+** (Win7/8/8.1 dropped; Electron 22 was the last, supported until 10 Oct 2023) | **Electron 44 removed win32-ia32 prebuilt binaries**; current README says Windows builds are x64 and arm64 only | [Electron Win7 notice](https://www.electronjs.org/blog/windows-7-to-8-1-deprecation-notice), [Breaking changes](https://www.electronjs.org/docs/latest/breaking-changes), [README](https://raw.githubusercontent.com/electron/electron/main/README.md) |
| **Tauri 2** | "Windows 7 and later"; WebView2 preinstalled on Win10 1803+ | **i686 target supported** (`rustup target add i686-pc-windows-msvc`) | [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/), [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/) |
| **WebView2 runtime** | Edge/WebView2 support for Win7/8/8.1 **ended at v109** (Jan 2023) | x86 runtime available | [Microsoft Learn](https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems) |
| Tauri WebView2 install modes | `downloadBootstrapper` (online), **`embedBootstrapper` (+1.8 MB, recommended for Win7)**, **`offlineInstaller` (+127 MB, no internet)**, `fixedVersion` (+180 MB) | n/a | [Tauri Windows installer](https://v2.tauri.app/distribute/windows-installer/) |
| WebView2 offline install | `MicrosoftEdgeWebView2RuntimeInstaller{X64/X86/ARM64}.exe /silent /install`; detect via `pv` regkey under `...\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}` | n/a | [Microsoft Learn distribution](https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution) |
| ESC-POS-.NET | Windows 7+ | n/a | [GitHub](https://github.com/lukevp/ESC-POS-.NET) |

**Consequence for the stack decision:** the installer should work **fully offline** (bundle the WebView2 offline installer, or check for it and fall back to Electron x64). If 32-bit or Win7 tills turn out to matter, **Tauri (i686 + embedded bootstrapper + a pinned last-Win7 runtime) is the only mainstream web-UI option**. A frozen Electron ≤22 legacy build is the alternative, at a security cost.

---

## 8. Internet reliability and power: why crash-safe, offline-first is mandatory

### 8.1 Internet
- Penetration is about **76.9% (36.2 M users, 2024)**. Algérie Télécom is the dominant fixed operator (ADSL → FTTH/MSAN), and LTE launched in 2016 ([Wikipedia: Internet in Algeria](https://en.wikipedia.org/wiki/Internet_in_Algeria)).
- **State-ordered nationwide shutdowns:** "Algerian authorities cut access to the internet nationwide 5 times in June [2023], for 10 hours each time … to combat exam cheating". That totals 50 hours, at an estimated cost of US$101.9 M ([Top10VPN Cost of Internet Shutdowns 2023](https://www.top10vpn.com/research/cost-of-internet-shutdowns/2023/)). Exam-period disruptions recur yearly (2024+ data not extracted, **(unverified)**).
- **Design:** DZBuild sync must be **opportunistic** (outbox queue, idempotent upserts, retry with backoff, and "last synced" UI). Nothing at the till may block on the network, including licence checks and OAuth token refresh. Use a long offline grace period.

### 8.2 Power
- Algerian vendors sell a line-interactive 650 VA UPS for **7,900 DA**, explicitly for "PC + screen + modem/router + small till (POS)" ([Lido Computer](https://www.lidocomputer.dz/products/onduleur-650-va-maxipower-4-sorties)). Surge-protected power strips (5,500 DA) appear next to POS gear ([Equipos](https://equiposdz.com/fr/produits)).
- Summer load-shedding and brief outages are widely reported in daily life, but I found **no statistics** (World Bank Enterprise Survey indicators for Algeria returned null) **(unverified)**.
- **Design rules** (from SQLite's official guidance):
  - Keep **`PRAGMA synchronous=FULL`**. In WAL mode, FULL syncs the WAL on every commit and survives power loss, while NORMAL may roll back the last transactions ([sqlite.org/wal](https://www.sqlite.org/wal.html), [howtocorrupt](https://www.sqlite.org/howtocorrupt.html)).
  - **Never** run with `synchronous=OFF`. **Never** copy the DB file while it is open. Back up with the **backup API or `VACUUM INTO`** (safe on a live DB), and verify with `PRAGMA integrity_check` ([howtocorrupt](https://www.sqlite.org/howtocorrupt.html)).
  - Cheap USB sticks "lie" about fsync, so **don't run the live DB from a USB drive**. Use them only as rotation targets for `VACUUM INTO` backups.
  - Write receipts, journal and stock movements in **one transaction per sale** (atomic), with an append-only sale journal (hash-chained for tamper-evidence) so a half-written sale can't exist.
  - On startup, run a quick `integrity_check` and auto-restore from the last good snapshot.
  - Use a HID-UPS (Windows battery API) to show a banner and force a checkpoint and backup when running on battery **(design)**.
  - Add AV exclusion guidance for the data folder.

---

## 9. Multi-terminal LAN setups

**What Algerian products offer:**
- "Monoposte ou multi-caisses" on Windows (ITRI-POS, sold by General IT) ([source](https://general-dz.com/produit/logiciel-de-gestion-commerciale-vetement-itri-pos/)).
- LAN printers "enable sharing across multiple checkout stations" ([Almawarid](https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/)).
- Kitchen printers with LAN ports ([General IT](https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/)).
- Scale networks: ACLAS "gestion en réseau" ([General IT](https://www.general-it.dz/product/balance-etiqueteuse-code-a-barre-aclas-ls6/)).
- The usual pattern is one "server" PC (often the back-office/manager PC) holding the database and several tills on a switch. The database technology used by Algerian vendors is rarely published. WinDev/HFSQL C/S, SQL Server Express, Firebird and MySQL are commonly cited in the francophone Maghreb **(unverified, no vendor page confirmed)**.

**Hard constraint from SQLite:** do **not** put a SQLite file on an SMB share for several tills. Network filesystem locking is unreliable and causes corruption, and WAL mode doesn't work across machines. SQLite's own advice is to use a client/server DB (**PostgreSQL recommended**) or a **proxy service on the DB host** ([sqlite.org/useovernet](https://www.sqlite.org/useovernet.html), [WAL](https://www.sqlite.org/wal.html)).

**Recommended topology (design):**
```
[Server PC]  POS app in "server mode" = local HTTP/WebSocket service + DB (SQLite WAL local, or embedded PostgreSQL)
     |  LAN (switch, fixed IPs / mDNS discovery "dzpos-server.local")
     +-- [Till 1]  app in "client mode", local SQLite cache + outbox (can keep selling if server is down)
     +-- [Till 2]  ...
     +-- [LAN printer: kitchen / shared label printer]  raw TCP 9100
     +-- [Label scale RLS/ACLAS] TCP/IP PLU push
     \-- (optional) internet -> DZBuild cloud sync (OAuth via dzbuild.dev)
```
- Tills sell from a local cache (prices, stock snapshot). Sales are written locally first, then pushed to the server (idempotent, UUIDv7 IDs). The server resolves stock and pushes changes back over WebSocket.
- Each till has its own ticket-number series (prefix per till), so numbering stays unique even when a till is offline.
- A pairing flow (QR or PIN), plus auto-discovery through mDNS or UDP broadcast, avoids the IP headaches common in shops.

---

## 10. Employee identification: fingerprint, RFID, badges

- **Clock-in devices:** ZKTeco **K-40** and **K-60 PRO** "pointeuses" are sold by General IT, along with a pointeuse management software ([search results: general-dz.com K-40](https://general-dz.com/produit/pointeuse-k-40-zkteco/), [K-60 PRO](https://general-dz.com/produit/pointeuse-k-60-pro-zkteco/), [logiciel pointeuse](https://general-dz.com/produit/logiciel-de-gestion-la-machine-pointeuse/)). Groupe SPV also has a "Pointeuse" category ([groupespv.com](https://groupespv.com/)).
- **Integration:** the ZK binary protocol over **TCP/UDP port 4370**.
  - [pyzk](https://github.com/fananimi/pyzk) (GPL-2.0): users, attendance, fingerprint templates, live capture; K40 verified.
  - [node-zklib](https://github.com/caobo171/node-zklib) (ISC, no native deps): `getUsers()`, `getAttendances()`, `getRealTimeLogs()`.
  - [PyZatt](https://github.com/adrobinoga/pyzk) (MIT).
  - **Feature idea:** import clock-in logs into the POS's staff and shift module (hours, lateness, payroll export).
- **Login at the till:**
  - (a) **125 kHz EM4100 RFID USB readers** act as HID keyboards (10-digit card number + Enter), so they reuse the scanner pipeline **(common behaviour, unverified per model)**.
  - (b) **Barcode/QR staff badges** on credit-card-size cards (85.6×54 mm). Almawarid uses a 1D scanner for club/member cards and mentions that PVC card printers cost about 150,000 DA ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). Prova sells a Seaory S26 badge printer at 189,000 DA.
  - (c) USB fingerprint readers (ZKTeco ZK4500/SLK20R class, ZKFinger SDK) are an optional plugin **(unverified local availability)**.
- **Chifa card readers** (CNAS health card, for pharmacies) are sold by General IT ([general-dz.com categories](https://general-dz.com/)). This is relevant if a pharmacy vertical is targeted.

---

## 11. Card payment terminals (TPE)

- Cards in Algeria: **CIB** (interbank, via GIE Monétique/SATIM) and **Edahabia** (Algérie Poste, launched 7 Dec 2016, usable "to pay for purchases in shops equipped with Eftpos terminals"; BaridiMob cardless since Dec 2019) ([Wikipedia: Algérie Poste](https://en.wikipedia.org/wiki/Alg%C3%A9rie_Poste)). CIB internet payments have been operational since October 2016 ([giemonetique.dz](https://giemonetique.dz/)).
- **Integration reality:** TPEs are supplied by banks and run as **standalone** devices. Algerian POS software does not integrate them. Almawarid lists "bank payment terminals: separate transaction, checked manually in the register" as not supported ([source](https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/)). I found no public ECR-link specification for SATIM/CIB terminals **(unverified whether one exists)**.
- Reference implementation of ECR integration elsewhere: Ingenico/Sagem **Telium "Concert" protocol (E+)** over USB-serial at 9600 baud, sending the amount and currency (Odoo OCA `hw_telium_payment_terminal`, [README](https://raw.githubusercontent.com/OCA/pos/12.0/hw_telium_payment_terminal/README.rst)). Many Algerian bank TPEs are Ingenico/PAX **(unverified)**, so Concert may work where the bank enables it.
- **Design:** v1 records card payments manually (method "CIB/Edahabia", last-4 and authorisation code optional, plus end-of-day reconciliation against the TPE batch report). Keep a pluggable payment-terminal interface for a future Concert/ECR driver. Also support **QR-based payments** (BaridiMob/wallet) as a "paid by QR" method with a reference field.

---

## 12. Other peripherals seen in Algerian POS shops
- **Bill counters and counterfeit detectors:** SH-2830 at 15,000 DA, Ribao BC-55 at 172,000 DA, SH-208C at 260,000 DA, PR-103D detector at 18,500 DA ([Groupe SPV](https://groupespv.com/), [Equipos](https://equiposdz.com/fr/produits)). Not integrated with the POS, but end-of-day cash counting should accept a per-denomination breakdown (DA notes 200/500/1000/2000, coins 5–200) **(design)**.
- **Touchscreen monitors** sold separately ([Prova "Ecran tactile" category](https://prova-dz.com/product-category/materiel/pos-2/)). Windows sees them as HID touch digitizers, with nothing special to do beyond a touch-friendly UI.
- **ESL (electronic shelf labels)** are offered by Eurequat Algérie ([source](https://www.eurequat-algerie.com/point-de-vente-pos/)). Niche, v3+.

---

## 13. Implementation checklist (derived)

| Area | Must-have v1 | Later |
|---|---|---|
| Receipt printing | RAW ESC/POS via spooler + TCP 9100 + COM; **raster Arabic** (576/384 dots, `GS v 0` bands); text-mode fallback; per-printer profiles (Xprinter/Rongta/SmartPOS/POSLUX/Epson/Sunmi); logo; QR; cut; drawer kick; status polling | ASB events, NV logo, Bluetooth printers |
| Label printing | TSPL generator + bitmap Arabic; presets 20×40, 40×30, 40×60, 50×30, 58×40, 50×80, 100×150; EAN-13/Code128/QR; batch print from purchase receipt | ZPL/EZPL output, TTF download |
| Scanner | Global wedge listener, **layout-proof decoding via `KeyboardEvent.code`**, burst timing, suffix handling, test page | USB-COM mode, HID-POS (WinRT) |
| Scales | EAN-13 20–29 parser (weight/price/count, PLU length, internal check); CSV/XLSX PLU export for Rongta/ACLAS/Dibal tools | Direct PLU push (Rongta TCP, AclasSDK), serial checkout scale (Toledo 8217, CAS ECR, Adam, continuous) |
| Drawer | Printer kick (pin 2/5), USB-COM kick, open-without-sale audit log | Drawer sensor enforcement |
| Displays | Second-screen customer display (Arabic OK); VFD 2×20 (ESC/POS, CD5220 dialects) | Ads playlist |
| Terminals | Works at 1024×768, 4 GB RAM, 3rd/4th-gen i3/i5, J1900; on-screen AZERTY+Arabic keyboard | Android companion |
| OS | Win10/11 x64 offline installer with bundled WebView2 or Electron runtime | Win7/x86 legacy build (Tauri i686) if demand is proven |
| Data safety | SQLite WAL + `synchronous=FULL`, atomic sale transactions, `VACUUM INTO` backups (scheduled + on-demand + USB rotation), integrity check at boot, UPS-aware | Hash-chained journal export |
| Multi-till | Server mode + client mode over LAN, local cache/outbox on tills, per-till ticket series, mDNS discovery | Multi-store sync via DZBuild cloud |
| Staff | PIN + badge (barcode/RFID wedge) login | ZKTeco import (TCP 4370), fingerprint USB |
| Payments | Manual CIB/Edahabia/QR method + reconciliation | ECR (Concert) driver where the bank allows |
| Data collectors | Import CSV/TXT inventory batches (POSLUX DT-92 class) | Android PDA app |

---

## 14. Source list (main)
- Almawarid (Algerian competitor), hardware guide: https://almawarid.app/blog/materiel-compatible-caisse-code-barres-algerie/
- Almawarid, 80 mm printer guide 2026: https://almawarid.app/blog/imprimante-ticket-caisse-80mm-algerie-guide-achat/
- General IT, receipt printers: https://general-dz.com/categorie-produit/code-a-barre/imprimante-ticket-de-caisse/
- General IT, touch POS: https://www.general-it.dz/product-category/caisse-tactile-2/
- General IT, drawers: https://www.general-it.dz/product-category/code-a-barre/tiroire-de-caisse/
- General IT, XP-80T spec: https://www.general-it.dz/product/imprimante-ticket-de-caisse-xp-80t-usb-lan-xprinter/
- Prova, label printers: https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-code-a-barre-thermique/
- Prova, receipt printers: https://prova-dz.com/product-category/materiel/imprimante-2/imprimante-bobine-de-caisse/
- Prova, scanners (wired/fixed/wireless): https://prova-dz.com/product-category/materiel/douchette/
- Prova, drawers: https://prova-dz.com/product-category/materiel/pos-2/tiroir-de-caisse/
- Prova, scales: https://prova-dz.com/product-category/materiel/pos-2/balance/
- Prova, labels: https://prova-dz.com/product-category/consommable/rouleau-etiquette-thermique/
- Equipos DZ, products: https://equiposdz.com/fr/produits and https://equiposdz.com/fr/produits/hfpos-698
- Groupe SPV: https://groupespv.com/
- CapMicro, XP-D200N: https://capmicrodz.com/imprimante-ticket-xprinter-d200n-usb-lan-bluetooth/
- Lido Computer, XP-365B & UPS: https://www.lidocomputer.dz/products/imprimante-code-a-barre-xprinter-xp-365b, https://www.lidocomputer.dz/products/onduleur-650-va-maxipower-4-sorties
- COMPOS PL200 VFD: https://compos-dz.com/shop/pl200-afficheur-client-vfd-interne-48
- Rongta RLS1000/1100: https://www.rongtatech.com/rls1000-rls1100-barcode-label-weighing-scale-15kg-30kg_p37.html
- Epson TM-T20II ESC/POS quick reference: https://www.datona.cz/sites/default/files/support/TM-T20II_escape_sekvence.pdf
- python-escpos constants: https://raw.githubusercontent.com/python-escpos/python-escpos/master/src/escpos/constants.py
- escpos-printer-db: https://raw.githubusercontent.com/receipt-print-hq/escpos-printer-db/master/data/profile/default.yml ; Xprinter XP-58 table: https://github.com/receipt-print-hq/escpos-printer-db/issues/36
- Arabic raster approach: https://github.com/abdallahelficky-wq/athar/pull/96 ; https://www.b4x.com/android/forum/threads/esc-pos-thermal-printers-showing-incorrect-arabic-characters.144258/ ; https://github.com/wwandreww/esc_pos_utils/issues/31 ; https://whizz-tech.com/support/printers/thermal-printer-prints-question-marks/
- TSPL manual: https://shop.mediaform.de/media/wysiwyg/downloads/armilla/TSC_TSPL_TSPL2_Programming.pdf
- Microsoft RAW printing: https://learn.microsoft.com/en-us/windows/win32/printdocs/sending-data-directly-to-a-printer
- Microsoft barcode scanner modes: https://learn.microsoft.com/en-us/windows/apps/develop/devices-sensors/pos/barcodescanner-configure
- Keyboard wedge guide: https://barcodescanneremulator.dev/guides/hid-keyboard-wedge
- Weighted EAN: https://help.loyverse.com/help/barcodes-with-embedded-weight ; https://manual.dotypos.com/eanpluavazenykod.html
- Scale protocols (posbox): https://raw.githubusercontent.com/AwesomeFoodCoops/posbox-image/master/overwrite_before_init/home/pi/odoo/addons/hw_scale/controllers/main.py
- OCA customer display: https://raw.githubusercontent.com/OCA/pos/8.0/hw_customer_display/controllers/main.py ; OCA Telium: https://raw.githubusercontent.com/OCA/pos/12.0/hw_telium_payment_terminal/README.rst
- ZKTeco libs: https://github.com/fananimi/pyzk ; https://github.com/caobo171/node-zklib
- StatCounter Algeria: https://gs.statcounter.com/windows-version-market-share/desktop/algeria
- Electron: https://www.electronjs.org/blog/windows-7-to-8-1-deprecation-notice ; https://www.electronjs.org/docs/latest/breaking-changes
- Tauri: https://v2.tauri.app/start/prerequisites/ ; https://v2.tauri.app/distribute/windows-installer/
- Edge/WebView2 OS support: https://learn.microsoft.com/en-us/deployedge/microsoft-edge-supported-operating-systems ; https://learn.microsoft.com/en-us/microsoft-edge/webview2/concepts/distribution
- Windows 10 lifecycle: https://learn.microsoft.com/en-us/lifecycle/products/windows-10-home-and-pro
- SQLite: https://www.sqlite.org/wal.html ; https://www.sqlite.org/useovernet.html ; https://www.sqlite.org/howtocorrupt.html
- Internet shutdowns: https://www.top10vpn.com/research/cost-of-internet-shutdowns/2023/ ; https://en.wikipedia.org/wiki/Internet_in_Algeria
- GS1 Algeria: https://www.gs1.dz/
