"""Construit assets/pstart-rs.ico à partir du rendu 256 px (voir icons.rs)."""
from pathlib import Path
from PIL import Image

here = Path(__file__).parent
raw = (here / "pstart-rs.256.rgba").read_bytes()
img = Image.frombytes("RGBA", (256, 256), raw)
img.save(here / "pstart-rs.ico", sizes=[(16, 16), (24, 24), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
print("ok", (here / "pstart-rs.ico").stat().st_size)
