# Regulatory data

Settings › Regulatory data shows the regulatory pack the software uses and installs new ones.

## The pack in use

- VAT rates, the stamp duty scale, regulated prices, the nisab and the wilayas come from a file signed by DZBuild.
- The screen shows its identifier, version, date in force and sources.

## Install a new pack

- "Install a signed pack…": choose the pack's .json file, then its .sig file, DZBuild's signature (from a USB stick, for example).
- A pack without a valid signature, or older than the one in place, is refused.
- The pack applies at the next start: "Restart to apply it".

## Several PCs

- A till takes the main PC's pack by itself (checked the same way): install a new pack on the main PC, then restart each till when its strip asks.
- A strip warns when a till's pack differs from the main PC's: a till must never charge an old stamp duty or old ceilings.
