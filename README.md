# PStart-rs

Lanceur portable pour clé USB, écrit en Rust avec [egui](https://github.com/emilk/egui).
Il relit et met à jour les fichiers **`PStart.xml`** du lanceur PStart (Pegtop) :
vous gardez votre arborescence de raccourcis telle quelle.

> Projet indépendant, sans lien avec Pegtop Software. Il ne contient aucun code de
> PStart : seul le format de fichier `PStart.xml` est pris en charge, à des fins
> d'interopérabilité.

## Fonctions

- Arborescence des groupes et raccourcis, avec le nombre d'entrées par groupe.
- Recherche instantanée sur le nom et le chemin (`Ctrl+F`, `Entrée` lance le
  premier résultat, `Échap` efface).
- Vues **Plus utilisés** et **Récents**, à partir des compteurs de PStart.
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
  entités `&#xE9;`). Une copie `PStart.xml.bak` est faite au premier
  enregistrement de chaque session.
- **Icônes** des programmes, extraites en tâche de fond.
- **Icône de notification** : clic gauche pour afficher/masquer, clic droit
  pour un menu en cascade reprenant tous vos groupes et raccourcis (avec leurs
  icônes). Le bouton de fermeture masque la fenêtre au lieu de quitter ;
  `Échap` (recherche vide) la masque aussi.
- **Raccourci clavier global** pour afficher/masquer la fenêtre
  (`Ctrl+Shift+Space` par défaut).
- **Instance unique** : relancer `pstart-rs.exe` réaffiche la fenêtre ouverte.

## Réglages

Au premier lancement, un fichier `pstart-rs.ini` est créé à côté de
`PStart.xml` (PStart-rs n'ajoute rien dans le fichier de PStart) :

```ini
hotkey=Ctrl+Shift+Space   ; vide pour désactiver ; ex. Ctrl+Alt+P, Shift+Super+S
tray=true                 ; icône dans la zone de notification
close_to_tray=true        ; la croix masque au lieu de quitter
start_hidden=false        ; démarrer directement dans la zone de notification
```

Relancez PStart-rs (menu de l'icône → « Quitter PStart-rs ») après modification.

## Utilisation

1. Téléchargez `pstart-rs.exe` depuis l'onglet
   [**Releases**](https://github.com/imtheyoyo/pstart-rs/releases/latest).
2. Placez-le à côté de votre `PStart.xml`, puis lancez-le.
   Vous pouvez aussi passer le fichier en argument
   (`pstart-rs.exe D:\PStart.xml`) ou le glisser sur la fenêtre.

Conseil : faites d'abord vos essais sur une copie de `PStart.xml`, et évitez
d'utiliser PStart et PStart-rs en même temps sur le même fichier (chacun
écraserait les compteurs de l'autre).

## Publier une version

Augmentez `version` dans `Cargo.toml` et poussez sur `main` : la compilation
automatique crée le tag `vX.Y.Z` et la release avec `pstart-rs.exe`.

## Compiler

```powershell
winget install Rustlang.Rustup
cargo run --release
cargo test
```

Pour vérifier l'aller-retour sans perte sur votre propre fichier :

```powershell
$env:PSTART_XML = "F:\_Appli Portables\PStart.xml"
cargo test -- --ignored
```

## Architecture

| Module | Rôle |
|---|---|
| `xml.rs` | Arbre XML générique, lecture via `quick-xml`, écriture au format PStart |
| `config.rs` | Chargement/enregistrement, accès aux entrées, statistiques, options |
| `paths.rs` | Résolution des chemins à la manière de PStart |
| `launcher.rs` | Lancement via `ShellExecuteW` (Windows) |
| `state.rs` | État partagé entre l'interface, l'icône de notification et le raccourci |
| `icons.rs` | Extraction des icônes (`SHGetFileInfoW`), icône de l'application |
| `tray.rs` | Icône de notification et menu en cascade (`tray-icon`) |
| `hotkey.rs` | Raccourci clavier global (`global-hotkey`) |
| `window.rs` | Affichage/masquage natif de la fenêtre |
| `instance.rs` | Instance unique (mutex nommé) |
| `settings.rs` | Lecture de `pstart-rs.ini` |
| `app.rs` | Interface egui |

## Feuille de route

- ~~**v0.2** : icônes des programmes, icône de notification avec menu en cascade,
  raccourci clavier global, instance unique.~~ ✔
- **v0.3** : édition (ajout, renommage, déplacement, glisser-déposer),
  recherche dans le menu Démarrer.
- **v0.4** : capteurs de bords d'écran, notes et alarmes, démarrage avec Windows.

## Licence

MIT
