# Removing all data

## First of all

- The law asks you to keep sales, invoices and the action log for 10 years.
- Export them first: "Export my data" in [Settings › Backups](route:/settings/backups) and, for invoices, the [fiscal archive](help:fiscal).

## Uninstalling keeps the data

- Uninstalling DZBuild POS never deletes the folder `%APPDATA%\dev.dzbuild.pos`.
- The box in the uninstaller only removes temporary display files, never your data.

## Really deleting

1. Close the app.
2. Delete the folder `%APPDATA%\dev.dzbuild.pos` by hand: in File Explorer, type this path in the address bar.
3. On the next start, the setup wizard opens.
