#!/usr/bin/env python3
"""Render the boot animation frames for the Plymouth theme.

The emblem is illogical-impulse's logo (from end-4/dots-hyprland, GPL-3.0):
a dark disc, an open ring with an arrowhead, an asterisk and three dots.

Usage: render.py OUT_DIR [--colors colors.json] [--size 512] [--fps 50]

Writes intro-NNNN.png (played once), loop-NNNN.png (repeats while booting)
and palette.txt (background/text colors for the Plymouth script). Colors
come from the shell's style.json (Material 3 Settings' shell integration,
colors under "colors") or a matugen colors.json when given, else the logo's
own teal. Rasterizes with rsvg-convert, cairosvg or ImageMagick, whichever is
installed. Standard library only otherwise.
"""
import argparse
import json
import math
import os
import shutil
import subprocess
import sys
import tempfile

RING = "m 909.5,1535 c 0,22.09 17.925,40.12 39.977,38.8 128.603,-7.7 252.553,-52.43 356.713,-129.22 114.91,-84.72 199.68,-203.99 241.9,-340.37 42.23,-136.376 39.68,-282.688 -7.26,-417.514 C 1493.89,551.871 1405.02,435.614 1287.23,354.947 1169.44,274.279 1028.92,233.439 886.247,238.405 743.57,243.37 606.23,293.882 494.335,382.543 382.441,471.205 301.868,593.359 264.415,731.122 c -33.947,124.867 -30.854,256.612 8.447,379.298 6.739,21.04 30.033,31.24 50.63,23.25 l 161.188,-62.48 c 20.597,-7.99 30.621,-31.13 24.652,-52.4 -20.233,-72.095 -20.675,-148.588 -0.899,-221.327 23.285,-85.651 73.38,-161.598 142.948,-216.721 69.568,-55.124 154.956,-86.528 243.662,-89.615 88.706,-3.088 176.067,22.304 249.307,72.457 73.23,50.153 128.48,122.433 157.67,206.258 29.18,83.825 30.76,174.791 4.51,259.578 -26.25,84.79 -78.95,158.95 -150.4,211.62 -60.67,44.73 -132.15,71.97 -206.69,79.16 -21.99,2.12 -39.94,19.83 -39.94,41.92 z"
ARROW = "m 1340.4,762.533 -97.84,-176.165 c -3.36,-6.054 1.09,-13.478 8.01,-13.369 l 191.22,3.026 c 6.75,0.106 10.98,7.33 7.78,13.271 l -93.38,173.139 c -3.38,6.26 -12.34,6.315 -15.79,0.098 z"
STAR = "m 875.094,1178 c -5.523,0 -10,-4.48 -10,-10 V 996.072 c 0,-3.084 -3.343,-5.008 -6.009,-3.458 l -148.816,86.456 c -4.761,2.76 -10.862,1.16 -13.648,-3.59 l -34.539,-58.86 c -2.803,-4.78 -1.188,-10.93 3.601,-13.71 l 149.428,-86.824 c 2.655,-1.542 2.654,-5.376 -10e-4,-6.918 L 666.155,822.702 c -4.778,-2.773 -6.401,-8.896 -3.626,-13.673 l 34.107,-58.69 c 2.774,-4.774 8.894,-6.396 13.669,-3.622 l 148.78,86.433 c 2.666,1.549 6.009,-0.375 6.009,-3.459 V 657 c 0,-5.523 4.477,-10 10,-10 h 67.804 c 5.523,0 10,4.477 10,10 v 172.691 c 0,3.084 3.343,5.008 6.01,3.459 l 148.782,-86.433 c 4.77,-2.774 10.89,-1.152 13.67,3.622 l 34.1,58.684 c 2.78,4.779 1.15,10.905 -3.63,13.677 l -149.19,86.467 c -2.655,1.541 -2.656,5.379 0,6.921 l 149.67,86.822 c 4.79,2.78 6.41,8.93 3.6,13.71 l -34.54,58.86 c -2.78,4.75 -8.89,6.35 -13.65,3.59 L 958.908,992.614 c -2.667,-1.55 -6.01,0.374 -6.01,3.458 V 1168 c 0,5.52 -4.477,10 -10,10 z"

# Emblem geometry in the logo's own coordinates.
CX, CY, R = 909.5, 906.5, 817.5
DOTS = [  # (cx, cy, r, rotation) as in the original SVG, biggest last
    (686.4613, -1244.3154, 55.947601, 131.714),
    (-328.14859, -1291.9602, 91.282997, 167.0),
    (-387.40717, -1575.8054, 111.0, 167.0),
]
# The ring runs from 6 o'clock clockwise on screen (angles decreasing in
# SVG terms) for about 300 degrees.
RING_START, RING_SWEEP = 90.0, -300.0

DEFAULT = {
    "background": "#101414",
    "disc": "#1e202b",
    "ring_a": "#38DFC0",
    "ring_b": "#66D9B3",
    "star_a": "#38C3A7",
    "star_b": "#3DE6AC",
    "dots": "#51debd",
    "text": "#dde4e1",
}


def palette(path):
    """Map the shell's Material colors onto the emblem's parts."""
    if not path:
        return dict(DEFAULT)
    with open(path, encoding="utf-8") as f:
        c = json.load(f)
    # style.json nests the roles under "colors"; colors.json is flat.
    if isinstance(c.get("colors"), dict):
        c = c["colors"]
    get = lambda k, d: c.get(k) or DEFAULT[d]
    return {
        "background": get("background", "background"),
        "disc": get("surface_container_high", "disc"),
        "ring_a": get("primary", "ring_a"),
        "ring_b": get("tertiary", "ring_b"),
        "star_a": get("primary", "star_a"),
        "star_b": get("secondary", "star_b"),
        "dots": get("primary", "dots"),
        "text": get("on_background", "text"),
    }


def ease_out_cubic(t):
    return 1 - (1 - t) ** 3


def ease_out_back(t, s=1.7):
    t -= 1
    return 1 + t * t * ((s + 1) * t + s)


def clamp01(x):
    return max(0.0, min(1.0, x))


def phase(t, start, end):
    return clamp01((t - start) / (end - start))


def wedge(start_deg, sweep_deg):
    """Pie slice polygon from the emblem center, for revealing the ring."""
    if abs(sweep_deg) < 0.01:
        return f"M {CX} {CY} Z"
    steps = max(2, int(abs(sweep_deg) / 3))
    rr = R * 1.2
    pts = [f"{CX},{CY}"]
    for i in range(steps + 1):
        a = math.radians(start_deg + sweep_deg * i / steps)
        pts.append(f"{CX + rr * math.cos(a):.1f},{CY + rr * math.sin(a):.1f}")
    return "M " + " L ".join(pts) + " Z"


def frame_svg(pal, *, disc, ring, ring_rot, star, star_rot, dots, size):
    """One frame. Arguments are animation progress values in 0..1 (dots is
    a list of per-dot scales) and rotations in degrees."""
    pad = 110
    vb = f"{CX - R - pad} {CY - R - pad} {2 * (R + pad)} {2 * (R + pad)}"
    disc_s = 0.6 + 0.4 * ease_out_cubic(disc)
    star_s = 0.2 + 0.8 * ease_out_back(star) if star > 0 else 0
    parts = [
        f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="{vb}">',
        "<defs>",
        f'<linearGradient id="ring" x1="420" y1="1489" x2="1514.5" y2="329.5" gradientUnits="userSpaceOnUse">'
        f'<stop stop-color="{pal["ring_a"]}"/><stop offset="1" stop-color="{pal["ring_b"]}"/></linearGradient>',
        f'<linearGradient id="star" x1="726.231" y1="1178" x2="1149.24" y2="615.17" gradientUnits="userSpaceOnUse">'
        f'<stop stop-color="{pal["star_a"]}"/><stop offset="1" stop-color="{pal["star_b"]}"/></linearGradient>',
        f'<clipPath id="reveal"><path d="{wedge(RING_START, RING_SWEEP * 1.08 * ease_out_cubic(ring))}"/></clipPath>',
        "</defs>",
        f'<g opacity="{ease_out_cubic(disc):.3f}" transform="translate({CX} {CY}) scale({disc_s:.4f}) translate({-CX} {-CY})">',
        f'<circle cx="{CX}" cy="{CY}" r="{R}" fill="{pal["disc"]}"/>',
        "</g>",
    ]
    if ring > 0:
        parts += [
            f'<g transform="rotate({ring_rot:.2f} {CX} {CY})" clip-path="url(#reveal)">',
            f'<path d="{RING}" fill="url(#ring)"/>',
            f'<path d="{ARROW}" fill="{pal["disc"]}" stroke="{pal["disc"]}" stroke-width="25"/>',
            "</g>",
        ]
    if star > 0:
        parts.append(
            f'<g opacity="{min(1, star * 2):.3f}" transform="translate({CX} {CY}) rotate({star_rot:.2f}) scale({star_s:.4f}) translate({-CX} {-CY})">'
            f'<path d="{STAR}" fill="url(#star)"/></g>'
        )
    # The dots sit in the ring's gap, so they turn with the ring.
    parts.append(f'<g transform="rotate({ring_rot:.2f} {CX} {CY})">')
    for (dx, dy, dr, rot), s in zip(DOTS, dots):
        if s <= 0:
            continue
        parts.append(
            f'<g transform="rotate({rot})"><circle cx="{dx}" cy="{dy}" r="{dr * s:.2f}" fill="{pal["dots"]}"/></g>'
        )
    parts.append("</g>")
    parts.append("</svg>")
    return "".join(parts)


INTRO_SECONDS = 2.0
LOOP_SECONDS = 2.0


def intro_frame(t, pal, size):
    """t in seconds from 0 to INTRO_SECONDS."""
    disc = phase(t, 0.0, 0.5)
    ring = phase(t, 0.25, 1.15)
    star = phase(t, 0.85, 1.4)
    star_rot = -150 * (1 - ease_out_cubic(star))
    dots = []
    for i in range(3):  # small, medium, big
        p = phase(t, 1.2 + 0.15 * i, 1.55 + 0.15 * i)
        dots.append(ease_out_back(p, 2.4) if p > 0 else 0)
    return frame_svg(pal, disc=disc, ring=ring, ring_rot=0, star=star,
                     star_rot=star_rot, dots=dots, size=size)


def loop_frame(t, pal, size):
    """t in seconds from 0 to LOOP_SECONDS; seamless when it wraps."""
    u = t / LOOP_SECONDS
    ring_rot = 360 * u  # ring, arrowhead and dots make one full turn per loop
    dots = []
    for i in range(3):  # a wave running through the dots
        w = math.sin(2 * math.pi * (u - 0.12 * i))
        dots.append(1 + 0.22 * max(0.0, w))
    return frame_svg(pal, disc=1, ring=1, ring_rot=ring_rot, star=1,
                     star_rot=0, dots=dots, size=size)


def entry_svg(pal, width=440, height=60):
    """The pill-shaped password field, Material 3 style."""
    r = height / 2
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}">'
            f'<rect width="{width}" height="{height}" rx="{r}" fill="{pal["disc"]}"/></svg>')


def bullet_svg(pal, size=14):
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}">'
            f'<circle cx="{size / 2}" cy="{size / 2}" r="{size / 2}" fill="{pal["text"]}"/></svg>')


def lock_svg(pal, size=24):
    """Material Symbols "lock" (Apache-2.0), filled."""
    return (f'<svg xmlns="http://www.w3.org/2000/svg" width="{size}" height="{size}" viewBox="0 -960 960 960">'
            f'<path fill="{pal["dots"]}" d="M240-80q-33 0-56.5-23.5T160-160v-400q0-33 23.5-56.5T240-640h40v-80q0-83 '
            f'58.5-141.5T480-920q83 0 141.5 58.5T680-720v80h40q33 0 56.5 23.5T800-560v400q0 33-23.5 56.5T720-80H240Zm240-200q33 '
            f'0 56.5-23.5T560-360q0-33-23.5-56.5T480-440q-33 0-56.5 23.5T400-360q0 33 23.5 56.5T480-280ZM360-640h240v-80q0-50'
            f'-35-85t-85-35q-50 0-85 35t-35 85v80Z"/></svg>')


def rasterizer():
    if shutil.which("rsvg-convert"):
        return lambda svg, png: subprocess.run(["rsvg-convert", "-o", png, svg], check=True)
    try:
        import cairosvg  # noqa: F401
        return lambda svg, png: cairosvg.svg2png(url=svg, write_to=png)
    except ImportError:
        pass
    for magick in ("magick", "convert"):
        if shutil.which(magick):
            return lambda svg, png, m=magick: subprocess.run(
                [m, "-background", "none", svg, png], check=True)
    sys.exit("Need rsvg-convert (librsvg), python-cairosvg or ImageMagick to render frames.")


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("out")
    ap.add_argument("--colors")
    ap.add_argument("--size", type=int, default=512)
    ap.add_argument("--fps", type=int, default=50)
    args = ap.parse_args()

    pal = palette(args.colors)
    render = rasterizer()
    os.makedirs(args.out, exist_ok=True)
    for name in os.listdir(args.out):
        if name.endswith(".png") and name.startswith(("intro-", "loop-")):
            os.remove(os.path.join(args.out, name))

    with tempfile.TemporaryDirectory() as tmp:
        def emit(prefix, n, make):
            for i in range(n):
                svg = os.path.join(tmp, "f.svg")
                with open(svg, "w", encoding="utf-8") as f:
                    f.write(make(i / args.fps, pal, args.size))
                render(svg, os.path.join(args.out, f"{prefix}-{i:04d}.png"))
        intro = int(INTRO_SECONDS * args.fps)
        loop = int(LOOP_SECONDS * args.fps)
        emit("intro", intro, intro_frame)
        emit("loop", loop, loop_frame)
        for name, svg_text in (("entry", entry_svg(pal)), ("bullet", bullet_svg(pal)), ("lock", lock_svg(pal))):
            svg = os.path.join(tmp, f"{name}.svg")
            with open(svg, "w", encoding="utf-8") as f:
                f.write(svg_text)
            render(svg, os.path.join(args.out, f"{name}.png"))

    with open(os.path.join(args.out, "palette.txt"), "w", encoding="utf-8") as f:
        f.write(f"background={pal['background']}\ntext={pal['text']}\naccent={pal['dots']}\n"
                f"disc={pal['disc']}\nintro_frames={intro}\nloop_frames={loop}\nfps={args.fps}\n")
    print(f"Rendered {intro} intro and {loop} loop frames to {args.out}")


if __name__ == "__main__":
    main()
