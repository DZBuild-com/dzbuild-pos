# Backups

Your sales, stock and action log are kept in one data file on this PC. Backups are checked copies of that file.

## What is automatic

- A backup at the first start of each day, at every close (Z), before every update and, if you leave it on, when the app closes.
- Each copy is read back and checked before it counts.
- The oldest automatic copies are deleted after the time you chose to keep them; your sales are never deleted.

## The second copy on a USB stick

- "Choose a USB stick or a folder": every backup is copied there and checked.
- When the stick is missing, a banner says so; the PC's backup is still made.
- "Back up to a USB stick now" copies a fresh backup to the stick plugged in.
- The second copy is never inside DZBuild POS's data folder, nor in a folder synced with the Internet (OneDrive, Google Drive, Dropbox): your sales, customers and PINs would go to the Internet without protection. Take a copy home regularly: a copy kept elsewhere survives a theft or a fire.

## Restoring

- "Preview and restore" shows the shop, the last receipt and the counts of the backup before you decide.
- Only the owner restores, with their PIN; the app restarts on the backup.
- When the app no longer starts, see [Restore a backup](help:restore).

## Taking your data with you

In "Export my data", "Export a backup (.dzpos)" makes a file to keep elsewhere or to import on another PC ([moving to another PC](help:move-pc)).

In [Settings › Data](route:/settings/data), "Export all the data (zip)" gives each list (products, customers, sales…) as a CSV file for Excel, and "Import a backup file" adds a .dzpos file to the list of backups without replacing anything.
