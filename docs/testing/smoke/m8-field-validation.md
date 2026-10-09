# M8 field validation

Hardware and shop checks for the M8 vertical packs that no automated test can make: real kitchen
printers, a driver-only printer, a checkout scale, a signed data pack, and the legal texts. Run them on
real devices before a release that changes these packs, and record each line as OK / KO with the
device model.

1. Kitchen printers: one network kitchen printer (TCP 9100) and one USB, each mapped to a station; send,
   cancel and reprint a ticket; the buzzer on a model that has one.
2. A driver-only printer (a laser or a GDI-only thermal model) as the receipt printer: a ticket prints as
   a page.
3. A checkout scale in the butcher template: the scale step of the wizard, a cutting run weighed on it.
4. The data-pack key: keep `src-tauri/keys/datapack.key` with the updater key (two USB sticks); sign and
   install a test pack on the PC.
5. Legal review before the first shop uses them: the instalment contract's clauses, the zakat statement's
   wording, the processing-register template, the jeweller register's columns against the wilaya's
   Garantie office.
6. A pharmacy: import the current Chifa reference list file and compare ten prices by hand.

## Record

- Run by, date, devices (model, driver, port): (to fill).

| # | Result (OK / KO) | Notes |
|---|---|---|
| 1 | | |
| 2 | | |
| 3 | | |
| 4 | | |
| 5 | | |
| 6 | | |
