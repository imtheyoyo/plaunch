# Journal des modifications

Toutes les versions de PLaunch. Les fichiers à télécharger
(`plaunch.exe` et, depuis la 0.7, son empreinte `plaunch.exe.sha256`)
sont dans l'onglet [Releases](https://github.com/imtheyoyo/plaunch/releases).

## v0.10.2

- **Programme déjà ouvert, lancé depuis l'icône de notification** : une
  boîte de dialogue demande s'il faut lancer une autre instance (Oui),
  afficher la fenêtre déjà ouverte (Non, choix par défaut) ou ne rien
  faire (Annuler).

## v0.10.1

- **Menu de l'icône de notification aux couleurs du thème** : le clic droit
  sur l'icône ouvre désormais un menu clair ou sombre selon le thème choisi
  dans les réglages. En thème « Système », il suit le mode des applications
  de Windows, y compris quand on le change pendant que PLaunch tourne.

## v0.10.0

- **Nouveau nom : PLaunch.** L'exécutable devient `plaunch.exe` et le fichier
  des raccourcis `PLaunch.xml` ; réglages, notes, journal et temps
  d'utilisation : `plaunch.ini`, `plaunch-notes.xml`, `plaunch.log`,
  `plaunch-usage.txt`.
- **Profils** : plusieurs fichiers de raccourcis dans le même dossier
  (`PLaunch-Travail.xml`, `PLaunch-Maison.xml`…). Changement et création
  depuis la barre d'état ou le menu de l'icône ; le dernier profil utilisé
  est rouvert au démarrage.
- **Vérification de la clé** au chargement : alerte si le lecteur est en
  lecture seule ou presque plein.
- **Groupes épinglés** dans la barre des favoris : un clic ouvre le contenu du
  groupe en menus (clic droit sur un groupe → « Épingler dans la barre »).
- **Infobulles détaillées** au survol d'un raccourci : description, cible,
  paramètres, lancements, temps d'utilisation, raccourci clavier, état.

## v0.9.0

- **Code réorganisé** : l'interface est découpée en modules (chargement,
  actions, vues, clavier, fenêtres de dialogue, arbre, statistiques), pour des
  corrections plus rapides et plus sûres.
- **Retour à la version précédente** : après une mise à jour, l'ancien
  exécutable est gardé ; Réglages → Mises à jour → « ↩ Revenir à la
  version… » le remet en place et redémarre (et inversement).
- **Programmes en cours d'exécution** : un point vert les signale ; lancer un
  programme déjà ouvert ramène sa fenêtre au premier plan. Clic droit →
  « Lancer une autre instance » pour en ouvrir une seconde. Réglable.
- **Dupliquer un raccourci** (clic droit → Dupliquer), sans ses statistiques,
  son épinglage ni son raccourci clavier.
- **Sélection multiple** (`Ctrl+clic`, `Maj+clic`) : lancer, épingler,
  déplacer vers un groupe (ordre conservé) ou supprimer plusieurs éléments.
- **Temps d'utilisation** par programme, mesuré en continu (onglet
  Statistiques et panneau de détails), enregistré dans `plaunch-usage.txt`.

## v0.8.1

- **Création d'un PLaunch.xml** : sans fichier au lancement, l'écran d'accueil
  propose d'en créer un à côté de l'exécutable (ou au chemin passé en
  argument), de le créer ailleurs ou d'ouvrir un fichier existant.
- Correctif : le panneau de détails ne devient plus trop étroit au démarrage
  sans fichier.

## v0.8.0

- **Éjection de la clé corrigée** (Windows 11) : un assistant lancé depuis le
  dossier temporaire attend la fermeture de PLaunch puis demande à Windows
  de retirer le périphérique ; un message confirme l'éjection ou indique ce
  qui la bloque.
- **Favoris réorganisables** : glisser-déposer dans la barre ou clic droit →
  « Déplacer à gauche / à droite ».
- **Emplacement choisi à la création** : le formulaire indique où l'élément
  sera créé et permet de choisir un autre groupe ; l'élément est ensuite
  affiché et sélectionné dans l'arbre.
- **Raccourcis clavier saisis au clavier** : cliquer sur le champ puis appuyer
  sur la combinaison (Échap annule, ✖ efface).
- **Alarmes avec calendrier** : date choisie dans un calendrier du mois,
  heure et minutes réglables, raccourcis « +1 h » et « Demain 9:00 ».
- **Nouveaux programmes détectés** : la fenêtre d'ajout s'ouvre d'elle-même
  au démarrage.

## v0.7.0

- **Mises à jour vérifiées** : empreinte SHA-256 publiée avec chaque version
  et contrôlée avant installation.
- **Journal** `plaunch.log` (erreurs, lancements échoués, mises à jour,
  plantages avec pile d'appels).
- **Annuler / rétablir** (`Ctrl+Z` / `Ctrl+Y`, boutons ↶ ↷) pour toute
  modification.
- **Rechargement automatique** de PLaunch.xml modifié par un autre programme.
- **Exécutable allégé** (9,3 Mo au lieu de 16) avec **icône intégrée**.
- **Recherche intelligente** : fautes de frappe, lettres inversées,
  initiales, accents ignorés, programmes les plus lancés en tête.
- **Variables de chemin** `%PLAUNCH%` et `%DRIVE%` ; bouton « Rendre les
  chemins portables ».
- **Lancer tout un groupe**.
- **Raccourci clavier par programme**, actif même fenêtre masquée.
- **Éjection de la clé** depuis le menu de l'icône ou les réglages.
- **Détection de nouveaux programmes** dans un dossier (format
  PortableApps.com reconnu) et **import** d'un dossier de raccourcis `.lnk`.
- **Séparateurs**, **liens web** et **icônes de groupe**.

## v0.6.0

- **Options par raccourci** : paramètres, description, fenêtre normale /
  réduite / agrandie, exécuter en tant qu'administrateur, lancer au
  démarrage de PLaunch.
- **Favoris** : barre sous la recherche, tête du menu de l'icône,
  `Ctrl+1` à `Ctrl+9`.
- **Changement de lettre de lecteur** détecté, avec correction des chemins
  et de « Démarrer avec Windows ».
- **Navigation au clavier** dans l'arbre et les résultats ; **mode compact**.
- **Rappels en notification Windows** (ou boîte de message, ou les deux).
- **Coin actif sur tous les écrans**.
- **Sauvegarde / restauration** de PLaunch.xml, des notes et des réglages
  (zip).
- **Mise à jour intégrée** depuis GitHub Releases.
- **Onglet Statistiques** : totaux, plus lancés, ancienneté, raccourcis
  introuvables avec nettoyage.

## v0.5.0

- **Langue** : français ou anglais (automatique selon Windows), changement à
  chaud.
- **Thème** clair, sombre ou système, **taille du texte** réglable
  (70 à 200 %).
- **Icône personnalisable** : forme et couleur, ou fichier `.ico` / `.exe`,
  appliquée à la fenêtre et à la zone de notification.

## v0.4.0

- **Notes avec alarmes** (onglet Notes), rappels uniques, quotidiens ou
  hebdomadaires, même fenêtre masquée.
- **Coin actif** : la fenêtre s'affiche quand la souris reste dans un coin ou
  contre un bord de l'écran.
- **Démarrer avec Windows**.
- **Fenêtre de réglages** (bouton ⚙) et fichier `plaunch.ini`.

## v0.3.0

- **Édition** : nouveau raccourci ou groupe, modification (`F2`),
  suppression (`Suppr`), chemins enregistrés en relatif.
- **Glisser-déposer** pour réorganiser l'arbre et ajouter des fichiers depuis
  l'Explorateur.
- **Recherche dans le menu Démarrer** (utilisateur et tous les utilisateurs),
  avec « Ajouter à PLaunch ».

## v0.2.0

- **Icônes** des programmes, extraites en tâche de fond.
- **Icône dans la zone de notification** avec menu en cascade des groupes et
  raccourcis.
- **Raccourci clavier global** pour afficher ou masquer la fenêtre.
- **Instance unique** : relancer l'exécutable réaffiche la fenêtre ouverte.

## v0.1.0

- Première version : lanceur egui dont les groupes et raccourcis sont
  rangés dans `PLaunch.xml`, relu et réécrit sans perte.
- Arborescence des groupes, recherche instantanée, vues « Plus utilisés » et
  « Récents ».
- Lancement des programmes, documents et dossiers ; mise à jour des
  compteurs `runcount` et `rundate`.
- Chemins relatifs, racine du lecteur, absolus, UNC et variables
  d'environnement ; raccourcis introuvables barrés.
