# Licences — the owner's guide

DZBuild POS is sold per PC. A licence is a **Trial** (works 7 days from the day you issue it) or
**Lifetime**. Each licence names one PC by its **device code** (`DZPOS-XXXX-XXXX-XXXX-XXXX`, shown on
the customer's activation screen) and is signed with **your private key**. The app checks the
signature offline with the public key built into it: nobody can make a licence without your key,
and no internet is ever needed.

Your tool is `dzpos-license` (`dist-owner\<version>\dzpos-license.exe` after `pnpm release:build`,
or `cargo run --release -p pos-license-cli -- <command>`). It is never in an installer: keep it with
the key.

## Once: the licence key — done

The licence key **`k1` was made on 2026-10-04** in `%USERPROFILE%\DZBuild POS licence key\`, and its
public half is in `crates/pos-license/src/keys.rs` (`PUBLIC_KEYS`, `KEYRING_SHA256`): every release
since 0.8.0 accepts the licences it signs. **Never run `keygen` again**, except for a rotation with a
new id (`--id k2`, § Rotation). The tool refuses an id the release already has (a second, different
`k1` would refuse every licence the first one signed) and, when the release has keys, prints a snippet
that **appends** to `PUBLIC_KEYS` instead of replacing it.

The folder holds `licence-signing.key` (the private key, sealed by the passphrase),
`licence-signing.pub`, `ledger.json` (every licence you issued, with the customers' names and phones)
and `issued\` (a copy of every `.dzlic`).

### k1: the passphrase file

The run that made `k1` left its passphrase in **`passphrase.txt`** in that folder. Put the
passphrase in your password manager, then **delete `passphrase.txt` before copying the folder to the
USB sticks** (`keygen-output.txt`, also there, holds only the public key and may stay). Until the file
is gone, `dzpos-license issue` prints a warning at each licence (it never reads the file).

## Keep the key safe

- Keep the folder on **two USB sticks in two places** (the shop and home), never in a cloud folder,
  never in the repository, never sent by WhatsApp or e-mail.
- Keep the passphrase in a password manager, not beside the sticks.
- After issuing, copy `ledger.json` to the second stick too (it is how the tool refuses a second
  trial for the same PC).
- Losing the key: the installed copies keep working, but you cannot issue licences for them any more
  until a release carries a new key (§ Rotation). Losing the passphrase is the same as losing the key.

## Issue a licence from a WhatsApp message

1. The customer installs DZBuild POS. The activation screen shows the PC's code and a QR code; the
   QR opens a WhatsApp chat with you holding the code (when your number is in `src-tauri/license.toml`).
2. Copy the code from the message. Then:

```powershell
$env:DZPOS_LICENSE_KEYS = "E:\DZBuild POS licence key"
dzpos-license issue --device DZPOS-XXXX-XXXX-XXXX-XXXX --kind trial --customer "Supérette El Amel" --phone "0550 12 34 56"
dzpos-license issue --device DZPOS-XXXX-XXXX-XXXX-XXXX --kind lifetime --customer "Supérette El Amel"
```

3. Send back the key it prints (one line starting with `DZL1.`) or the `.dzlic` file from `issued\`.
   The customer pastes the key, or loads the file, on the activation screen (or in
   Réglages › Licence once the shop is set up).

The code has a check character: a code typed with one wrong character, or with two characters
swapped, is always refused before anything is signed (`inspect` also flags a key whose code fails
it). Lower case, spaces, `O` for `0` and `I`/`L` for `1`, `DZP0S` for `DZPOS`, typographic dashes,
Arabic-Indic digits, the invisible marks an Arabic chat adds and the words around the code (paste the
whole message) are accepted. A key pasted with the message around it works too.

Rules the tool keeps: **one trial per PC** (ever); `--days` (1 to 30) for a longer trial; one live
lifetime licence per PC. `dzpos-license list --device <code>` prints a PC's licences and their keys
again (a customer who lost the message).

## What the customer sees

- No licence on a new install: the activation screen (the setup wizard comes after it).
- Trial: a banner counts the days left; it turns orange 3 days before the end.
- Trial ended, licence removed or refused: **read-only mode**. Sales, returns, documents, purchases,
  stock counts and every change are refused; sign-in, reports, exports, backups, restores and the
  fiscal archive keep working (the law requires the shop to keep its books). A cash session already
  open can still be closed and its Z printed.
- Each PC needs its own licence: a till without one cannot join the shop's network or sell.
- The demo shop needs no licence.

## Re-issue after a hardware change

The device code changes when Windows is reinstalled, the disk is replaced or reformatted, the
processor or the motherboard changes, the Windows licence changes, or the PC's disk image is put on
other hardware (reimaging a till from another one's image gives it its own code). Rarely, a firmware
update that rewrites the motherboard's identity or a storage driver that reports the disk
differently changes it too. The app then says that its licence belongs to another PC and shows the
new code; issue the new code with `--replace`.

```powershell
dzpos-license list --device DZPOS-OLD0-…                      # find the old licence id
dzpos-license issue --device DZPOS-NEW0-… --kind lifetime --replace <old licence id>
```

The ledger links the two licences. The old one keeps working on the old PC: if that PC must stop (it
was sold, the customer was refunded), revoke it (§ Revoke). A trial is never moved: issue a lifetime
licence.

## Revoke

```powershell
dzpos-license revoke <licence id>
```

It marks the ledger and prints a line to add to `REVOKED` in `crates/pos-license/src/keys.rs`. A PC
refuses the key once it runs a release with that line (`license_revoked`, read-only mode). A PC that
never updates is not reached: revocation travels with updates only.

## Rotation

To change keys (a key that may have leaked): `dzpos-license keygen --keys <new folder> --id k2`,
**append** its entry to `PUBLIC_KEYS` (keep `k1` until every customer has a `k2` licence), run
`cargo test -p pos-license the_public_keys_match_their_digest` and set `KEYRING_SHA256` to the value
it prints, release, re-issue the customers' licences with the new folder, then remove `k1` in a later
release. Licences signed by a removed key are refused (`license_invalid`).

## Limits

A licence check inside a program the customer runs on their own PC can be delayed, not made
impossible. What DZBuild POS does: the signature makes a keygen impossible without your private key;
the check runs in Rust, not in the screens; the public keys are checked against a digest at every
start; the licence is kept in three places (the shop database, the Windows Credential Manager, a file
under `%LOCALAPPDATA%`) sealed with the PC's identity, and an edited copy is reported (journaled
`license.tampered`) and stops sales; the first trial of a PC is written in the hash-chained audit
journal; the trial's end is measured against the newest time the PC has ever shown, so setting the
clock back does not bring a trial back; and while a trial runs, a clock set back more than ten minutes
behind that newest time stops the sales at once ("set the date": the trial counts from the newest
time the PC showed) until the date is set right, so setting the clock back again and again cannot
freeze a trial either. A lifetime licence ignores the clock. What it cannot stop: someone patching the
program itself, or making another PC report all six identities of a licensed one (the four of Windows,
the motherboard's SMBIOS UUID and the system disk's serial number: possible with firmware tools or in
a virtual machine), or a person who deletes the shop database, the Credential Manager entry and the
file and keeps the clock back for good (their receipts then carry the wrong dates). The release build
is stripped and optimised, which makes patching harder, not impossible.

Tills installed from one disk image get different device codes: the motherboard's SMBIOS UUID and
the system disk's serial number are part of the code (an image copies neither), so each till needs
its own licence. Two clones share a code only when neither their motherboards nor their disks report
an identity (some virtual machines). The other side of it: a motherboard change, or reimaging a PC
onto other hardware, changes the code, and that PC then needs a licence issued with `--replace`
(§ Re-issue). The way the code is computed is frozen since the first licences: no release changes
it.

## Troubleshooting the customer's messages

| The customer sees | Meaning | What to do |
|---|---|---|
| Not a valid licence key | the text is cut or not a key | send the `.dzlic` file |
| Issued for another PC | the code in the key is not this PC's | check the code; issue for the right one |
| Trial already used | this PC had a trial | issue a lifetime licence |
| The PC's date is behind (the key's date, or the newest time this PC showed during a trial) | the PC clock is wrong or was set back | set the Windows date and time right; the trial goes on |
| Licence data altered | a copy of the licence was edited | send the key again (activating it again repairs it) |
| Licence cancelled | the key is in `REVOKED` | issue a new key if the cancellation was a mistake |
