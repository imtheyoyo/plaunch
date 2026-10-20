# PLaunch

Lanceur portable pour clé USB, écrit en Rust avec [egui](https://github.com/emilk/egui).
Vos groupes et raccourcis sont rangés dans un fichier **`PLaunch.xml`** placé à
côté de l'exécutable : tout reste sur la clé, rien n'est installé sur le PC.

## Fonctions

- Arborescence des groupes et raccourcis, avec le nombre d'entrées par groupe.
- Recherche instantanée sur le nom et le chemin (`Ctrl+F`, `Entrée` lance le
  premier résultat, `Échap` efface).
- Vues **Plus utilisés** et **Récents**, à partir des compteurs de PLaunch.
- Lancement par double-clic (ou simple clic si `singleclick` est activé dans
  le fichier), menu contextuel « Lancer / Ouvrir l'emplacement ».
- Mise à jour de `runcount` et `rundate` à chaque lancement.
- Chemins relatifs au dossier du lanceur (`_HD\…`, `..\PortableApps\…`),
  relatifs à la racine du lecteur (`\Outils\…`), absolus, UNC (`\\NAS\…`) et
  variables d'environnement (`%USERPROFILE%`).
- Raccourcis introuvables barrés ; la vérification se fait en tâche de fond
  pour ne pas bloquer sur les partages réseau.
- Écriture **sans perte** : tout le reste du fichier (notes, couleurs, options,
  capteurs…) est réécrit à l'identique, au même format (tabulations, CRLF,
  entités `&#xE9;`). Une copie `PLaunch.xml.bak` est faite au premier
  enregistrement de chaque session.
- **Icônes** des programmes, extraites en tâche de fond.
- **Icône de notification** : clic gauche pour afficher/masquer, clic droit
  pour un menu en cascade reprenant tous vos groupes et raccourcis (avec leurs
  icônes). Le bouton de fermeture masque la fenêtre au lieu de quitter ;
  `Échap` (recherche vide) la masque aussi.
- **Raccourci clavier global** pour afficher/masquer la fenêtre
  (`Ctrl+Shift+Space` par défaut).
- **Instance unique** : relancer `plaunch.exe` réaffiche la fenêtre ouverte.
- **Édition** : nouveau raccourci ou groupe (bouton `＋`, menus contextuels),
  modification (`F2`) avec sélecteur de fichier, suppression (`Suppr`, avec
  confirmation). Les chemins sur le même lecteur que PLaunch sont
  enregistrés en **relatif**, pour rester portables.
- **Glisser-déposer** : réorganisez raccourcis et groupes dans l'arbre
  (en haut d'un groupe = avant lui, ailleurs = dedans) ; déposez des fichiers
  depuis l'Explorateur pour les ajouter.
- **Recherche dans le menu Démarrer** (utilisateur et tous les utilisateurs),
  selon les options `<search>` de PLaunch et ses couleurs ; clic droit sur un
  résultat → « Ajouter à PLaunch ».
- **Notes avec alarmes** (onglet *Notes*) : cartes colorées avec la palette de
  PLaunch, alarme `AAAA/MM/JJ HH:MM` unique, quotidienne ou hebdomadaire. Le
  rappel s'affiche au premier plan même fenêtre masquée ; les alarmes manquées
  pendant que l'ordinateur était éteint sont signalées au démarrage. Les notes
  sont stockées dans `plaunch-notes.xml`, à côté de `PLaunch.xml`.
- **Coin actif** : laisser la souris un instant dans un coin (ou contre un
  bord) de l'écran principal affiche la fenêtre.
- **Démarrer avec Windows** (réglages ou menu de l'icône) : valeur `PLaunch`
  dans `HKCU\…\Run`, distincte de celle de PLaunch. Chemin absolu : réactivez
  l'option si la clé USB change de lettre.
- **Fenêtre de réglages** (bouton ⚙).
- **Langue** : français ou anglais (automatique selon Windows par défaut),
  changement à chaud, menu de l'icône compris.
- **Thème** clair, sombre ou celui du système, et **taille du texte** réglable
  (70 à 200 %).
- **Icône personnalisable** : forme (lecture, étoile, grille, éclair) et
  couleur au choix, ou icône prise dans un fichier `.ico` / `.exe` (chemin
  relatif accepté pour rester portable). Appliquée à la fenêtre et à la zone
  de notification sans redémarrer.

### Nouveautés de la v0.9

- **Code réorganisé** : l'interface est découpée en modules (`app/loading`,
  `actions`, `views`, `input`, `dialogs`, `tree`, `stats`, `widgets`).
- **Retour à la version précédente** : après une mise à jour, l'ancien
  exécutable est gardé ; Réglages → Mises à jour → « ↩ Revenir à la version… »
  le remet en place et redémarre (et inversement).
- **Programmes en cours d'exécution** : un point vert les signale ; lancer un
  programme déjà ouvert ramène sa fenêtre au premier plan (clic droit →
  « Lancer une autre instance » pour en ouvrir une seconde). Réglable.
- **Dupliquer un raccourci** (clic droit → Dupliquer).
- **Sélection multiple** (`Ctrl+clic`, `Maj+clic`) : lancer, épingler,
  déplacer vers un groupe ou supprimer plusieurs éléments à la fois.
- **Temps d'utilisation** par programme, mesuré pendant que PLaunch tourne
  (onglet Statistiques et panneau de détails), enregistré dans
  `plaunch-usage.txt`.

### Nouveautés de la v0.8.1

- **Création d'un PLaunch.xml** : si aucun fichier n'est trouvé au lancement,
  l'écran d'accueil propose de créer un PLaunch.xml vide à côté de
  l'exécutable (ou au chemin passé en argument), de le créer ailleurs, ou
  d'ouvrir un fichier existant.

### Nouveautés de la v0.8

- **Éjection de la clé corrigée** (Windows 11) : PLaunch se ferme et un
  assistant lancé depuis le dossier temporaire demande à Windows de retirer
  le périphérique (comme « Retirer le périphérique en toute sécurité »). Un
  message confirme l'éjection ou indique ce qui la bloque (programme, fichier
  ouvert).
- **Favoris réorganisables** : glisser-déposer dans la barre, ou clic droit →
  « Déplacer à gauche / à droite ». L'ordre est gardé dans PLaunch.xml.
- **Emplacement choisi à la création** : le formulaire indique où le raccourci,
  le groupe ou le lien sera créé et permet de choisir un autre groupe ; après
  création, l'élément est affiché et sélectionné dans l'arbre.
- **Raccourcis clavier saisis au clavier** : cliquer sur le champ puis appuyer
  sur la combinaison (Échap annule, Suppr efface), pour un programme comme
  pour l'affichage de la fenêtre.
- **Alarmes avec calendrier** : choix de la date dans un calendrier du mois,
  heure et minutes réglables, raccourcis « +1 h » et « Demain 9:00 ».
- **Nouveaux programmes détectés** : la fenêtre d'ajout s'ouvre d'elle-même
  au démarrage.

### Nouveautés de la v0.7

- **Mises à jour vérifiées** : chaque version publie `plaunch.exe.sha256` ;
  la mise à jour intégrée refuse un fichier dont l'empreinte ne correspond pas.
- **Journal** `plaunch.log` à côté de l'exécutable (erreurs, lancements
  échoués, mises à jour, plantages avec pile d'appels). Bouton « Ouvrir le
  journal » dans les réglages.
- **Annuler / rétablir** (`Ctrl+Z` / `Ctrl+Y`, boutons ↶ ↷) pour toute
  modification : ajout, suppression, déplacement, nettoyage, import…
- **Rechargement automatique** de `PLaunch.xml` s'il est modifié par un autre
  programme (synchronisation, autre PC, PLaunch).
- **Exécutable allégé** (TLS de Windows, optimisation pour la taille) avec son
  **icône intégrée**, visible dans l'Explorateur.
- **Recherche intelligente** : fautes de frappe (« notpad »), lettres
  inversées, initiales (« vmp » → VLC media player), accents ignorés ; les
  programmes les plus lancés remontent.
- **Variables de chemin** : `%PLAUNCH%` (dossier de PLaunch.xml) et `%DRIVE%`
  (son lecteur), en plus des variables Windows ; bouton « Rendre les chemins
  portables » qui convertit en relatif les chemins absolus de la clé.
- **Lancer tout un groupe** (clic droit ou panneau de détails).
- **Raccourci clavier par programme** (ex. `Ctrl+Alt+V`), actif même fenêtre
  masquée.
- **Éjection de la clé** (menu de l'icône ou réglages) : liste des programmes
  ouverts depuis la clé, fermeture propre (ou forcée), puis éjection.
- **Détection de nouveaux programmes** dans un dossier (format
  PortableApps.com reconnu, avec nom et catégorie), signalée au démarrage si
  souhaité.
- **Import** d'un dossier de raccourcis `.lnk` / `.url` (un groupe par
  sous-dossier).
- **Séparateurs**, **liens web** (`https://…`) et **icônes de groupe**.

Nouvelles balises de `PLaunch.xml` : `runasadmin`, `favorite`, `globalhotkey`,
`<separator>` et l'attribut `icon` des groupes.

### Nouveautés de la v0.6

- **Options par raccourci** (formulaire de modification) : paramètres de la
  ligne de commande, description, fenêtre normale / réduite / agrandie,
  **exécuter en tant qu'administrateur** (demande UAC) et **lancer au
  démarrage** de PLaunch (balises `parameters`, `description`,
  `windowstate`, `autorun`, `runasadmin` et `favorite`).
- **Favoris** : épinglez un raccourci (clic droit ou panneau de détails). Ils
  apparaissent dans une barre sous la recherche, en tête du menu de l'icône de
  notification, et se lancent avec `Ctrl+1` à `Ctrl+9`.
- **Changement de lettre de lecteur** : si la clé passe de `F:` à `E:`,
  PLaunch propose de corriger les chemins absolus qui pointaient sur
  l'ancienne lettre, et met à jour « Démarrer avec Windows ». Correction
  manuelle possible depuis les réglages.
- **Navigation au clavier** : flèches haut/bas dans l'arbre et les résultats
  de recherche, droite/gauche pour ouvrir/fermer un groupe, `Entrée` lance.
- **Mode compact** (bouton ◧) : la fenêtre se réduit à la recherche, aux
  favoris et à la liste.
- **Rappels en notification Windows** (au choix : notification, boîte de
  message ou les deux).
- **Coin actif sur tous les écrans**.
- **Sauvegarde / restauration** : `PLaunch.xml`, notes et réglages dans un zip
  (les fichiers remplacés sont gardés en `.avant-restauration`).
- **Mise à jour intégrée** : vérification au démarrage, téléchargement de la
  dernière version publiée, remplacement de l'exécutable sur la clé et
  relance.
- **Statistiques** (onglet) : totaux, raccourcis les plus lancés, ancienneté
  du dernier lancement, raccourcis introuvables avec nettoyage en un clic.

## Réglages

Les réglages se modifient avec le bouton ⚙ ou directement dans
`plaunch.ini`, créé à côté de `PLaunch.xml` au premier lancement :

```ini
hotkey=Ctrl+Shift+Space   ; vide pour désactiver ; ex. Ctrl+Alt+P, Shift+Super+S
tray=true                 ; icône dans la zone de notification
close_to_tray=true        ; la croix masque au lieu de quitter
start_hidden=false        ; démarrer directement dans la zone de notification
hot_corner=none           ; topleft, topright, bottomleft, bottomright, top, bottom, left, right
hot_corner_delay=300      ; temps de présence dans la zone (ms)
hot_corner_size=8         ; taille des coins (pixels)
language=auto             ; auto, fr, en
theme=system              ; system, light, dark
text_size=100             ; pourcentage (70-200)
icon_shape=play           ; play, star, grid, bolt
icon_color=blue           ; blue, green, orange, purple, red, gray
icon_file=                ; ex. _Icones\plaunch.ico (prioritaire sur forme/couleur)
hot_corner_all_screens=true ; coin actif sur chaque écran
reminders=toast           ; toast, dialog, both
compact=false             ; mode compact
check_updates=true        ; vérifier les mises à jour au démarrage
last_drive=F              ; géré automatiquement (détection du changement de lettre)
scan_folder=PortableApps  ; dossier où chercher de nouveaux programmes
scan_on_start=false       ; les signaler au démarrage
scan_ignored=             ; programmes à ne plus signaler (séparés par |)
```

Depuis la fenêtre ⚙, tout s'applique immédiatement, sauf le raccourci global
et l'activation de l'icône de notification (au prochain démarrage). Après
une modification directe du fichier, relancez PLaunch.

## Utilisation

1. Téléchargez `plaunch.exe` depuis l'onglet
   [**Releases**](https://github.com/imtheyoyo/plaunch/releases/latest).
2. Placez-le à côté de votre `PLaunch.xml`, puis lancez-le.
   Vous pouvez aussi passer le fichier en argument
   (`plaunch.exe D:\PLaunch.xml`) ou le glisser sur la fenêtre.

Conseil : faites d'abord vos essais sur une copie de `PLaunch.xml`, et évitez
d'utiliser PLaunch et PLaunch en même temps sur le même fichier (chacun
écraserait les compteurs de l'autre).

## Publier une version

Augmentez `version` dans `Cargo.toml` et poussez sur `main` : la compilation
automatique crée le tag `vX.Y.Z` et la release avec `plaunch.exe`.

## Compiler

```powershell
winget install Rustlang.Rustup
cargo run --release
cargo test
```

Pour vérifier l'aller-retour sans perte sur votre propre fichier :

```powershell
$env:PLAUNCH_XML = "F:\_Appli Portables\PLaunch.xml"
cargo test -- --ignored
```

## Architecture

| Module | Rôle |
|---|---|
| `xml.rs` | Arbre XML générique, lecture via `quick-xml`, écriture au format PLaunch |
| `config.rs` | Chargement/enregistrement, accès aux entrées, statistiques, options |
| `edit.rs` | Ajout, modification, suppression et déplacement dans l'arborescence |
| `startmenu.rs` | Raccourcis du menu Démarrer pour la recherche |
| `paths.rs` | Résolution des chemins à la manière de PLaunch |
| `launcher.rs` | Lancement via `ShellExecuteW` (Windows) |
| `state.rs` | État partagé entre l'interface, l'icône de notification et le raccourci |
| `icons.rs` | Extraction des icônes (`SHGetFileInfoW`), icône de l'application |
| `tray.rs` | Icône de notification et menu en cascade (`tray-icon`) |
| `hotkey.rs` | Raccourci clavier global (`global-hotkey`) |
| `window.rs` | Affichage/masquage natif de la fenêtre |
| `instance.rs` | Instance unique (mutex nommé) |
| `settings.rs` | Lecture et écriture de `plaunch.ini` |
| `notes.rs` | Notes et alarmes (`plaunch-notes.xml`) |
| `alarms.rs` | Déclenchement des rappels |
| `hotcorner.rs` | Coin ou bord actif |
| `autostart.rs` | Démarrage avec Windows (registre) |
| `i18n.rs` | Traductions (le texte français sert de clé, anglais en table) |
| `backup.rs` | Sauvegarde et restauration (zip) |
| `log.rs` | Journal et plantages |
| `fuzzy.rs` | Recherche tolérante |
| `scan.rs` | Détection et import de programmes |
| `eject.rs` | Éjection de la clé |
| `update.rs` | Mise à jour depuis GitHub Releases |
| `app.rs`, `app/*.rs` | Interface egui (chargement, actions, vues, clavier, dialogues, arbre, statistiques) |
| `monitor.rs`, `usage.rs` | Programmes ouverts et temps d'utilisation |

## Journal des modifications

Voir [CHANGELOG.md](CHANGELOG.md) ; chaque release reprend la section de sa version.

## Feuille de route

- ~~**v0.2** : icônes des programmes, icône de notification avec menu en cascade,
  raccourci clavier global, instance unique.~~ ✔
- ~~**v0.3** : édition (ajout, renommage, déplacement, glisser-déposer),
  recherche dans le menu Démarrer.~~ ✔
- ~~**v0.4** : capteurs de bords d'écran, notes et alarmes, démarrage avec Windows.~~ ✔
- ~~**v0.5** : langue, thème, taille du texte et icône personnalisable.~~ ✔
- ~~**v0.6** : options des raccourcis (paramètres, administrateur…), favoris,
  lettre de lecteur, clavier et mode compact, notifications, multi-écran,
  sauvegarde, mise à jour intégrée, statistiques.~~ ✔
- ~~**v0.7** : mises à jour vérifiées, journal, annuler/rétablir, rechargement
  automatique, exécutable allégé, recherche floue, variables de chemin,
  groupes de lancement, raccourcis par programme, éjection, détection et
  import de programmes, séparateurs, liens web et icônes de groupe.~~ ✔
- ~~**v0.8** : éjection fiable, favoris réorganisables, emplacement à la
  création, saisie des raccourcis au clavier, calendrier des alarmes.~~ ✔
- ~~**v0.9** : code réorganisé, retour arrière, programmes ouverts,
  duplication, sélection multiple, temps d'utilisation.~~ ✔

## Licence

MIT
