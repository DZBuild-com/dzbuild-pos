# Pharmacie : médicaments, péremptions, liste Chifa et registre

## La fiche médicament

Ouvrez un produit dans **Produits**, onglet **Médicament** (le module Pharmacie doit être activé).

- **DCI** : la dénomination commune. Les médicaments de la même DCI sont listés en dessous, les moins chers d'abord : la caisse les propose comme génériques.
- **Médicament contrôlé** : sa délivrance demande l'ordonnance et s'inscrit au registre. Le **Tableau** (A, B ou C) se tape en 20 caractères au plus.
- **Sur ordonnance** : la caisse demande l'ordonnance.
- Le tarif de référence Chifa s'affiche quand la liste importée connaît le produit. Il est informatif.

## Les péremptions

Dans **Stock**, onglet **Péremptions**, le tableau de bord montre d'abord les lots périmés, puis ceux qui périment dans 7, 30, 60 ou 90 jours, avec leur valeur au coût. Exportez ou imprimez-les avec les boutons Excel, CSV et PDF. Sous le tableau de bord, la liste des lots permet de retirer un lot périmé du stock.

## La liste Chifa

Dans **Produits**, onglet **Liste Chifa** :

1. Choisissez le fichier reçu (Excel, OpenDocument ou CSV).
2. Indiquez la colonne du code, de la désignation, du tarif de référence et, si le fichier l'a, de la colonne « remboursable ».
3. Touchez **Vérifier** : chaque ligne à corriger est listée avec sa colonne.
4. Une fois le fichier sans erreur, touchez **Appliquer** : toute la liste s'applique d'un seul coup.

## Le registre des médicaments contrôlés

Dans **Produits**, onglet **Registre**, réservé aux utilisateurs qui ont la permission du registre. Choisissez la période : chaque délivrance y figure avec son numéro, son ticket, le prescripteur, l'ordonnance et le patient. **Imprimer le registre** enregistre le registre paginé en PDF.

> Ce sont des données de santé : ne les montrez qu'aux personnes autorisées. Le registre est tenu sur le poste principal : un poste de caisse y inscrit chaque délivrance dès la vente faite et ne vend un médicament contrôlé que relié au poste principal. Sur un poste de caisse, le registre se consulte depuis le poste principal et s'imprime sur celui-ci.

- [ ] Renseigner la DCI et le statut contrôlé des médicaments
- [ ] Importer la liste Chifa
- [ ] Vérifier les péremptions chaque semaine
