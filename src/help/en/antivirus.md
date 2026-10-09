# Antivirus

## Why

An antivirus that checks the shop's data file while a sale is being saved can slow the till down or block a backup.

## What to do

- Add the folder `%APPDATA%\dev.dzbuild.pos` to the exclusions of Windows Security: Virus & threat protection › Manage settings › Add or remove exclusions › Add an exclusion › Folder.
- With another antivirus, add the same folder to its exclusions.
- Never exclude the whole disk.

## Checking

After the change, make a backup by hand: "Back up now" in [Settings › Backups](route:/settings/backups).
