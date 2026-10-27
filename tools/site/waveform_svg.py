#!/usr/bin/env python3
"""Writes site/img/break-waveform.svg: a mirrored min/max waveform of the
built-in amen (four bars in sequence). Stdlib only.
Usage: python3 tools/site/waveform_svg.py [bins]"""
import struct, sys, wave
from pathlib import Path

root = Path(__file__).resolve().parents[2]
bins = int(sys.argv[1]) if len(sys.argv) > 1 else 400
peaks = []
for n in range(1, 5):
    with wave.open(str(root / f"sample/174_JunglizerAmen_Bar_0{n}.wav")) as w:
        ch, width, frames = w.getnchannels(), w.getsampwidth(), w.getnframes()
        raw = w.readframes(frames)
    assert width in (2, 3, 4)
    if width == 2:
        data = struct.unpack("<%dh" % (len(raw) // 2), raw); scale = 32768
    elif width == 4:
        data = struct.unpack("<%di" % (len(raw) // 4), raw); scale = 2**31
    else:
        data = [int.from_bytes(raw[i:i+3], "little", signed=True) for i in range(0, len(raw), 3)]; scale = 2**23
    mono = [max(abs(x) for x in data[i:i+ch]) / scale for i in range(0, len(data), ch)]
    per = bins // 4
    for b in range(per):
        seg = mono[b * len(mono) // per:(b + 1) * len(mono) // per]
        peaks.append(max(seg) if seg else 0)
top = max(peaks) or 1
H = 100  # half height
pts = [(i, round(H * p / top ** 1 * 0.98 ** 0, 1)) for i, p in enumerate(peaks)]
# mild compression so quiet tails stay visible
pts = [(i, round(H * (p / top) ** 0.6, 1)) for i, p in enumerate(peaks)]
up = "M0 %d" % H + "".join("L%d %s" % (i, f"{H - h:g}") for i, h in pts) + "L%d %d" % (len(pts) - 1, H)
down = "".join("L%d %s" % (i, f"{H + h:g}") for i, h in reversed(pts)) + "Z"
svg = ('<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 %d %d" preserveAspectRatio="none">'
       '<path fill="#abb29a" d="%s%s"/></svg>\n') % (len(pts) - 1, 2 * H, up, down)
out = root / "site/img/break-waveform.svg"
out.write_text(svg)
print(out, len(svg), "bytes")
