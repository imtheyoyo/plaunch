# PStart-rs

Lanceur portable pour clé USB, écrit en Rust avec [egui](https://github.com/emilk/egui).
Il relit et met à jour les fichiers **`PStart.xml`** du lanceur PStart (Pegtop) :
vous gardez votre arborescence de raccourcis telle quelle.

> Projet indépendant, sans lien avec Pegtop Software. Il ne contient aucun code de
> PStart : seul le format de fichier `PStart.xml` est pris en charge, à des fins
> d'interopérabilité.

## Fonctions (v0.1)

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

## Utilisation

1. Téléchargez `pstart-rs.exe` depuis l'onglet **Actions** (artefact
   `pstart-rs-windows-x64`) ou **Releases**.
2. Placez-le à côté de votre `PStart.xml`, puis lancez-le.
   Vous pouvez aussi passer le fichier en argument
   (`pstart-rs.exe D:\PStart.xml`) ou le glisser sur la fenêtre.

Conseil : faites d'abord vos essais sur une copie de `PStart.xml`, et évitez
d'utiliser PStart et PStart-rs en même temps sur le même fichier (chacun
écraserait les compteurs de l'autre).

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
| `app.rs` | Interface egui |

## Feuille de route

- **v0.2** : icônes des programmes, icône de notification avec menu en cascade,
  raccourci clavier global, instance unique.
- **v0.3** : édition (ajout, renommage, déplacement, glisser-déposer),
  recherche dans le menu Démarrer.
- **v0.4** : capteurs de bords d'écran, notes et alarmes, démarrage avec Windows.

## Licence

MIT
