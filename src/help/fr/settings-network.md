# Réseau de caisses

Plusieurs PC dans la boutique : un poste principal tient les comptes, les postes de caisse vendent sur sa copie, même quand le réseau est coupé.

## Poste principal ou poste de caisse

- **Poste unique** : un seul PC tient la boutique. C'est le réglage de départ.
- **Poste principal** : il tient les comptes (stock, clients, Z de tous les postes) et sert les postes de caisse. Laissez-le allumé pendant les heures d'ouverture.
- **Poste de caisse** : il vend sur une copie des données du poste principal et lui envoie chaque opération.
- Seul le propriétaire change le rôle d'un PC. Les réglages de la boutique (identité, règles de vente, sauvegardes, mises à jour) se font sur le poste principal ; sur un poste de caisse ils sont en lecture seule.

## Ajouter un poste de caisse

1. Sur le poste principal, dans [Réglages › Réseau](route:/settings/network), « Faire de ce PC le poste principal ».
2. Puis « Ajouter un poste de caisse » : un code d'appairage de 6 chiffres s'affiche, valable 10 minutes et une seule fois.
3. Sur le nouveau PC, au premier écran, choisissez « Poste de caisse d'une boutique » (ou, s'il sert déjà, « Rejoindre un poste principal »).
4. Choisissez le poste principal trouvé, ou tapez son « Adresse du poste principal », puis le « Code d'appairage (6 chiffres) ».
5. Donnez au poste sa « Série du poste » (C2, C3…) : ses tickets la portent dans leur numéro.

## Le pare-feu

- Le carton « Pare-feu Windows » du poste principal dit si les postes de caisse peuvent le joindre.
- L'installation ajoute les règles « DZBuild POS LAN » (TCP 7345) et « DZBuild POS mDNS » (UDP 5353), pour les réseaux privés.
- Si Windows classe le réseau de la boutique comme public, passez-le en réseau privé dans les paramètres de Windows.
- Un administrateur peut aussi ajouter les règles avec les commandes affichées (« Copier »).

## Quand le réseau est coupé

- Un poste de caisse continue de vendre hors ligne : ses tickets, versements et Z sont gardés, « à envoyer ».
- Dès que le poste principal répond, tout part, dans l'ordre. « Envoyer et recevoir maintenant » relance l'envoi.
- Le crédit client (kridi) attend le poste principal : ses soldes sont là-bas.
- Un Z hors ligne demande l'accord d'un gérant.

## Écritures en attente du poste principal

- Après une vente faite sur un poste de caisse, le poste principal enregistre ses inscriptions aux registres (médicaments contrôlés, bijouterie), ses IMEI, ses recharges flexy, ses arrhes et ses bons. La carte « Écritures en attente du poste principal » (Réglages › Réseau, sur le poste de caisse) montre ce qu'il n'a pas encore pris : le numéro du ticket, sa date, ce qui attend, et le refus du poste principal s'il y en a un.
- Les inscriptions aux registres légaux ne dépendent d'aucun solde : elles sont prises même quand un bon ou un solde est refusé. Un badge rouge signale une vente dont le registre a été refusé ; prévenez le propriétaire.
- Une opération d'argent (arrhes, bon vendu, vente à tempérament…) dont la réponse s'est perdue est gardée et renvoyée toute seule : ne la refaites pas ; l'argent entre dans le tiroir quand le poste principal répond.
- Un retour d'un téléphone vendu par IMEI, d'une vente qui a donné des points ou remboursé en bon d'échange se fait sur le poste principal : le poste de caisse doit être en ligne.
- « Réessayer » renvoie tout de suite ce qui attend. Une bande en haut de l'écran signale les refus à tous les utilisateurs.

## Quitter le réseau ou remplacer le poste principal

- « Quitter le réseau » (code PIN du propriétaire) : la copie du poste est rangée dans ses sauvegardes et le PC redémarre comme un PC neuf.
- Si le poste principal est hors service pour de bon, sur un poste de caisse : « Faire de ce poste le poste principal ». Il garde tout ce qu'il a reçu.
- Une sauvegarde ne se restaure ni sur un poste de caisse ni sur un poste principal qui a des postes de caisse : elle les ferait revenir en arrière.
