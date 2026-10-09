# Network of tills

Several PCs in the shop: a main PC holds the books, the tills sell on its copy, even when the network is down.

## Main PC or till

- **Single PC**: one PC holds the shop. This is the setting you start with.
- **Main PC**: it holds the books (stock, customers, the Z of every till) and serves the tills. Leave it on during opening hours.
- **Till**: it sells on a copy of the main PC's data and sends it every operation.
- Only the owner changes a PC's role. The shop's settings (identity, sales rules, backups, updates) are set on the main PC; on a till they are read-only.

## Adding a till

1. On the main PC, in [Settings › Network](route:/settings/network), "Make this PC the main PC".
2. Then "Add a till": a 6-digit pairing code shows, valid for 10 minutes and once only.
3. On the new PC, at the first screen, choose "Till of a shop" (or, when it is already in use, "Join a main PC").
4. Pick the main PC found, or type its "Main PC address", then the "Pairing code (6 digits)".
5. Give the till its "Till series" (C2, C3…): its receipt numbers carry it.

## The firewall

- The "Windows Firewall" card of the main PC says whether the tills can reach it.
- The installer adds the rules "DZBuild POS LAN" (TCP 7345) and "DZBuild POS mDNS" (UDP 5353), for private networks.
- When Windows classes the shop's network as public, make it private in Windows settings.
- An administrator can also add the rules with the commands shown ("Copy").

## When the network is down

- A till keeps selling offline: its receipts, payments and Z are kept, "to send".
- As soon as the main PC answers, everything goes, in order. "Send and receive now" sends again.
- Customer credit (kridi) waits for the main PC: the balances are there.
- An offline Z needs a manager's approval.

## Entries waiting for the main PC

- After a sale made on a till, the main PC records its register entries (controlled medicines, jewellery), its IMEIs, flexy top-ups, deposits and vouchers. The "Entries waiting for the main PC" card (Settings › Network, on the till) shows what it has not taken yet: the ticket number, its date, what waits, and the main PC's refusal if there is one.
- The legal register entries depend on no balance: they are taken even when a voucher or a balance is refused. A red badge marks a sale whose register entry was refused; tell the owner.
- A money operation (a deposit, a voucher sold, an instalment sale…) whose answer was lost is kept and sent again by itself: do not enter it again; the cash goes into the drawer when the main PC answers.
- A return of a phone sold by its IMEI, of a sale that earned points or refunded as an exchange voucher is made on the main PC: the till must be online.
- "Try again" sends what waits at once. A strip at the top of the screen shows the refusals to every user.

## Leaving the network or replacing the main PC

- "Leave the network" (owner's PIN): the till's copy is kept in its backups and the PC restarts as a new PC.
- When the main PC is out of service for good, on a till: "Make this till the main PC". It keeps everything it received.
- A backup is never restored on a till, nor on a main PC that has tills: it would take them back in time.
