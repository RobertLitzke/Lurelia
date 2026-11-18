#!/usr/bin/env python3
"""Count warm pixels in a screenshot, to settle a colour claim with a number.

A bot ten metres away is a small figure, and "I think the lamp is amber" is
not evidence. This reads a PNG the game wrote, counts the pixels in a box
that are bright and clearly more red than blue, and prints the strongest one,
so two people looking at the same frame get the same answer.

    python tools/warm.py shot.png                 the whole frame
    python tools/warm.py shot.png 880 320 1030 560    a box round the subject
    python tools/warm.py shot.png --cool           count cold pixels instead

Pick a subject whose own colour cannot be confused with what is being
measured: a warm lamp on an amber bot proves nothing, so measure the one
with the cold team colour (docs/BOTS.md, "Watching bots headlessly").

Stdlib only, like everything else in here.
"""

import sys
import zlib
from struct import unpack

# Bright enough to be a light rather than a shadow, and clearly on one side
# of neutral rather than merely off-white.
BRIGHT = 140
SPLIT = 55
TINT = 22


def load(path):
    """A PNG as (width, height, channels, pixels), filters undone."""
    d = open(path, "rb").read()
    assert d[:8] == b"\x89PNG\r\n\x1a\n", f"{path} is not a PNG"
    i, w, h, bits, colour, idat = 8, 0, 0, 0, 0, b""
    while i < len(d):
        ln = unpack(">I", d[i : i + 4])[0]
        typ = d[i + 4 : i + 8]
        if typ == b"IHDR":
            w, h = unpack(">II", d[i + 8 : i + 16])
            bits, colour = d[i + 16], d[i + 17]
        elif typ == b"IDAT":
            idat += d[i + 8 : i + 8 + ln]
        i += 12 + ln
    assert bits == 8, f"{bits} bits a channel, expected 8"
    ch = {0: 1, 2: 3, 4: 2, 6: 4}[colour]
    raw = zlib.decompress(idat)
    stride = w * ch
    out = bytearray(stride * h)
    prev = bytearray(stride)
    pos = 0
    for y in range(h):
        f = raw[pos]
        pos += 1
        line = bytearray(raw[pos : pos + stride])
        pos += stride
        # The five PNG filters, undone in place against the row above.
        if f:
            for x in range(stride):
                a = line[x - ch] if x >= ch else 0
                b = prev[x]
                c = prev[x - ch] if x >= ch else 0
                if f == 1:
                    line[x] = (line[x] + a) & 255
                elif f == 2:
                    line[x] = (line[x] + b) & 255
                elif f == 3:
                    line[x] = (line[x] + (a + b) // 2) & 255
                elif f == 4:
                    p = a + b - c
                    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                    line[x] = (line[x] + (a if (pa <= pb and pa <= pc) else (b if pb <= pc else c))) & 255
                else:
                    raise AssertionError(f"filter {f} on row {y}")
        out[y * stride : (y + 1) * stride] = line
        prev = line
    return w, h, ch, out


def count(path, box=None, cool=False):
    w, h, ch, px = load(path)
    x0, y0, x1, y1 = box or (0, 0, w, h)
    x0, y0 = max(0, x0), max(0, y0)
    x1, y1 = min(w, x1), min(h, y1)
    hits, seen, best = 0, 0, None
    for y in range(y0, y1):
        for x in range(x0, x1):
            o = (y * w + x) * ch
            r, g, b = px[o], px[o + 1], px[o + 2]
            seen += 1
            if max(r, g, b) < BRIGHT:
                continue
            # Warm is red over both the others. Cold is anything clearly
            # away from red: cyan is as green as it is blue, so asking for
            # blue over green would reject the one colour this is for.
            if cool:
                gap, tint = min(g, b) - r, max(g, b) - r
            else:
                gap, tint = r - b, r - g
            if gap > SPLIT and tint > TINT:
                hits += 1
                if best is None or gap > best[5]:
                    best = (x, y, r, g, b, gap)
    kind = "cold" if cool else "warm"
    print(f"{path}: {hits} {kind} of {seen} pixels in {x0},{y0}..{x1},{y1}")
    if best:
        x, y, r, g, b, gap = best
        print(f"  strongest at ({x},{y}) rgb {r},{g},{b}")
    return hits


def main(argv):
    args = [a for a in argv if a != "--cool"]
    if not args:
        print(__doc__)
        return 2
    box = tuple(int(v) for v in args[1:5]) if len(args) >= 5 else None
    count(args[0], box, "--cool" in argv)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
