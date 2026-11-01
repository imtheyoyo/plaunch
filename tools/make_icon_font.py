"""Construit assets/fonts/plaunch-icons.ttf : sous-ensemble des icônes
Phosphor (licence MIT) utilisées par l'interface.

    npm pack @phosphor-icons/web@2.1.1 && tar xzf phosphor-icons-web-2.1.1.tgz
    python3 tools/make_icon_font.py package/src

Les icônes « remplies » (fill) sont recopiées sur des codes U+E9xx pour
cohabiter avec les icônes « regular » dans une seule police.
Chaque nom doit aussi figurer dans src/app/icon.rs.
"""
import re
import sys
from fontTools.ttLib import TTFont
from fontTools.merge import Merger
from fontTools import subset

REGULAR = """plus play star x folder folder-open alarm calendar-blank arrow-circle-up
list gear-six arrows-in-simple arrows-out-simple arrow-clockwise arrow-arc-right
arrow-arc-left warning caret-down caret-left caret-right caret-up arrow-u-up-left
keyboard circle magnifying-glass note-pencil trash pencil-simple copy
check sun moon monitor eject power info question rocket-launch
squares-four rows clock-counter-clockwise chart-bar user-circle arrow-right""".split()
# nom -> code de destination
FILL = {"star": 0xE900, "circle": 0xE901, "play": 0xE902, "caret-left": 0xE903, "caret-right": 0xE904}
# Symboles Unicode déjà présents dans les textes de l'interface : la police
# les dessine aussi, ce qui évite les carrés quand Windows ne les a pas.
UNICODE = {
    0xFF0B: "plus", 0x25B6: "caret-right!", 0x25C0: "caret-left!", 0x2605: "star!",
    0x2606: "star", 0x2716: "x", 0x1F4C1: "folder", 0x1F4C2: "folder-open",
    0x23F0: "alarm", 0x1F4C5: "calendar-blank", 0x2B06: "arrow-circle-up",
    0x2630: "list", 0x2699: "gear-six", 0x25E7: "arrows-in-simple",
    0x27F3: "arrow-clockwise", 0x21B7: "arrow-arc-right", 0x21B6: "arrow-arc-left",
    0x26A0: "warning", 0x25BE: "caret-down", 0x21A9: "arrow-u-up-left",
    0x2328: "keyboard", 0x25CF: "circle!", 0x2192: "arrow-right",
}


def codes(css, prefix, names):
    out = {}
    for n in names:
        m = re.search(r"\." + prefix + r"\.ph-" + re.escape(n) + r':before\s*\{\s*content:\s*"\\([0-9a-f]+)"', css)
        if not m:
            sys.exit(f"icône inconnue : {n}")
        out[n] = int(m.group(1), 16)
    return out


def sub(path, unicodes, out):
    opts = subset.Options()
    opts.layout_features = []
    opts.hinting = False
    opts.desubroutinize = True
    opts.name_IDs = ["*"]
    font = TTFont(path)
    s = subset.Subsetter(opts)
    s.populate(unicodes=unicodes)
    s.subset(font)
    font.save(out)


def main(src):
    reg = codes(open(f"{src}/regular/style.css").read(), r"ph", REGULAR)
    fil = codes(open(f"{src}/fill/style.css").read(), r"ph-fill", list(FILL))
    sub(f"{src}/regular/Phosphor.ttf", list(reg.values()), "/tmp/ph-reg.ttf")
    sub(f"{src}/fill/Phosphor-Fill.ttf", list(fil.values()), "/tmp/ph-fill.ttf")
    # Recodage des icônes remplies.
    f = TTFont("/tmp/ph-fill.ttf")
    for table in f["cmap"].tables:
        new = {}
        for name, code in fil.items():
            if code in table.cmap:
                new[FILL[name]] = table.cmap[code]
        table.cmap = new
    f.save("/tmp/ph-fill.ttf")
    merged = Merger().merge(["/tmp/ph-reg.ttf", "/tmp/ph-fill.ttf"])
    # Codes Unicode supplémentaires (« ! » : version remplie).
    by_code = merged.getBestCmap()
    for table in merged["cmap"].tables:
        if not table.isUnicode():
            continue
        for uni, name in UNICODE.items():
            if uni > 0xFFFF and table.format == 4:
                continue
            code = FILL[name[:-1]] if name.endswith("!") else reg[name]
            table.cmap[uni] = by_code[code]
    if not any(t.format == 12 for t in merged["cmap"].tables):
        from fontTools.ttLib.tables._c_m_a_p import CmapSubtable
        t = CmapSubtable.newSubtable(12)
        t.platformID, t.platEncID, t.language = 3, 10, 0
        t.cmap = dict(by_code)
        for uni, name in UNICODE.items():
            code = FILL[name[:-1]] if name.endswith("!") else reg[name]
            t.cmap[uni] = by_code[code]
        merged["cmap"].tables.append(t)
    merged.save("assets/fonts/plaunch-icons.ttf")
    for n, c in sorted(reg.items()):
        print(f"{n:28} U+{c:04X}")
    for n, c in FILL.items():
        print(f"{n + ' (fill)':28} U+{c:04X}")


main(sys.argv[1])
