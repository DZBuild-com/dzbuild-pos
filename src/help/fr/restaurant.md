# Salle et tickets de table

## La salle

En mode Restaurant, la caisse s'ouvre sur la salle. Chaque table montre son code, ses couverts, les minutes depuis son ouverture et depuis le dernier bon de cuisine, et le total de son ticket :

- **blanche** : libre ;
- **jaune** : occupée ;
- **orange** : addition imprimée.

« Non imprimé » signale un bon de cuisine qui n'est pas sorti : réimprimez-le depuis la liste des impressions. La salle se met à jour toute seule toutes les 20 secondes.

Touchez une table libre, tapez le nombre de couverts puis « Ouvrir la table ». Une table occupée ouvre son ticket (plusieurs tickets : choisissez-le).

Au clavier, les flèches passent d'une table à l'autre et Entrée l'ouvre ; les couverts se tapent sur les chiffres, puis Entrée ouvre la table.

## Le comptoir

À droite, « À emporter » ouvre un ticket numéroté du jour, « Livraison » demande l'adresse et le téléphone, et chaque ticket prédéfini (Comptoir, Terrasse…) a son bouton. Les tickets du comptoir en cours sont listés dessous.

« Vente directe » ouvre l'écran de vente classique ; Échap ouvre le menu.

## Le ticket de table

Touchez les produits à gauche (le menu du jour en premier) ou scannez-les. Un produit avec des options demande d'abord le choix (« Choisir 1 », « Jusqu'à 3 ») ; « Ajouter » attend les choix obligatoires.

Au clavier, F2 revient à la recherche : tapez le nom, ↓ choisit le produit et Entrée l'ajoute. Dans les options, les flèches passent d'un choix à l'autre, Espace le coche et Entrée ajoute la ligne.

- Choisissez la **place** (À partager, 1, 2…) et le **service** (Entrée, Plat, Dessert, Boisson) avant d'ajouter.
- Une ligne pas encore envoyée se modifie : quantité, note pour la cuisine, place, service, ou retrait.
- « Envoyer en cuisine » (F6) imprime un bon par poste de préparation ; les lignes envoyées portent une coche.

## Annuler, offrir, diviser

- **Annuler** une ligne envoyée (F8) demande la quantité et un motif ; la cuisine reçoit un bon d'annulation. Sans le droit, un responsable donne son accord.
- **Offrir** (F5) met la ligne à 0 avec son motif ; le droit de remise ou l'accord d'un responsable est demandé.
- **Addition** imprime le ticket à payer (« ce document n'est pas un ticket de caisse »), partagé en parts égales si besoin.
- **Diviser** déplace des articles ou une place vers un nouveau ticket ; **Transférer** change de table ; **Fusionner** réunit deux tickets.

## Encaisser

« Encaisser » (F12) ouvre le paiement habituel : espèces, carte, chèque… Le droit de timbre et le rendu viennent du logiciel. Une fois payé, le ticket de caisse est imprimé et la salle revient.

## Plusieurs postes de caisse

Un ticket de table reste sur le poste de caisse qui l'a ouvert : il y est servi, envoyé en cuisine et encaissé. Les autres postes de caisse ne le reprennent pas (servir une même table depuis plusieurs postes n'est pas encore possible). Les salles, tables, postes de préparation et options se règlent sur le poste principal et arrivent sur chaque poste de caisse.

- [ ] Ouvrir une table avec ses couverts
- [ ] Envoyer en cuisine et vérifier le bon
- [ ] Encaisser et retrouver la table libre
