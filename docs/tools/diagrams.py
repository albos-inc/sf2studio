#!/usr/bin/env python3
"""The guide's diagrams: `python3 docs/tools/diagrams.py` writes
docs/images/<en|ja>/diagram-*.svg from the drawings and words below.

Every diagram is drawn once; only the words differ between the languages.
"""

import math
import os
from html import escape

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "images")

FONT = ("-apple-system, BlinkMacSystemFont, 'Hiragino Sans', 'Hiragino Kaku Gothic ProN', "
        "'Yu Gothic UI', 'Yu Gothic', Meiryo, 'Segoe UI', 'Noto Sans CJK JP', sans-serif")
INK = "#1f2430"
MUTED = "#687082"
LINE = "#d9dde5"
PANEL = "#f4f6f9"
ACCENT = "#4084ff"
LANE_A = "#5aaaff"
LANE_B = "#ffa03c"
LANE_C = "#6ed278"
RED = "#e8345c"
YELLOW = "#ffc83c"
DARK = "#16181d"
# The spectrogram's colors, quiet to loud.
HEAT = ["#000004", "#3c0f6e", "#b43264", "#fa8c28", "#fcfabe"]


class Svg:
    def __init__(self, width, height):
        self.width, self.height = width, height
        self.parts = []

    def add(self, s):
        self.parts.append(s)

    def rect(self, x, y, w, h, fill="none", stroke="none", rx=8, sw=1.5, extra=""):
        self.add(f'<rect x="{x:.1f}" y="{y:.1f}" width="{w:.1f}" height="{h:.1f}" rx="{rx}" '
                 f'fill="{fill}" stroke="{stroke}" stroke-width="{sw}" {extra}/>')

    def line(self, x1, y1, x2, y2, stroke=INK, sw=1.5, dash=None, arrow=False):
        d = f' stroke-dasharray="{dash}"' if dash else ""
        a = ' marker-end="url(#arrow)"' if arrow else ""
        self.add(f'<line x1="{x1:.1f}" y1="{y1:.1f}" x2="{x2:.1f}" y2="{y2:.1f}" stroke="{stroke}" '
                 f'stroke-width="{sw}"{d}{a} stroke-linecap="round"/>')

    def path(self, d, stroke=INK, sw=2, fill="none", dash=None, arrow=False, extra=""):
        ds = f' stroke-dasharray="{dash}"' if dash else ""
        a = ' marker-end="url(#arrow)"' if arrow else ""
        self.add(f'<path d="{d}" stroke="{stroke}" stroke-width="{sw}" fill="{fill}"{ds}{a} '
                 f'stroke-linecap="round" stroke-linejoin="round" {extra}/>')

    def polyline(self, points, stroke=INK, sw=2, dash=None):
        d = "M" + " L".join(f"{x:.1f},{y:.1f}" for x, y in points)
        self.path(d, stroke=stroke, sw=sw, dash=dash)

    def text(self, x, y, s, size=14, weight=400, fill=INK, anchor="start", italic=False):
        lines = s if isinstance(s, list) else [s]
        style = ' font-style="italic"' if italic else ""
        for i, line in enumerate(lines):
            self.add(f'<text x="{x:.1f}" y="{y + i * size * 1.35:.1f}" font-size="{size}" '
                     f'font-weight="{weight}" fill="{fill}" text-anchor="{anchor}"{style}>{escape(line)}</text>')

    def badge(self, x, y, label, fill=RED):
        self.add(f'<circle cx="{x}" cy="{y}" r="12" fill="{fill}" stroke="#fff" stroke-width="2"/>')
        self.text(x, y + 5, str(label), size=13, weight=700, fill="#fff", anchor="middle")

    def svg(self):
        defs = (
            '<defs>'
            '<marker id="arrow" viewBox="0 0 10 10" refX="8" refY="5" markerWidth="7" markerHeight="7" '
            f'orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" fill="{MUTED}"/></marker>'
            '<linearGradient id="heat" x1="0" y1="1" x2="0" y2="0">'
            + "".join(f'<stop offset="{i / (len(HEAT) - 1):.2f}" stop-color="{c}"/>' for i, c in enumerate(HEAT))
            + '</linearGradient>'
            '<linearGradient id="heat-x" x1="0" y1="0" x2="1" y2="0">'
            + "".join(f'<stop offset="{i / (len(HEAT) - 1):.2f}" stop-color="{c}"/>' for i, c in enumerate(HEAT))
            + '</linearGradient>'
            '<linearGradient id="press" x1="0" y1="0" x2="0" y2="1">'
            '<stop offset="0" stop-color="#4084ff" stop-opacity="0.08"/>'
            '<stop offset="1" stop-color="#4084ff" stop-opacity="0.75"/></linearGradient>'
            '</defs>'
        )
        body = "\n".join(self.parts)
        return (
            f'<svg xmlns="http://www.w3.org/2000/svg" width="{self.width}" height="{self.height}" '
            f'viewBox="0 0 {self.width} {self.height}" font-family="{FONT}">\n{defs}\n'
            f'<rect x="0.5" y="0.5" width="{self.width - 1}" height="{self.height - 1}" rx="16" fill="#ffffff" '
            f'stroke="{LINE}"/>\n{body}\n</svg>\n'
        )


def card(svg, x, y, w, h, title, lines, color=ACCENT, size=13.5):
    svg.rect(x, y, w, h, fill=PANEL, stroke=LINE, rx=12)
    svg.rect(x, y, 6, h, fill=color, rx=3)
    svg.text(x + 20, y + 30, title, size=16, weight=700)
    yy = y + 58
    for line in lines:
        svg.add(f'<circle cx="{x + 24}" cy="{yy - 4.5}" r="2.5" fill="{MUTED}"/>')
        svg.text(x + 34, yy, line, size=size, fill=INK)
        yy += size * 1.7


# MARK: - Diagrams


def modes(t):
    s = Svg(900, 360)
    s.text(30, 40, t["title"], size=19, weight=700)
    xs = [30, 320, 610]
    colors = ["#9b6bff", ACCENT, "#22a06b"]
    for i, key in enumerate(["create", "tune", "compare"]):
        title, lines = t[key]
        card(s, xs[i], 64, 260, 190, title, lines, color=colors[i])
    # Create → Compare.
    s.path("M160,256 C160,320 740,320 740,258", stroke=MUTED, sw=2, arrow=True, dash="6 5")
    s.text(450, 335, t["send"], size=13, fill=MUTED, anchor="middle")
    # Tune ⇄ Compare.
    s.line(582, 150, 606, 150, stroke=MUTED, sw=2, arrow=True)
    s.line(608, 166, 584, 166, stroke=MUTED, sw=2, arrow=True)
    s.text(595, 192, t["shared"], size=11, fill=MUTED, anchor="middle")
    return s


def lane(t):
    s = Svg(900, 400)
    s.text(30, 40, t["title"], size=19, weight=700)
    # The lane's three choices.
    s.rect(30, 62, 330, 300, fill=PANEL, stroke=LANE_A, rx=12, sw=2)
    s.text(50, 94, "A", size=22, weight=700, fill=LANE_A)
    rows = [("synth", 84), ("sound", 190), ("preset", 274)]
    for key, y in rows:
        label, options = t[key]
        s.text(80, y + 8, label, size=15, weight=700)
        for i, option in enumerate(options):
            s.rect(80, y + 18 + i * 22, 262, 20, fill="#fff", stroke=LINE, rx=5, sw=1)
            s.text(90, y + 33 + i * 22, option, size=12, fill=INK)
    s.text(40, 385, t["wav_note"], size=11.5, fill=MUTED)
    # It renders what is played.
    s.line(372, 194, 470, 194, stroke=MUTED, sw=2, arrow=True)
    s.text(421, 170, t["renders"], size=12, fill=MUTED, anchor="middle")
    s.rect(380, 206, 84, 64, fill="#fff", stroke=LINE, rx=8, sw=1)
    s.text(422, 226, t["play"], size=11.5, fill=INK, anchor="middle")
    # The lane as drawn on the timeline.
    x0, y0, w = 480, 74, 390
    s.rect(x0, y0, w, 24, fill="#2d4a6b", rx=4)
    s.text(x0 + 10, y0 + 17, "▶ A  🔊  sf2synth · piano.sf2 · 0:0", size=12.5, fill="#fff")
    s.rect(x0, y0 + 24, w, 132, fill=DARK, rx=0)
    for n in range(1, 9):
        y = y0 + 24 + 120 - math.log(n) / math.log(9) * 100
        for k in range(4):
            sx = x0 + 18 + k * 92
            s.add(f'<rect x="{sx}" y="{y - 2:.1f}" width="{70 - n * 5}" height="4" fill="url(#heat-x)" '
                  f'opacity="{1 - n * 0.09:.2f}"/>')
    pts = []
    for k in range(4):
        sx = x0 + 18 + k * 92
        pts += [(sx, y0 + 150), (sx + 3, y0 + 50 + k * 4), (sx + 66, y0 + 80 + k * 4), (sx + 74, y0 + 150)]
    s.polyline(pts, stroke=LANE_A, sw=2)
    s.text(x0 + w / 2, y0 + 182, t["info"], size=12, fill=MUTED, anchor="middle")
    s.text(x0 + w / 2, y0 + 206, t["shows"], size=12.5, fill=INK, anchor="middle")
    s.text(x0 + w / 2, y0 + 228, t["shows2"], size=12.5, fill=INK, anchor="middle")
    return s


def keyboard(t):
    s = Svg(900, 330)
    s.text(30, 40, t["title"], size=19, weight=700)
    kx, ky, kw, kh = 130, 70, 64, 220
    for i in range(5):
        fill = "url(#press)" if i == 2 else "#fbfbfc"
        s.rect(kx + i * kw, ky, kw - 2, kh, fill="#fbfbfc", stroke="#c9ced8", rx=4, sw=1)
        if i == 2:
            s.rect(kx + i * kw, ky, kw - 2, kh, fill=fill, rx=4)
    for i in [0, 1, 3]:
        s.rect(kx + (i + 1) * kw - 20, ky, 38, 136, fill="#22252b", rx=3)
    # Where the press lands.
    s.line(kx + 2 * kw + 4, ky + 176, kx + 3 * kw - 6, ky + 176, stroke=ACCENT, sw=2.5)
    s.rect(kx + 3 * kw + 6, ky + 150, 104, 26, fill="#000000cc", rx=4)
    s.text(kx + 3 * kw + 14, ky + 168, "E4  vel 102", size=13, fill="#fff")
    s.text(kx + 2 * kw + 31, ky - 8, t["soft"], size=13, fill=MUTED, anchor="middle")
    s.text(kx + 2 * kw + 31, ky + kh + 22, t["loud"], size=13, weight=700, fill=ACCENT, anchor="middle")
    # The last note's bar.
    s.rect(kx + 4, ky + kh - 150, 4, 150, fill=YELLOW, rx=1)
    s.line(kx - 6, ky + kh - 150, kx - 6, ky + kh, stroke=YELLOW, sw=2)
    s.text(kx - 12, ky + kh - 70, t["bar"], size=11.5, fill=MUTED, anchor="end")
    # The rules.
    x = 500
    for i, (title, body) in enumerate(t["rules"]):
        y = 82 + i * 60
        s.badge(x, y - 5, i + 1, fill=ACCENT)
        s.text(x + 22, y, title, size=14.5, weight=700)
        s.text(x + 22, y + 21, body, size=12.5, fill=MUTED)
    return s


def velocity_curves(t):
    s = Svg(900, 380)
    s.text(30, 40, t["title"], size=19, weight=700)
    px, py, pw, ph = 80, 70, 470, 250
    s.rect(px, py, pw, ph, fill=PANEL, stroke=LINE, rx=4, sw=1)
    floor = -60.0

    def to_xy(v, db):
        return px + v / 127 * pw, py + (max(db, floor) / floor) * ph

    for db in range(0, -61, -12):
        _, y = to_xy(0, db)
        s.line(px, y, px + pw, y, stroke=LINE, sw=1)
        s.text(px - 8, y + 4, f"{db}", size=11, fill=MUTED, anchor="end")
    for v in [0, 32, 64, 96, 127]:
        x, _ = to_xy(v, 0)
        s.line(x, py, x, py + ph, stroke=LINE, sw=1)
        s.text(x, py + ph + 18, str(v), size=11, fill=MUTED, anchor="middle")
    s.text(px + pw / 2, py + ph + 40, t["x"], size=12.5, fill=INK, anchor="middle")
    s.add(f'<text x="30" y="{py + ph / 2}" font-size="12.5" fill="{INK}" text-anchor="middle" '
          f'transform="rotate(-90 30 {py + ph / 2})">{escape(t["y"])}</text>')

    def concave(x):
        if x >= 1:
            return 1.0
        if x <= 0:
            return 0.0
        return min(1.0, -(400 / 960) * math.log10(1 - x))

    curves = [
        (lambda v: concave(1 - v / 127), 96, ACCENT, None),
        (lambda v: 1 - v / 127, 96, "#8a93a3", None),
        (lambda v: 1 - concave(v / 127), 96, LANE_B, None),
        (lambda v: concave(1 - v / 127), 48, ACCENT, "6 5"),
    ]
    for f, range_db, color, dash in curves:
        pts = [to_xy(v, -range_db * f(v)) for v in [i / 4 for i in range(4, 509)]]
        s.polyline(pts, stroke=color, sw=2.6, dash=dash)
    x = 590
    for i, (color, dash, title, body) in enumerate(zip(
            [ACCENT, "#8a93a3", LANE_B, ACCENT], [None, None, None, "6 5"], t["names"], t["notes"])):
        y = 82 + i * 70
        s.line(x, y - 5, x + 34, y - 5, stroke=color, sw=3, dash=dash)
        s.text(x + 44, y, title, size=14, weight=700)
        s.text(x + 44, y + 20, body, size=12, fill=MUTED)
    return s


def level_matching(t):
    s = Svg(900, 320)
    s.text(30, 40, t["title"], size=19, weight=700)
    base = 250

    def bars(x0, heights, labels):
        for i, (h, color, label) in enumerate(zip(heights, [LANE_A, LANE_B], labels)):
            x = x0 + i * 90
            s.rect(x, base - h, 60, h, fill=color, rx=6)
            s.text(x + 30, base + 22, "AB"[i], size=15, weight=700, fill=color, anchor="middle")
            s.text(x + 30, base - h - 10, label, size=12.5, fill=INK, anchor="middle")

    s.text(80, 80, t["before"], size=14, weight=700)
    bars(80, [110, 150], ["-41.7 dB", "-36.5 dB"])
    s.line(290, 180, 400, 180, stroke=MUTED, sw=2, arrow=True)
    s.text(345, 166, t["switch"], size=13, fill=MUTED, anchor="middle")
    s.text(440, 80, t["after"], size=14, weight=700)
    bars(440, [110, 110], [t["gain_a"], t["gain_b"]])
    s.line(430, base - 110, 640, base - 110, stroke=RED, sw=1.5, dash="5 4")
    s.text(660, 145, t["note1"], size=12.5, fill=INK)
    s.text(660, 210, t["note2"], size=12.5, fill=MUTED)
    return s


def spectrogram(t):
    s = Svg(900, 420)
    s.text(30, 40, t["title"], size=19, weight=700)
    x0, y0, w, h = 100, 70, 430, 270
    s.rect(x0, y0, w, h, fill=DARK, rx=4)
    # A note's partials: lower ones louder and longer.
    root = 0.18
    for n in range(1, 13):
        y = y0 + h - (root + math.log(n) / math.log(64)) * h
        if y < y0 + 6:
            break
        length = 320 / (1 + 0.25 * n)
        steps = 24
        for k in range(steps):
            op = max(0.0, 1 - n * 0.07) * (1 - k / steps)
            s.add(f'<rect x="{x0 + 40 + k * length / steps:.1f}" y="{y - 2.5:.1f}" width="{length / steps + 0.5:.1f}" '
                  f'height="5" fill="{HEAT[3] if n < 4 else HEAT[2]}" opacity="{op:.2f}"/>')
        if n <= 8:
            s.add(f'<line x1="{x0 + 4}" y1="{y:.1f}" x2="{x0 + w - 30}" y2="{y:.1f}" stroke="#78ffc8" '
                  f'stroke-opacity="{0.55 if n == 1 else 0.22}" stroke-width="1"/>')
            s.text(x0 + 6, y - 3, "262 Hz" if n == 1 else f"×{n}", size=10, fill="#78ffc8")
    # A hammer's noise at the start.
    s.add(f'<rect x="{x0 + 38}" y="{y0 + 8}" width="8" height="{h - 16}" fill="{HEAT[2]}" opacity="0.5"/>')
    # Level line.
    pts = [(x0 + 30, y0 + h - 4), (x0 + 42, y0 + 70), (x0 + 110, y0 + 96), (x0 + 260, y0 + 128), (x0 + 285, y0 + 200),
           (x0 + 310, y0 + h - 4), (x0 + w, y0 + h - 4)]
    s.polyline(pts, stroke=LANE_A, sw=2)
    for db in [-12, -24, -48]:
        y = y0 + h - (db + 72) / 72 * h
        s.text(x0 + w - 6, y + 4, str(db), size=10, fill="#9aa0aa", anchor="end")
    # Playhead and loop.
    s.add(f'<rect x="{x0 + 200}" y="{y0}" width="110" height="{h}" fill="#ffffff" opacity="0.09"/>')
    s.line(x0 + 230, y0, x0 + 230, y0 + h, stroke="#fff", sw=1.8)
    # Axes.
    s.line(x0, y0 + h + 22, x0 + w, y0 + h + 22, stroke=MUTED, sw=1.5, arrow=True)
    s.text(x0 + w / 2, y0 + h + 42, t["time"], size=12.5, fill=INK, anchor="middle")
    s.line(x0 - 22, y0 + h, x0 - 22, y0, stroke=MUTED, sw=1.5, arrow=True)
    s.add(f'<text x="{x0 - 34}" y="{y0 + h / 2}" font-size="12.5" fill="{INK}" text-anchor="middle" '
          f'transform="rotate(-90 {x0 - 34} {y0 + h / 2})">{escape(t["freq"])}</text>')
    # Color scale.
    s.add(f'<rect x="{x0 + w + 20}" y="{y0}" width="16" height="{h}" fill="url(#heat)" rx="3"/>')
    s.text(x0 + w + 44, y0 + 12, t["loud"], size=12, fill=INK)
    s.text(x0 + w + 44, y0 + h, t["quiet"], size=12, fill=INK)
    # Callouts.
    notes = t["notes"]
    marks = [(x0 + 60, y0 + h - root * h), (x0 + 6, y0 + 40), (x0 + 140, y0 + 100), (x0 + w - 16, y0 + 130),
             (x0 + 255, y0 + 20)]
    for i, (mx, my) in enumerate(marks):
        s.badge(mx, my, i + 1)
    ty = 92
    for i, note in enumerate(notes):
        s.badge(x0 + w + 100, ty - 5, i + 1)
        s.text(x0 + w + 120, ty, note, size=12, fill=INK)
        ty += 20 * (len(note) if isinstance(note, list) else 1) + 16
    return s


def envelope(t):
    s = Svg(900, 330)
    s.text(30, 40, t["title"], size=19, weight=700)
    x0, y0, w, h = 70, 70, 560, 200
    s.rect(x0, y0, w, h, fill=PANEL, stroke=LINE, rx=4, sw=1)
    down, up = x0 + 40, x0 + 380
    s.line(down, y0, down, y0 + h, stroke=MUTED, sw=1, dash="3 4")
    s.line(up, y0, up, y0 + h, stroke=MUTED, sw=1, dash="3 4")
    s.text(down, y0 + h + 20, t["down"], size=12, fill=MUTED, anchor="middle")
    s.text(up, y0 + h + 20, t["up"], size=12, fill=MUTED, anchor="middle")
    top = y0 + 20
    bottom = y0 + h - 4
    # The sample as it is.
    s.polyline([(down, bottom), (down + 2, top), (up, top + 60), (up + 20, bottom)], stroke="#9aa0aa", sw=2, dash="6 5")
    # Shaped.
    s.polyline([(down, bottom), (down + 50, top), (up, top + 100), (up + 150, bottom)], stroke=ACCENT, sw=3)
    s.badge(down + 25, top - 4, 1)
    s.badge(down + 190, top + 34, 2)
    s.badge(up + 90, top + 100, 3)
    x = 660
    for i, (title, body) in enumerate(t["items"]):
        y = 92 + i * 66
        s.badge(x, y - 5, i + 1)
        s.text(x + 22, y, title, size=14, weight=700)
        s.text(x + 22, y + 20, body, size=12, fill=MUTED)
    s.line(x0 + 20, y0 + h + 44, x0 + 54, y0 + h + 44, stroke="#9aa0aa", sw=2, dash="6 5")
    s.text(x0 + 62, y0 + h + 48, t["original"], size=12, fill=MUTED)
    s.line(x0 + 260, y0 + h + 44, x0 + 294, y0 + h + 44, stroke=ACCENT, sw=3)
    s.text(x0 + 302, y0 + h + 48, t["shaped"], size=12, fill=MUTED)
    return s


def velocity_layers(t):
    s = Svg(900, 300)
    s.text(30, 40, t["title"], size=19, weight=700)
    x0, y0, w = 60, 100, 780
    layers = [(1, 42), (43, 84), (85, 127)]
    cutoffs = t["cutoffs"]
    shades = ["#3a4a66", "#6d86b5", "#a9c4ff"]
    for i, (lo, hi) in enumerate(layers):
        xa = x0 + (lo - 1) / 126 * w
        xb = x0 + (hi - 1) / 126 * w
        s.rect(xa + 2, y0, xb - xa - 4, 54, fill=shades[i], rx=8)
        s.text((xa + xb) / 2, y0 + 24, f"v{i + 1}: {lo}–{hi}", size=14, weight=700, fill="#fff", anchor="middle")
        s.text((xa + xb) / 2, y0 + 44, cutoffs[i], size=12, fill="#fff", anchor="middle")
    s.line(x0, y0 + 74, x0 + w, y0 + 74, stroke=MUTED, sw=1.5, arrow=True)
    s.text(x0, y0 + 94, t["soft"], size=12, fill=MUTED)
    s.text(x0 + w, y0 + 94, t["loud"], size=12, fill=MUTED, anchor="end")
    s.text(x0, y0 + 130, t["note1"], size=12.5, fill=INK)
    s.text(x0, y0 + 152, t["note2"], size=12.5, fill=MUTED)
    s.text(x0, y0 + 174, t["note3"], size=12.5, fill=MUTED)
    return s


def stretch(t):
    s = Svg(900, 330)
    s.text(30, 40, t["title"], size=19, weight=700)
    px, py, pw, ph = 80, 70, 520, 200
    s.rect(px, py, pw, ph, fill=PANEL, stroke=LINE, rx=4, sw=1)
    mid = py + ph / 2
    s.line(px, mid, px + pw, mid, stroke=MUTED, sw=1)
    amount = 15.0

    def to_xy(key, cents):
        return px + (key - 21) / 87 * pw, mid - cents / 20 * (ph / 2)

    for cents in [-15, 0, 15]:
        _, y = to_xy(21, cents)
        s.text(px - 8, y + 4, f"{cents:+d}" if cents else "0", size=11, fill=MUTED, anchor="end")
    for key, name in [(21, "A0"), (36, "C2"), (60, "C4"), (84, "C6"), (108, "C8")]:
        x, _ = to_xy(key, 0)
        s.line(x, py + ph, x, py + ph + 5, stroke=MUTED, sw=1)
        s.text(x, py + ph + 20, name, size=11, fill=MUTED, anchor="middle")
    pts = []
    for key in range(21, 109):
        d = max(-1.0, min(1.0, (key - 60) / 48))
        pts.append(to_xy(key, amount * d * abs(d)))
    s.polyline(pts, stroke=ACCENT, sw=3)
    pts = [(x, 2 * mid - y) for x, y in pts]
    s.polyline(pts, stroke=LANE_B, sw=2, dash="6 5")
    s.text(px + pw / 2, py + ph + 42, t["x"], size=12.5, fill=INK, anchor="middle")
    s.add(f'<text x="34" y="{mid}" font-size="12.5" fill="{INK}" text-anchor="middle" '
          f'transform="rotate(-90 34 {mid})">{escape(t["y"])}</text>')
    x = 630
    for i, (color, dash, title, body) in enumerate(zip([ACCENT, LANE_B], [None, "6 5"], t["names"], t["notes"])):
        y = 90 + i * 80
        s.line(x, y - 5, x + 30, y - 5, stroke=color, sw=3, dash=dash)
        s.text(x + 40, y, title, size=14, weight=700)
        s.text(x + 40, y + 20, body, size=12, fill=MUTED)
    s.text(630, 250, t["note"], size=12, fill=MUTED)
    return s


def abx(t):
    s = Svg(900, 300)
    s.text(30, 40, t["title"], size=19, weight=700)
    for i, (label, color) in enumerate([("A", LANE_A), ("B", LANE_B)]):
        s.rect(40, 70 + i * 70, 90, 52, fill=color, rx=10)
        s.text(85, 103 + i * 70, f"🔊 {label}", size=18, weight=700, fill="#fff", anchor="middle")
    s.rect(200, 105, 90, 52, fill="#5b6270", rx=10)
    s.text(245, 138, "🔊 X", size=18, weight=700, fill="#fff", anchor="middle")
    s.path("M132,96 C165,96 165,122 196,124", stroke=MUTED, sw=1.5, dash="4 4")
    s.path("M132,166 C165,166 165,140 196,138", stroke=MUTED, sw=1.5, dash="4 4")
    s.text(245, 180, t["hidden"], size=12, fill=MUTED, anchor="middle")
    s.line(300, 131, 360, 131, stroke=MUTED, sw=2, arrow=True)
    s.rect(370, 96, 170, 70, fill=PANEL, stroke=LINE, rx=10)
    s.text(455, 124, t["answer"], size=14, weight=700, anchor="middle")
    s.text(455, 148, t["answer2"], size=12, fill=MUTED, anchor="middle")
    s.line(550, 131, 600, 131, stroke=MUTED, sw=2, arrow=True)
    s.rect(610, 80, 260, 104, fill=PANEL, stroke=LINE, rx=10)
    s.text(626, 108, t["score"], size=14, weight=700)
    s.text(626, 132, t["chance"], size=12.5, fill=INK)
    s.text(626, 156, t["verdict"], size=12.5, fill="#1e8e4e", weight=700)
    s.path("M740,186 C740,240 455,240 455,170", stroke=MUTED, sw=1.5, dash="5 5", arrow=True)
    s.text(600, 250, t["again"], size=12, fill=MUTED, anchor="middle")
    s.text(40, 280, t["rule"], size=12.5, fill=INK)
    return s


def create_flow(t):
    s = Svg(900, 400)
    s.text(30, 40, t["title"], size=19, weight=700)
    heads = t["heads"]
    for x, head in zip([30, 300, 640], heads):
        s.text(x, 74, head, size=13, weight=700, fill=MUTED)
    for i, (title, body) in enumerate(t["origins"]):
        y = 88 + i * 92
        s.rect(30, y, 230, 78, fill=PANEL, stroke=LINE, rx=10)
        s.text(46, y + 28, title, size=14.5, weight=700)
        s.text(46, y + 50, body, size=12, fill=MUTED)
        s.line(262, y + 39, 296, 200, stroke=MUTED, sw=1.5, arrow=True)
    for i, (title, lines) in enumerate(t["shape"]):
        y = 88 + i * 140
        s.rect(300, y, 300, 126, fill="#fff", stroke=ACCENT if i == 0 else "#22a06b", rx=10, sw=2)
        s.text(316, y + 26, title, size=14, weight=700)
        s.text(316, y + 50, lines, size=12, fill=INK)
    s.line(604, 200, 636, 200, stroke=MUTED, sw=2, arrow=True)
    for i, (title, body) in enumerate(t["uses"]):
        y = 88 + i * 92
        s.rect(640, y, 230, 78, fill=PANEL, stroke=LINE, rx=10)
        s.text(656, y + 28, title, size=14.5, weight=700)
        s.text(656, y + 50, body, size=12, fill=MUTED)
    s.text(30, 382, t["note"], size=12, fill=MUTED)
    return s


def tune_loop(t):
    s = Svg(900, 320)
    s.text(30, 40, t["title"], size=19, weight=700)
    xs = [30, 250, 470, 690]
    for i, (title, body) in enumerate(t["steps"]):
        x = xs[i]
        s.rect(x, 70, 190, 150, fill=PANEL, stroke=LINE, rx=12)
        s.badge(x + 24, 96, i + 1, fill=ACCENT)
        s.text(x + 44, 101, title, size=14.5, weight=700)
        s.text(x + 18, 132, body, size=12, fill=INK)
        if i < 3:
            s.line(x + 194, 145, x + 214, 145, stroke=MUTED, sw=2, arrow=True)
    s.path("M785,224 C785,275 125,275 125,226", stroke=MUTED, sw=1.5, dash="5 5", arrow=True)
    s.text(455, 296, t["again"], size=12, fill=MUTED, anchor="middle")
    return s


def recordings(t):
    s = Svg(900, 340)
    s.text(30, 40, t["title"], size=19, weight=700)
    s.text(30, 76, t["names_head"], size=13, weight=700, fill=MUTED)
    for i, (name, key) in enumerate(t["names"]):
        y = 104 + i * 30
        s.rect(30, y - 18, 170, 24, fill=PANEL, stroke=LINE, rx=6, sw=1)
        s.text(40, y, name, size=12.5, fill=INK)
        s.line(206, y - 6, 236, y - 6, stroke=MUTED, sw=1.5, arrow=True)
        s.text(244, y, key, size=12.5, weight=700, fill=INK)
    # Key ranges meet halfway between the roots.
    x0, y0, w = 420, 120, 450
    s.text(x0, 76, t["ranges_head"], size=13, weight=700, fill=MUTED)
    lo, hi = 21, 108

    def kx(key):
        return x0 + (key - lo) / (hi - lo) * w

    s.rect(x0, y0, w, 44, fill="#fbfbfc", stroke="#c9ced8", rx=4, sw=1)
    for key in range(lo, hi + 1):
        if key % 12 in (1, 3, 6, 8, 10):
            s.rect(kx(key) - 2, y0, 4, 26, fill="#2a2d33", rx=1)
    roots = [36, 48, 60, 72, 84]
    edges = [lo] + [(a + b) / 2 for a, b in zip(roots, roots[1:])] + [hi]
    colors = ["#7c5cff", ACCENT, "#22a06b", LANE_B, RED]
    for i, root in enumerate(roots):
        a, b = kx(edges[i] + (0.5 if i else 0)), kx(edges[i + 1] - 0.5 if i < len(roots) - 1 else edges[i + 1])
        s.rect(a, y0 + 54, b - a, 16, fill=colors[i], rx=4)
        s.add(f'<circle cx="{kx(root):.1f}" cy="{y0 + 36}" r="5" fill="{colors[i]}"/>')
        s.text(kx(root), y0 + 92, f"C{root // 12 - 1}", size=12, weight=700, fill=colors[i], anchor="middle")
    s.text(x0, y0 + 124, t["ranges1"], size=12.5, fill=INK)
    s.text(x0, y0 + 146, t["ranges2"], size=12.5, fill=MUTED)
    s.text(x0, y0 + 168, t["ranges3"], size=12.5, fill=MUTED)
    return s


# MARK: - Words

WORDS = {
    "en": {
        "modes": {
            "title": "Three modes",
            "create": ("Create — make an SF2", ["Start from an SF2, recordings", "or the built-in synthesis", "Shape tone, touch, sustain", "Play it, save it"]),
            "tune": ("Tune — play and adjust", ["Play the 88-key keyboard", "See each note in every lane", "Adjust sf2synth as you listen", "Save the settings as TOML"]),
            "compare": ("Compare — side by side", ["Up to 9 lanes of synths,", "SF2 files and recordings", "Switch what you hear live", "Charts and a blind test"]),
            "send": "“Compare with others” puts the new instrument in a lane",
            "shared": ["same", "lanes"],
        },
        "lane": {
            "title": "A lane: what plays, and how it is shown",
            "synth": ("Synth", ["sf2synth", "macOS sampler (macOS only)", "WAV recording"]),
            "sound": ("Sound", ["an SF2 / DLS file", "Same as A · built-in GS set"]),
            "preset": ("Preset", ["bank:program  (e.g. 0:0 Piano 1)"]),
            "wav_note": "A WAV lane plays the file as it is: no Sound or Preset.",
            "renders": "renders",
            "play": ["what is", "played"],
            "info": "level · peak · gain · brightness (left panel)",
            "shows": "spectrogram, level line and close-up",
            "shows2": "on the timeline, in the lane's color",
        },
        "keyboard": {
            "title": "Where you press sets the velocity",
            "soft": "top: soft (1)",
            "loud": "bottom: loud (127)",
            "bar": ["last note:", "bar height =", "velocity"],
            "rules": [
                ("Hover shows key and velocity", "The line and label follow the pointer."),
                ("Hold for as long as you like", "Release to render the note in every lane (Tune)."),
                ("Fixed velocity", "Every press uses the slider's velocity."),
                ("Fixed length", "Every note lasts the set time, held or not."),
            ],
        },
        "curves": {
            "title": "Velocity response: how loud each velocity plays",
            "x": "velocity",
            "y": "level (dB)",
            "names": ["Concave (SF2 default)", "Linear", "Convex", "Concave, range halved"],
            "notes": ["Soft notes stay audible; −12 dB at 64.", "−48 dB at 64 with a 96 dB range.",
                      "Only hard strikes are loud.", "Range 48 dB: soft notes louder."],
        },
        "matching": {
            "title": "Match levels: compare sound, not loudness",
            "before": "Measured level",
            "switch": "Match levels",
            "after": "What you hear",
            "gain_a": "gain +0.0 dB",
            "gain_b": "gain −5.2 dB",
            "note1": ["Every lane is brought down", "to the quietest one, so", "nothing clips."],
            "note2": ["The lane panel shows each", "lane's gain. Turn it off to", "hear the real difference."],
        },
        "spectrogram": {
            "title": "Reading a lane",
            "time": "time →",
            "freq": "frequency (log) →",
            "loud": "loud",
            "quiet": "quiet",
            "notes": [["The played key's fundamental", "(green lines: keyboard notes)"],
                      ["Harmonics ×2, ×3 … above it"],
                      ["Brighter = louder at that", "frequency at that moment"],
                      ["Level line: dB scale on the", "right (top 0 dB, bottom −72)"],
                      ["Loop (shaded) and playhead"]],
        },
        "envelope": {
            "title": "Sustain: attack, fade and release",
            "down": "key down",
            "up": "key up",
            "items": [("Attack", "0 = as is (up to 100 ms)"),
                      ("Fade while held", "0 = the sample's own decay"),
                      ("Release", "0 = as is (up to 5 s)")],
            "original": "the sample as it is",
            "shaped": "with the settings",
        },
        "layers": {
            "title": "Velocity layers: darker copies for softer playing (3 layers)",
            "cutoffs": ["darkest: 2500 Hz", "≈ 6700 Hz", "as recorded"],
            "soft": "velocity 1",
            "loud": "127",
            "note1": "Each layer is a copy of the samples through a low-pass filter; the softest uses “Softest layer cutoff”.",
            "note2": "Layers are only made when one sample covers every velocity (not for files with layers of their own).",
            "note3": "The file grows with each layer.",
        },
        "stretch": {
            "title": "Stretch tuning",
            "x": "key",
            "y": "cents",
            "names": ["+15 cents", "−15 cents"],
            "notes": [["High notes sharp, low notes flat,", "like a tuned piano."], ["The other way round:", "high notes flat, low sharp."]],
            "note": ["Middle C stays in tune; the", "change grows towards the ends."],
        },
        "abx": {
            "title": "The blind test (ABX)",
            "hidden": "A or B, hidden",
            "answer": "“X is A” / “X is B”",
            "answer2": "a new X each time",
            "score": "Right 9 / 10",
            "chance": "chance of guessing this well 1.1%",
            "verdict": "You can tell them apart.",
            "again": "listen again, answer again",
            "rule": "Answer at least 8 times; below 5% chance, the difference is real (you are not just guessing).",
        },
        "create": {
            "title": "How an instrument is made",
            "heads": ["Start from", "Shape", "Use"],
            "origins": [("An SF2 / DLS", "a preset's samples, kept"), ("Recordings", "one WAV per note"),
                        ("Synthesis", "a piano-like tone")],
            "shape": [("Baked into the samples", ["Volume", "Brightness (shelf at 2.5 kHz)", "Warmth (shelf at 200 Hz)",
                                                  "Velocity layers (darker copies)"]),
                      ("Written as SF2 parameters", ["Touch: range, curve, soft notes darker",
                                                     "Sustain: attack, fade, release",
                                                     "Stretch tuning, stereo width, reverb"])],
            "uses": [("Play it", "on the keyboard below"), ("Save SF2…", "a file for any SF2 player"),
                     ("Compare with others", "a lane in Compare")],
            "note": "It is made again 0.4 s after each change. Parameters work in every SF2 player; baked changes are in the sound itself.",
        },
        "tune": {
            "title": "Tune: play, look, adjust",
            "steps": [("Press a key", ["You hear it at once,", "through the lane", "you are tuning."]),
                      ("Release", ["The note is rendered", "in every lane."]),
                      ("Look", ["Spectrogram, harmonics", "and level, lane", "by lane."]),
                      ("Adjust", ["Change a setting:", "the lane renders", "again 0.25 s later."])],
            "again": "play the same key again to hear the change",
        },
        "recordings": {
            "title": "Recordings: one WAV per note",
            "names_head": "The key comes from the file name",
            "names": [("C4.wav", "C4 (60)"), ("F#2-loud.wav", "F♯2"), ("Bb5.wav", "A♯5"), ("piano_060.wav", "C4 (60)"),
                      ("take3.wav", "from the pitch"), ("noise.wav", "error: no key")],
            "ranges_head": "Each note covers the keys around it",
            "ranges1": "Ranges meet halfway between recorded notes;",
            "ranges2": "the lowest and highest reach the ends.",
            "ranges3": "More recordings: less stretching of each one.",
        },
    },
    "ja": {
        "modes": {
            "title": "3 つのモード",
            "create": ("作成 — SF2 を作る", ["元にする音: 既存の SF2・", "録音・内蔵の合成", "音色・タッチ・余韻を整える", "弾いて確かめ、保存する"]),
            "tune": ("調整再生 — 弾いて調整", ["88 鍵の鍵盤を弾く", "1 音ずつ全レーンで見る", "聞きながら sf2synth を調整", "設定を TOML に保存"]),
            "compare": ("比較 — 並べて聞き比べ", ["シンセ・SF2・録音を", "最大 9 レーン並べる", "再生しながら聞く対象を切替", "計測グラフとブラインドテスト"]),
            "send": "「比較に追加」で作った音源がレーンに入る",
            "shared": ["同じ", "レーン"],
        },
        "lane": {
            "title": "レーン: 何を鳴らし、どう表示するか",
            "synth": ("シンセ", ["sf2synth", "Mac 標準（macOS のみ）", "WAV（録音）"]),
            "sound": ("音源", ["SF2 / DLS ファイル", "A と同じ · 内蔵 GS 音源"]),
            "preset": ("音色", ["バンク:プログラム（例 0:0 Piano 1）"]),
            "wav_note": "WAV のレーンはファイルをそのまま鳴らします（音源・音色なし）。",
            "renders": "書き出し",
            "play": ["再生する", "内容"],
            "info": "音量 · ピーク · 補正 · 明るさ（左パネル）",
            "shows": "スペクトログラム・音量推移・拡大波形を",
            "shows2": "タイムラインにレーンの色で表示",
        },
        "keyboard": {
            "title": "押す位置で強さ（ベロシティ）が決まる",
            "soft": "上: 弱い (1)",
            "loud": "下: 強い (127)",
            "bar": ["直前の音:", "バーの高さ =", "ベロシティ"],
            "rules": [
                ("ポインタの位置で鍵盤と強さを表示", "線とラベルがポインタについて動きます。"),
                ("押している間鳴り続ける", "離すと、その音を全レーンで書き出します（調整再生）。"),
                ("強さを固定", "どこを押してもスライダーの強さで鳴ります。"),
                ("長さを固定", "押す長さに関係なく、設定した秒数だけ鳴ります。"),
            ],
        },
        "curves": {
            "title": "ベロシティの反応: 強さごとの音量",
            "x": "ベロシティ",
            "y": "音量 (dB)",
            "names": ["凹型（SF2 既定）", "直線", "凸型", "凹型・幅を半分に"],
            "notes": ["弱音も聞こえやすい。64 で −12 dB。", "幅 96 dB なら 64 で −48 dB。",
                      "強く弾いたときだけ大きい。", "幅 48 dB: 弱音が大きくなる。"],
        },
        "matching": {
            "title": "音量を揃える: 大きさではなく音そのものを比べる",
            "before": "測った音量",
            "switch": "音量を揃える",
            "after": "聞こえる音量",
            "gain_a": "補正 +0.0 dB",
            "gain_b": "補正 −5.2 dB",
            "note1": ["すべてのレーンを一番小さい", "レーンに合わせて下げます。", "音が割れません。"],
            "note2": ["補正量はレーンの欄に表示。", "オフにすると実際の音量差", "のまま聞けます。"],
        },
        "spectrogram": {
            "title": "レーンの読み方",
            "time": "時間 →",
            "freq": "周波数（対数）→",
            "loud": "大",
            "quiet": "小",
            "notes": [["弾いた鍵盤の基音", "（緑の線: 鍵盤で弾いた音のみ）"],
                      ["その上に倍音 ×2、×3 …"],
                      ["明るい色ほど、その時刻・", "その周波数の音が大きい"],
                      ["音量推移の線: 目盛りは右端", "（上 0 dB、下 −72 dB）"],
                      ["ループ範囲（薄い帯）と再生位置"]],
        },
        "envelope": {
            "title": "余韻: 立ち上がり・減衰・離したあと",
            "down": "押す",
            "up": "離す",
            "items": [("立ち上がり", "0 = 元のまま（最大 100 ms）"),
                      ("押したままの減衰", "0 = サンプル自身の減衰のまま"),
                      ("離したあとの余韻", "0 = 元のまま（最大 5 秒）")],
            "original": "元のサンプル",
            "shaped": "設定を反映",
        },
        "layers": {
            "title": "強さのレイヤー: 弱く弾くほど暗い音のコピーを鳴らす（3 レイヤーの例）",
            "cutoffs": ["一番暗い: 2500 Hz", "約 6700 Hz", "元のまま"],
            "soft": "ベロシティ 1",
            "loud": "127",
            "note1": "各レイヤーはサンプルに高域を削るフィルターをかけたコピーです。一番弱い層は「一番弱いレイヤーの高域」を使います。",
            "note2": "1 つのサンプルが全ベロシティを受け持つ場合だけ作ります（元からレイヤーのある音源では作りません）。",
            "note3": "レイヤーを増やすほどファイルは大きくなります。",
        },
        "stretch": {
            "title": "ストレッチ調律",
            "x": "鍵盤",
            "y": "セント",
            "names": ["+15 セント", "−15 セント"],
            "notes": [["高音を高めに、低音を低めに。", "調律されたピアノと同じ傾向。"], ["逆向き: 高音を低めに、", "低音を高めに。"]],
            "note": ["中央の C はそのまま。端に", "行くほど大きくずらします。"],
        },
        "abx": {
            "title": "ブラインドテスト（ABX）",
            "hidden": "A か B（どちらかは秘密）",
            "answer": "「X は A」/「X は B」",
            "answer2": "答えるたびに X を選び直す",
            "score": "正解 9 / 10",
            "chance": "当てずっぽうでこうなる確率 1.1%",
            "verdict": "聞き分けられています。",
            "again": "聞き直して、また答える",
            "rule": "8 回以上答え、確率が 5% 未満なら、たまたまではなく本当に違いが聞こえています。",
        },
        "create": {
            "title": "音源ができるまで",
            "heads": ["元にする音", "整える", "使う"],
            "origins": [("既存の SF2 / DLS", "プリセットのサンプルを使う"), ("録音", "1 音につき 1 つの WAV"),
                        ("合成", "ピアノ風の音を一から")],
            "shape": [("サンプルに焼き込む", ["音量", "明るさ（2.5 kHz 以上を増減）", "温かみ（200 Hz 以下を増減）",
                                             "強さのレイヤー（暗くしたコピー）"]),
                      ("SF2 のパラメータとして書く", ["タッチ: 強弱の差・カーブ・弱音をこもらせる",
                                                     "余韻: 立ち上がり・減衰・離したあと",
                                                     "ストレッチ調律・ステレオの広がり・リバーブ"])],
            "uses": [("弾く", "下の鍵盤で試し弾き"), ("SF2 を保存…", "どの SF2 プレイヤーでも使える"),
                     ("比較に追加", "比較モードのレーンに入る")],
            "note": "変更するたびに 0.4 秒待って作り直します。パラメータはどの SF2 プレイヤーでも効き、焼き込みは音そのものに残ります。",
        },
        "tune": {
            "title": "調整再生: 弾く・見る・直す",
            "steps": [("鍵盤を押す", ["すぐに鳴ります", "（調整中のレーンの", "音で）。"]),
                      ("離す", ["その音を全レーンで", "書き出します。"]),
                      ("見る", ["スペクトログラム・", "倍音・音量を", "レーンごとに比べる。"]),
                      ("直す", ["設定を変えると", "0.25 秒後に", "書き出し直します。"])],
            "again": "同じ鍵盤をもう一度弾いて違いを聞く",
        },
        "recordings": {
            "title": "録音から作る: 1 音につき 1 つの WAV",
            "names_head": "鍵盤はファイル名から読み取る",
            "names": [("C4.wav", "C4 (60)"), ("F#2-loud.wav", "F♯2"), ("Bb5.wav", "A♯5"), ("piano_060.wav", "C4 (60)"),
                      ("take3.wav", "音程から検出"), ("noise.wav", "エラー: 鍵盤不明")],
            "ranges_head": "各音は周りの鍵盤も受け持つ",
            "ranges1": "受け持つ範囲は隣の録音との中間で分けます。",
            "ranges2": "一番低い音・高い音は鍵盤の端まで広げます。",
            "ranges3": "録音が多いほど、1 つの音を引き伸ばす幅が減ります。",
        },
    },
}

DIAGRAMS = [
    ("modes", modes),
    ("lane", lane),
    ("keyboard", keyboard),
    ("curves", velocity_curves),
    ("matching", level_matching),
    ("spectrogram", spectrogram),
    ("envelope", envelope),
    ("layers", velocity_layers),
    ("stretch", stretch),
    ("abx", abx),
    ("create", create_flow),
    ("tune", tune_loop),
    ("recordings", recordings),
]


def main():
    for language, words in WORDS.items():
        directory = os.path.join(OUT, language)
        os.makedirs(directory, exist_ok=True)
        for name, draw in DIAGRAMS:
            path = os.path.join(directory, f"diagram-{name}.svg")
            with open(path, "w", encoding="utf-8") as f:
                f.write(draw(words[name]).svg())
            print(path)


if __name__ == "__main__":
    main()
