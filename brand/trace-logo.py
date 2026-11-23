"""Trace brand/erebor-logo-source.png into the themed SVGs (needs potrace, numpy, scipy, Pillow).
Run from brand/: python3 trace-logo.py ."""
import os, re, subprocess, sys, tempfile
from PIL import Image
import numpy as np
from scipy import ndimage as nd

im = np.asarray(Image.open("erebor-logo-source.png").convert("RGB")).astype(int)
r, g, b = im[..., 0], im[..., 1], im[..., 2]
lum = 0.299 * r + 0.587 * g + 0.114 * b
inside = np.zeros_like(lum, bool); inside[60:-60, 60:-60] = True
lab, n = nd.label((lum > 45) & inside)
Y, X = np.mgrid[0:1254, 0:1254]
MOUNTAIN, ROCK, PEAK_X = lab[300, 560], lab[760, 950], 629

cream = np.zeros_like(lum, bool); gold = np.zeros_like(lum, bool)
for i in range(1, n + 1):
    m = lab == i
    if m.sum() < 150 or i in (MOUNTAIN, ROCK):
        continue
    (gold if ((r - b)[m] > 70).mean() > 0.5 else cream)[m] = True
masks = {
    "cream": cream,
    "gold": gold,
    "facet": gold & (lum < 125),
    "mleft": (lab == MOUNTAIN) & (X < PEAK_X),
    "mright": (lab == MOUNTAIN) & (X >= PEAK_X),
    "rock": lab == ROCK,
}

TMP = tempfile.mkdtemp()

def trace(name, m):
    m = nd.binary_opening(m, iterations=1) if name == "facet" else m
    pbm, out = os.path.join(TMP, f"{name}.pbm"), os.path.join(TMP, f"{name}.svg")
    Image.fromarray(np.where(m, 0, 255).astype("uint8")).convert("1").save(pbm)
    subprocess.run(["potrace", pbm, "-s", "--flat", "-t", "12", "-a", "1.0", "-O", "0.6", "-o", out], check=True)
    return " ".join(re.findall(r'<path d="([^"]+)"', open(out).read())).replace("\n", " ")

d = {k: trace(k, v) for k, v in masks.items()}

def svg(ink, gold, deep, vb="88 152 1078 908", bg=None):
    g0, g1, g2 = gold
    # Gradient coordinates live in potrace space (x10, y flipped); y1/y2 are image pixels.
    grad = lambda id, y1, y2, stops: (
        f'<linearGradient id="{id}" gradientUnits="userSpaceOnUse" x1="0" y1="{(1254 - y1) * 10}" x2="0" y2="{(1254 - y2) * 10}">'
        + "".join(f'<stop offset="{o}" stop-color="{c}"/>' for o, c in stops) + "</linearGradient>")
    defs = (grad("eg", 170, 1050, [(0, g0), (0.55, g1), (1, g2)])
            + grad("el", 330, 720, [(0, ink), (0.3, ink), (0.75, g1), (1, g2)])
            + grad("er", 170, 740, [(0, g0), (0.6, g1), (1, g2)])
            + grad("ek", 735, 870, [(0, ink), (0.35, ink), (1, g1)]))
    bgel = f'<rect width="1254" height="1254" rx="270" fill="{bg}"/>' if bg else ""
    p = lambda k, fill: f'<path fill="{fill}" d="{d[k]}"/>'
    return (f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="{vb}" role="img" aria-label="Erebor">'
            f"<defs>{defs}</defs>{bgel}"
            '<g transform="translate(0 1254) scale(.1 -.1)">'
            + p("mleft", "url(#el)") + p("mright", "url(#er)") + p("rock", "url(#ek)")
            + p("cream", ink) + p("gold", "url(#eg)") + p("facet", deep)
            + "</g></svg>\n")

DARK = dict(ink="#F4E8D2", gold=("#FBD46E", "#E6A93C", "#C08A2A"), deep="#8E5C12")
LIGHT = dict(ink="#161616", gold=("#F0BE4E", "#D99E2C", "#AE7A16"), deep="#7A4E0C")
out = sys.argv[1] if len(sys.argv) > 1 else "."
open(f"{out}/erebor-mark-dark.svg", "w").write(svg(**DARK))
open(f"{out}/erebor-mark-light.svg", "w").write(svg(**LIGHT))
open(f"{out}/erebor-app-icon.svg", "w").write(svg(**DARK, vb="0 0 1254 1254", bg="#0B0E19"))
