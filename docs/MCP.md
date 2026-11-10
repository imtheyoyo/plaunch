# Pilotage de PLaunch par une IA (MCP)

`plaunch-mcp.exe` est une variante de PLaunch qui embarque un serveur
[MCP](https://modelcontextprotocol.io) : une IA (Claude Code, Claude Desktop…)
peut voir la fenêtre, lire la liste des widgets, cliquer, taper au clavier,
lire le journal et appeler des commandes de PLaunch. C'est un outil de
**test et de débogage** : l'IA reproduit un problème, vérifie un correctif et
recommence, sans intervention manuelle.

`plaunch.exe` reste inchangé : le serveur n'y est pas compilé.

## Démarrage

1. Fermer PLaunch s'il est ouvert (une seule instance à la fois).
2. Lancer `plaunch-mcp.exe --mcp` (à côté de `PLaunch.xml`, comme d'habitude).

Le serveur écoute sur `http://127.0.0.1:47800/mcp`. Options :

| Option | Effet |
|---|---|
| `--mcp` | active le serveur sur le port 47800 |
| `--mcp=PORT` | autre port |
| `--mcp-token=JETON` | exige `Authorization: Bearer JETON` (ou variable `MCP_TOKEN`) |
| `--mcp-stdio` | passerelle stdio (voir Claude Desktop) |

Sans `--mcp`, `plaunch-mcp.exe` se comporte exactement comme `plaunch.exe`.
En mode MCP, la fenêtre ne démarre pas masquée (les captures en ont besoin).

## Brancher un client

### Claude Code

```powershell
claude mcp add --transport http plaunch http://127.0.0.1:47800/mcp
# avec un jeton :
claude mcp add --transport http plaunch http://127.0.0.1:47800/mcp --header "Authorization: Bearer JETON"
```

### Claude Desktop

Dans `claude_desktop_config.json` (Réglages › Développeur › Modifier la config) :

```json
{
  "mcpServers": {
    "plaunch": {
      "command": "F:\\_Appli Portables\\plaunch-mcp.exe",
      "args": ["--mcp-stdio"]
    }
  }
}
```

La passerelle relaie les messages vers l'instance en cours et lance
`plaunch-mcp.exe --mcp` si elle n'est pas ouverte.

## Outils

Génériques (crate `egui-mcp`, communs avec ComicRust) :

| Outil | Rôle |
|---|---|
| `screenshot` | capture PNG ; 1 pixel = 1 point, donc coordonnées utilisables pour `click` |
| `ui_tree` | widgets visibles : rôle, libellé, valeur, position, état (coché, désactivé…) |
| `click_widget` | clic sur un widget désigné par son libellé (ou son id) |
| `wait_for` | attend qu'un widget apparaisse ou disparaisse |
| `click`, `hover`, `drag`, `scroll` | souris, en points |
| `type_text`, `press_key` | clavier (« Enter », « Ctrl+Z », « F2 »…) |
| `wait` | pause |
| `get_logs` | fin de `plaunch.log` |

Propres à PLaunch :

| Outil | Rôle |
|---|---|
| `get_state` | fichier, vue, recherche, sélection, dialogue ouvert, statut, erreurs |
| `list_items` | arborescence avec chemins (« 0/2/1 »), cibles et existence |
| `select_item`, `launch_item`, `edit_item`, `toggle_group` | sur un élément (chemin ou « Groupe › Nom ») |
| `set_search`, `set_view` | recherche, vue (arborescence, plus utilisés, récents, notes, statistiques) |
| `new_entry`, `new_group`, `open_settings`, `close_dialog` | dialogues |
| `undo`, `redo`, `reload`, `toggle_compact`, `show_window` | commandes |

Aucune suppression n'est exposée directement (l'IA peut toutefois cliquer sur
n'importe quel bouton : tester sur une copie de `PLaunch.xml`, par exemple avec
un profil dédié).

## Sécurité

- Écoute sur `127.0.0.1` uniquement : rien n'est accessible depuis le réseau.
- Requêtes venant d'une page web refusées (en-tête `Origin` étranger).
- Jeton facultatif pour empêcher les autres programmes de la machine de piloter
  PLaunch.
- À n'utiliser que pendant les tests.

## Méthode de test conseillée

1. `get_state` et `screenshot` pour l'état de départ.
2. `ui_tree` (éventuellement filtré) pour repérer les widgets.
3. Actions : `click_widget`, `type_text`, `press_key`…
4. Vérification : `screenshot`, `get_state`, `get_logs` (erreurs, plantages).

## Fonctionnement

Le crate `crates/egui-mcp` fait le travail générique : serveur HTTP local
(transport MCP « Streamable HTTP », réponses JSON), injection des événements
dans `eframe::App::raw_input_hook` image par image, arbre des widgets lu dans
AccessKit, captures via `egui::Context::request_screenshot`. Les outils de
PLaunch sont dans `src/app/mcp.rs` et réutilisent les actions de l'interface
(`Action`), comme un clic de l'utilisateur.
