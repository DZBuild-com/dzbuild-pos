# SmartScreen

Windows warns about a downloaded program until its publisher has a reputation. With an OV
certificate the warning names "DZBuild" and fades as installs accumulate (days to weeks); an
unsigned setup shows "Éditeur inconnu" and Smart App Control on Windows 11 may block it.

What to tell a shop (and the resellers) while the reputation builds:

1. On "Windows a protégé votre ordinateur", click **Informations complémentaires**.
2. Check that the publisher is **DZBuild**.
3. Click **Exécuter quand même**.

Never ask a shop to disable SmartScreen or the antivirus. Prefer the offline setup from a USB
stick for shops without a good connection. The Microsoft Store channel (optional, owner) avoids
the warning entirely: it takes the signed offline setup with the silent switch `/S`.
