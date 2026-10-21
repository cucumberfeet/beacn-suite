#!/usr/bin/env python3
"""Build the "official screen kit" for the beacn-utility fork from the user's OWN copy of the
official BEACN Windows app (BEACN.exe from BEACN Setup V1.4.2.12) and its MixImages pack.

Nothing produced here is redistributed: run it on the machine that owns the installer.

Usage: make-official-kit.py /path/to/BEACN.exe /path/to/MixImages/default.images OUT_DIR

Needs: python3-pillow, rsvg-convert (librsvg2-tools). Assets are located by content hash,
so a different BEACN version fails loudly instead of producing a wrong kit.
"""
import hashlib, io, json, os, re, struct, subprocess, sys, tempfile
from PIL import Image

# name -> (kind, sha256 of decoded RGBA pixels for raster / raw bytes for svg)
WANTED = {
    # Default theme Mix/Mix Create background texture (800x480 PNG)
    "background": ("png", None, (800, 480)),
    # Official preview render of the default-theme Mix Create screen (used for the header strip)
    "preview_create": ("jpg", None, (800, 480)),
    # Last full frame of the boot animation (BEACN logo on black)
    "splash": ("jpg", None, (799, 479)),
    # Default-theme screen component SVGs
    "card": ("svg", None, None),
    "band": ("svg", None, None),
    "button_on": ("svg", None, None),
    "button_off": ("svg", None, None),
    "dot_on": ("svg", None, None),
    "dot_off": ("svg", None, None),
    "dots_pill": ("svg", None, None),
    "mix_pill": ("svg", None, None),
    "knob_a": ("svg", None, None),
    "knob_b": ("svg", None, None),
}
HASHES = json.loads(open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "kit-hashes.json")).read())

def sha(b): return hashlib.sha256(b).hexdigest()

def carve(exe):
    d = open(exe, "rb").read()
    found = {}
    for m in re.finditer(re.escape(b"\x89PNG\r\n\x1a\n"), d):
        s = m.start(); e = d.find(b"IEND\xaeB`\x82", s)
        if e < 0: continue
        b = d[s:e + 8]
        try:
            im = Image.open(io.BytesIO(b)); im.load()
        except Exception: continue
        found.setdefault(("png", sha(im.convert("RGBA").tobytes())), im.convert("RGBA"))
    for m in re.finditer(re.escape(b"\xff\xd8\xff"), d):
        s = m.start()
        try:
            im = Image.open(io.BytesIO(d[s:s + 4_000_000])); im.load()
        except Exception: continue
        found.setdefault(("jpg", sha(im.convert("RGBA").tobytes())), im.convert("RGBA"))
    for m in re.finditer(rb"<svg", d):
        s = m.start(); e = d.find(b"</svg>", s)
        if e < 0 or e - s > 500000: continue
        b = d[s:e + 6]
        found.setdefault(("svg", sha(b)), b)
    return found

def svg_to_png(svg, w, h, out):
    with tempfile.NamedTemporaryFile(suffix=".svg", delete=False) as t:
        t.write(svg); p = t.name
    subprocess.run(["rsvg-convert", "-w", str(w), "-h", str(h), p, "-o", out], check=True)
    os.unlink(p)

SVG_SIZES = {  # pixel size on the 800x480 screen (viewBox rounded)
    "card": (187, 276), "band": (187, 36), "button_on": (105, 32), "button_off": (105, 32),
    "dot_on": (13, 13), "dot_off": (9, 9), "dots_pill": (77, 23), "mix_pill": (58, 32),
    "knob_a": (23, 23), "knob_b": (23, 23),
}

def load_pack(path):
    d = open(path, "rb").read(); i = 0; r = []
    while i < len(d):
        L = struct.unpack_from("<I", d, i)[0]; r.append(d[i + 4:i + 4 + L]); i += 4 + L
    return r

def make_zero_tile(pack, out):
    """There is no volume-0 tile in the official pack (it starts at 1). Build one from the
    v=1 tile by replacing its "1" with the "0" glyph taken from the "10" tile."""
    def tile(v, m): return Image.open(io.BytesIO(pack[v * (v - 1) // 2 + m])).convert("RGB")
    one, ten = tile(1, 0), tile(10, 0)
    W, H = one.size
    # glyph area: the digits sit inside the inner circle; find digit-coloured pixels (bright green)
    def bbox(im):
        px = im.load(); xs = []; ys = []
        for y in range(55, 125):
            for x in range(45, W - 45):
                r, g, b = px[x, y]
                if g > 150 and r < 120: xs.append(x); ys.append(y)
        return min(xs), min(ys), max(xs), max(ys)
    bx0, by0, bx1, by1 = bbox(one)
    tx0, ty0, tx1, ty1 = bbox(ten)
    # "10": the right half of the glyph run is the 0
    gap_split = tx0 + (tx1 - tx0) * 0.42
    px = ten.load(); cols = [x for x in range(int(gap_split), tx1 + 1) if any(px[x, y][1] > 150 and px[x, y][0] < 120 for y in range(ty0, ty1 + 1))]
    zx0, zx1 = min(cols), max(cols)
    zero = ten.crop((zx0 - 1, ty0 - 1, zx1 + 2, ty1 + 2))
    # wipe the "1" with the inner-circle background colour sampled next to it
    bg = one.getpixel((bx0 - 6, (by0 + by1) // 2))
    out_im = one.copy()
    out_im.paste(bg, (bx0 - 2, by0 - 2, bx1 + 3, by1 + 3))
    cx = (bx0 + bx1) // 2
    out_im.paste(zero, (cx - zero.size[0] // 2, ty0 - 1))
    out_im.save(out, quality=92)

def main():
    exe, images, out = sys.argv[1:4]
    os.makedirs(out, exist_ok=True)
    found = carve(exe)
    missing = []
    for name, (kind, _, _) in WANTED.items():
        key = (kind, HASHES[name])
        if key not in found:
            missing.append(name); continue
        obj = found[key]
        if kind == "svg":
            w, h = SVG_SIZES[name]; svg_to_png(obj, w, h, f"{out}/{name}.png")
        elif name == "preview_create":
            obj.crop((0, 0, 800, 61)).convert("RGB").save(f"{out}/header.png")
        elif name == "splash":
            s = Image.new("RGB", (800, 480), (0, 0, 0)); s.paste(obj.convert("RGB"), (0, 0)); s.save(f"{out}/splash.png")
        else:
            obj.convert("RGB").save(f"{out}/{name}.png")
    if missing:
        sys.exit(f"Could not find {missing} in {exe}; is this BEACN app 1.4.2.12?")
    make_zero_tile(load_pack(images), f"{out}/zero.jpg")
    json.dump({"kit_version": 1, "theme": "default", "source": os.path.basename(exe),
               "source_sha256": sha(open(exe, "rb").read())}, open(f"{out}/kit.json", "w"), indent=1)
    print("kit written to", out)

if __name__ == "__main__":
    main()
