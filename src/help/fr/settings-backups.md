# Sauvegardes

Vos ventes, votre stock et votre journal des opérations sont gardés dans un seul fichier de données sur ce PC. Les sauvegardes sont des copies vérifiées de ce fichier.

## Ce qui est automatique

- Une sauvegarde au premier démarrage de chaque jour, à chaque clôture (Z), avant chaque mise à jour et, si vous le laissez activé, à la fermeture.
- Chaque copie est relue et vérifiée avant de compter.
- Les plus anciennes copies automatiques sont supprimées après la durée de conservation choisie ; vos ventes, elles, ne sont jamais supprimées.

## La copie de secours sur clé USB

- « Choisir une clé USB ou un dossier » : chaque sauvegarde y est copiée et vérifiée.
- Si la clé est absente, un bandeau le dit ; la sauvegarde du PC, elle, est faite.
- « Sauvegarder sur une clé USB maintenant » copie une sauvegarde fraîche sur la clé branchée.
- La copie de secours n'est jamais dans le dossier de données de DZBuild POS, ni dans un dossier synchronisé avec Internet (OneDrive, Google Drive, Dropbox) : vos ventes, vos clients et les codes PIN partiraient sur Internet sans protection. Emportez régulièrement une copie chez vous : une copie gardée ailleurs résiste à un vol ou à un incendie.

## Restaurer

- « Aperçu et restauration » montre la boutique, le dernier ticket et les comptes de la sauvegarde avant de décider.
- Seul le propriétaire restaure, avec son code PIN ; l'application redémarre sur la sauvegarde.
- Si l'application ne démarre plus, voyez [Restaurer une sauvegarde](help:restore).

## Emporter vos données

Dans « Exporter mes données », « Exporter une sauvegarde (.dzpos) » crée un fichier à garder ailleurs ou à importer sur un autre PC ([changer de PC](help:move-pc)).

Dans [Réglages › Données](route:/settings/data), « Exporter toutes les données (zip) » donne chaque liste (produits, clients, ventes…) en CSV pour Excel, et « Importer un fichier de sauvegarde » ajoute un fichier .dzpos à la liste des sauvegardes sans rien remplacer.
