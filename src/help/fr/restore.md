# Restaurer une sauvegarde

Quand le fichier des données de la boutique est endommagé (disque en panne, coupure de courant), DZBuild POS affiche un écran rouge au lieu de s'ouvrir. Rien n'est perdu tant qu'une sauvegarde existe.

## Sur l'écran rouge

1. Cliquez « Restaurer une sauvegarde ».
2. Choisissez la sauvegarde : la plus récente est proposée ; celles de la clé USB apparaissent quand vous la branchez.
3. « Vérifier cette sauvegarde » montre la boutique et le dernier ticket qu'elle contient.
4. Choisissez le propriétaire et tapez son code PIN (celui de cette sauvegarde).
5. « Restaurer et redémarrer ».

## Ce qui se passe

- Les fichiers endommagés sont déplacés dans le dossier des sauvegardes (corrupt-…), jamais effacés.
- L'application redémarre sur la sauvegarde ; la restauration est notée au journal des opérations.
- Les ventes faites après la sauvegarde n'y sont pas : gardez les tickets papier de cette période. Les numéros déjà imprimés sont listés au démarrage.

## Si ça ne marche pas

- « Trop de codes PIN erronés » : attendez 15 minutes.
- « Version plus récente » : installez d'abord la dernière version de DZBuild POS.
- Sinon, exportez le [diagnostic](help:settings-diagnostics) et contactez l'assistance.
