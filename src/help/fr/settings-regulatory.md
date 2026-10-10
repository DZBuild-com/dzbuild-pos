# Données réglementaires

Réglages › Données réglementaires montre le pack réglementaire que le logiciel utilise et installe les nouveaux.

## Le pack utilisé

- Les taux de TVA, le barème du droit de timbre, les prix plafonnés, le taux de la zakat et les wilayas viennent d'un fichier signé par DZBuild.
- Le nissab n'en fait pas partie : la boutique le saisit elle-même à chaque année hégirienne, sur l'écran [Zakat](route:/zakat).
- L'écran montre son identifiant, sa version, sa date d'entrée en vigueur et ses sources.

## Installer un nouveau pack

- « Installer un pack signé… » : choisissez le fichier .json du pack, puis son fichier .sig, la signature de DZBuild (sur une clé USB par exemple).
- Un pack sans signature valide ou plus ancien que celui en place est refusé.
- Le pack s'applique au prochain démarrage : « Redémarrer pour l'appliquer ».

## Plusieurs postes

- Un poste de caisse prend le pack du poste principal tout seul (vérifié de la même façon) : installez un nouveau pack sur le poste principal, puis redémarrez chaque poste de caisse quand la bande le demande.
- Une bande prévient quand le pack d'un poste de caisse diffère de celui du poste principal : un poste ne doit jamais appliquer un ancien droit de timbre ou d'anciens plafonds.
