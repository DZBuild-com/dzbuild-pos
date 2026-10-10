# M7 DZBuild link — staging checklist

Run on a clean Windows 11 PC with the current DZBuild POS build, against DZBuild staging (`dzbuild.toml` pointed at staging for this run only; never commit it). Tick each line; note the request id DZBuild returns for any failure.

## Link
- [ ] Settings › DZBuild › Lier la boutique opens Edge on dzbuild.com (not a window of the app); sign in with 2FA; the consent lists the scopes in plain words; the page after the redirect says "Connexion réussie" in the till's language.
- [ ] An account with two stores shows the store choice; the chosen store's name appears in Settings.
- [ ] Lier avec un téléphone: the code and the QR appear; approving on a phone links within 10 s; a wrong code on the phone is refused by DZBuild.
- [ ] Windows Credential Manager › Informations d'identification Windows lists `DZBuild POS/dzbuild-refresh/<id>`; `pos.db`, the logs folder and a fresh backup contain no `rt_`/`at_` token (search the files).
- [ ] DZBuild's connected devices page lists this till with its name and version.

## Sync
- [ ] Publish 5 products (2 with photos): they appear on the storefront with prices TTC and stock; a product beyond the plan cap is a draft with the plan notice.
- [ ] Sell 3 tickets and 1 return at the till: the dashboard shows them with the POS numbers; the stock online drops once (not twice).
- [ ] Pull the network cable, sell 2 tickets, reconnect: the dot turns amber then green; the dashboard gets both, once.
- [ ] Close a Z: the closure appears with its audit and document hashes.
- [ ] Edit a price and a stock level in the dashboard: the till applies both within a cycle; a simultaneous edit at the till shows the "la modification en ligne est gardée" notice.

## Online orders
- [ ] Place a COD pickup order on the storefront: the badge and chime at the till; claim, prepare, cash in: the order shows Delivered online and the stock is not deducted twice.
- [ ] Claim the same order on a second till: refused "préparée par un autre appareil".
- [ ] Ship an order with a BL: the BL names the buyer; the order shows Shipped.
- [ ] Refuse an order: cancelled online with the reason.
- [ ] A buyer flagged on DZBuild shows the warning; the sale is still possible.

## Backups
- [ ] Create the passphrase, print the recovery code; run an online backup; DZBuild shows ciphertext only (download it from the dashboard: `age --decrypt` with the recovery code restores `pos.db`).
- [ ] On a second PC: link, fetch the backup with the passphrase, restore with the owner PIN.

## Unlink and failure modes
- [ ] Revoke the till from DZBuild's devices page: the till shows "Liaison révoquée depuis DZBuild" and keeps selling; nothing is lost.
- [ ] Unlink with the PIN: DZBuild revokes the token; the Credential Manager entry is gone.
- [ ] With DZBuild's staging down, the till sells and closes normally; one "pas encore disponible"/offline state, no toast storm.
