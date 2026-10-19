"""Découpe CHANGELOG.md en un fichier de notes par version (notes/vX.Y.Z.md).

Utilisé par les workflows : synchronisation des notes de toutes les releases
et texte de la release publiée automatiquement.
"""
import pathlib
import re
import sys

FOOTER = (
    "\n\n---\nTéléchargez `pstart-rs.exe` ci-dessous et placez-le à côté de "
    "votre `PStart.xml`. Historique complet : "
    "[CHANGELOG.md](https://github.com/imtheyoyo/pstart-rs/blob/main/CHANGELOG.md)."
)


def main() -> None:
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "notes")
    out.mkdir(parents=True, exist_ok=True)
    text = pathlib.Path("CHANGELOG.md").read_text(encoding="utf-8")
    parts = re.split(r"^## (v[\d.]+)\s*$", text, flags=re.M)
    for tag, body in zip(parts[1::2], parts[2::2]):
        (out / f"{tag}.md").write_text(body.strip() + FOOTER + "\n", encoding="utf-8")
        print(tag)


if __name__ == "__main__":
    main()
