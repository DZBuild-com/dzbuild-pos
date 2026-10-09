<p align="center">
  <a href="README.md">English</a> · <b>Français</b> · <a href="README.ar.md">العربية</a>
</p>

<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/dzbuild-pos-logo-horizontal-on-dark.png">
    <img src="assets/brand/dzbuild-pos-logo-horizontal.png" alt="DZBuild POS" width="420">
  </picture>
</p>

<h3 align="center">Logiciel de caisse et de gestion de stock gratuit pour les commerces algériens. Fonctionne hors ligne.</h3>

<p align="center">
  <a href="https://github.com/DZBuild-com/dzbuild-pos/actions/workflows/ci.yml"><img src="https://github.com/DZBuild-com/dzbuild-pos/actions/workflows/ci.yml/badge.svg?branch=main" alt="CI"></a>
  <a href="https://github.com/DZBuild-com/dzbuild-pos/releases"><img src="https://img.shields.io/github/v/release/DZBuild-com/dzbuild-pos?label=version&color=F59E0B" alt="Version"></a>
  <a href="LICENSE"><img src="https://img.shields.io/badge/licence-GPL--3.0--or--later-blue" alt="Licence : GPL-3.0-or-later"></a>
  <img src="https://img.shields.io/badge/Windows-10%20%7C%2011-0078D6" alt="Windows 10 | 11">
  <img src="https://img.shields.io/badge/langues-ar%20%C2%B7%20fr%20%C2%B7%20en-FFC300" alt="Arabe, français, anglais">
</p>

DZBuild POS est un logiciel Windows pour les commerces en Algérie : caisse, stock, crédit client
(kridi), fournisseurs et factures conformes à la loi algérienne. L'interface et les documents
imprimés existent en arabe, en français et en anglais. Il fonctionne sans internet et sans
abonnement.

## Télécharger

Les installateurs sont sur [dzbuild.com/pos](https://dzbuild.com/pos) et dans les
[Releases GitHub](https://github.com/DZBuild-com/dzbuild-pos/releases).

| Installateur | Pour |
| --- | --- |
| 64 bits | La plupart des PC (Windows 10 ou 11, 64 bits). Installe WebView2 s'il manque. |
| 32 bits | Les PC plus anciens sous Windows 10 32 bits. |
| Hors ligne (64 ou 32 bits) | Les PC sans internet. WebView2 est inclus, le fichier se copie sur une clé USB. |

Chaque version fournit un fichier `SHA256SUMS.txt` pour vérifier le téléchargement.

> [!NOTE]
> Les installateurs ne sont pas encore signés. Windows SmartScreen peut afficher « Windows a protégé
> votre ordinateur ». Cliquez sur **Informations complémentaires**, puis **Exécuter quand même**,
> seulement si le fichier vient de dzbuild.com/pos ou de ce dépôt et que son SHA-256 correspond.

## Pourquoi c'est gratuit

DZBuild POS est édité par DZBuild, qui exploite aussi la plateforme de boutiques en ligne
[DZBuild](https://dzbuild.com). Le logiciel est gratuit : pas de clé de licence, pas d'abonnement,
pas de compte. Tout fonctionne hors ligne.

Un commerce qui vend aussi en ligne peut relier sa caisse à une boutique DZBuild (catalogue, stock,
commandes en ligne). Ce lien est facultatif.

## Fonctionnalités

- **Caisse** : vente au clavier et à la douchette, paiements multiples, droit de timbre et rendu
  calculés en direct, tickets en attente, retours et échanges, sessions de caisse jusqu'au Z.
- **Stock** : journal des mouvements non modifiable, inventaires, transferts, lots et péremption
  (FEFO), numéros de série et IMEI, CMUP.
- **Clients et kridi** : vente à crédit avec plafond, versements avec reçu numéroté, relevés,
  ancienneté des créances, fidélité et bons d'achat.
- **Achats** : bons de commande, réceptions partielles, factures et règlements fournisseurs, retours.
- **Documents et fiscalité** : devis, proformas, bons de livraison, factures et avoirs à numérotation
  continue, mentions du décret 05-468, journal d'audit chaîné, 60 rapports dont TVA, timbre et aide
  G50, archive fiscale avec manifeste SHA-256.
- **Impression et matériel** : imprimantes ESC/POS, factures A4/A5 et PDF, étiquettes TSPL et ZPL,
  balances, tiroir-caisse, afficheur client.
- **Plusieurs caisses** : un PC principal et des caisses en réseau local. Une caisse continue de
  vendre si le réseau tombe.
- **Import et export** : Excel (xlsx, xls, ods) et CSV, y compris les anciens encodages Windows.
- **Modèles métier** : 25 modèles d'activité (restaurant, pharmacie, boucherie, téléphonie, gros,
  bijouterie, etc.).

**Ce qu'il ne fera pas** : factures fictives, stock caché, suppression de ventes. C'est illégal en
Algérie. Une erreur se corrige par un avoir ou un retour.

## Compiler

Voir la section « Build from source » du [README anglais](README.md#build-from-source).

## Contribuer

Le projet est maintenu par l'équipe DZBuild. Code, traductions et retours sur le matériel sont les
bienvenus : lisez [CONTRIBUTING.md](CONTRIBUTING.md) (en anglais). Questions dans les
[Discussions](https://github.com/DZBuild-com/dzbuild-pos/discussions). Pour une faille de sécurité,
suivez [SECURITY.md](SECURITY.md), jamais de ticket public.

## Licence

Logiciel libre sous [GNU GPL version 3 ou ultérieure](LICENSE). « DZBuild », « DZBuild POS » et le
logo sont des marques de DZBuild, non couvertes par la licence : voir [TRADEMARKS.md](TRADEMARKS.md).

Liens : [dzbuild.com/pos](https://dzbuild.com/pos) · [aide](https://dzbuild.com/pos/support) ·
[blog](https://blog.dzbuild.com) · [créer une boutique DZBuild](https://dzbuild.com/register)
