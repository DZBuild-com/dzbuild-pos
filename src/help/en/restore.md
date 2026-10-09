# Restore a backup

When the shop's data file is damaged (disk fault, power cut), DZBuild POS shows a red screen instead of opening. Nothing is lost as long as a backup exists.

## On the red screen

1. Click "Restore a backup".
2. Choose the backup: the newest is offered; those on the USB stick show up when you plug it in.
3. "Check this backup" shows the shop and the last receipt it holds.
4. Choose the owner and type their PIN (the one of this backup).
5. "Restore and restart".

## What happens

- The damaged files are moved to the backups folder (corrupt-…), never deleted.
- The app restarts on the backup; the restore is written in the action log.
- Sales made after the backup are not in it: keep the paper receipts of that period. The numbers already printed are listed at start-up.

## When it does not work

- "Too many wrong PINs": wait 15 minutes.
- "Newer version": install the latest DZBuild POS first.
- Otherwise, export the [diagnostic](help:settings-diagnostics) and contact support.
