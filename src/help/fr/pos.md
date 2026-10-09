# Vendre

L'écran de vente se commande au clavier, au lecteur de codes-barres ou au toucher.

## Ajouter des articles

- Scannez le code-barres : l'article s'ajoute, même si le curseur est ailleurs.
- `F2` recherche par nom en arabe, en français ou en anglais, ou par référence.
- Pour plusieurs articles identiques : tapez la quantité, `*`, puis scannez.
- `+` et `-` changent la quantité de la ligne choisie ; `Suppr` la retire (selon vos droits).

## Remises, client et attente

- `F5` fait une remise sur la ligne ou sur le ticket, dans la limite de votre rôle ; au-delà, un responsable tape son code PIN.
- `F3` choisit le client : ses prix, ses remises et sa carte de crédit (kridi) s'appliquent.
- `F6` met le ticket en attente, `F7` le reprend.

## Encaisser

- `F12` ouvre le paiement. Choisissez le moyen de paiement ; plusieurs moyens peuvent se combiner.
- Le droit de timbre s'ajoute pour les espèces selon le barème ; le rendu s'affiche en grand.
- Validez : le ticket est numéroté, imprimé, et ne peut plus être modifié. Une erreur se corrige par un [retour](help:sales).

## Les modules de votre activité à la caisse

Les touches `Ctrl+1` à `Ctrl+9` servent aux modules allumés dans [Réglages › Activité et modules](route:/settings/activity) ; `F1` affiche les touches de cette caisse.

- `Ctrl+5` remplace la grille par les **touches rayon** : touchez un rayon, tapez le prix (et la quantité), puis « Ajouter ».
- `Ctrl+2` ajoute une **recharge flexy** : l'opérateur se devine d'après le numéro.
- Un téléphone suivi par **IMEI** demande un numéro par pièce ; `Ctrl+4` rouvre la saisie de la ligne choisie.
- À « Encaisser », un **médicament contrôlé** demande l'ordonnance et un **bijou** l'identité de l'acheteur si la fiche du client ne l'a pas.
- Au paiement, « Bon d'achat » lit un bon cadeau, des arrhes ou l'avoir du client ; vos types de paiement (prélèvement CCP…) suivent les moyens habituels, et les points de fidélité gagnés s'affichent.
- Une carte fidélité scannée choisit le client ; le ticket s'imprime dans la langue de sa fiche.
- Menu (`Échap`) › « Vendre depuis le camion » vend le stock d'un camion en tournée jusqu'à « Revenir au magasin ».
- Sur un poste de caisse, un téléphone à IMEI, une recharge flexy, un bon cadeau ou des arrhes sont enregistrés sur le poste principal dès la vente faite : le poste de caisse ne les vend que relié au poste principal.

## Si l'imprimante ne répond pas

La vente est enregistrée quand même. Une notification propose « Réimprimer » ; vérifiez le papier et le câble, puis [Réglages › Matériel](route:/settings/hardware).
