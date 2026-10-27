#!/usr/bin/env python3
"""Check a re-rendered amen against the shipped WAVs.

  check_render.py <candidate dir> <shipped dir>

Per bar: identical frame count, every onset within 0.5 ms, and the residual
(candidate minus shipped) RMS at least 30 dB below the shipped RMS. Exits 1 on
any failure. Stdlib plus numpy.
"""
import os
import sys
import wave

import numpy as np

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from onsets import onsets  # noqa: E402

NAME = "174_JunglizerAmen_Bar_{:02d}.wav"
FRAMES = 60828
ONSET_TOL_MS = 0.5
MIN_DB_BELOW = 30.0


def read(path):
    with wave.open(path) as w:
        sw, ch, n = w.getsampwidth(), w.getnchannels(), w.getnframes()
        raw = np.frombuffer(w.readframes(n), dtype=np.uint8)
    b = raw.reshape(-1, sw).astype(np.int32)
    v = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16) if sw == 3 else None
    if sw == 3:
        v = np.where(v >= 1 << 23, v - (1 << 24), v)
        x = v / float(1 << 23)
    else:
        x = np.frombuffer(raw.tobytes(), dtype="<i2") / 32768.0
    return n, x.reshape(-1, ch)


def check_bar(cand, ship):
    nc, xc = read(cand)
    ns, xs = read(ship)
    ok = nc == ns == FRAMES
    res = None
    if nc == ns:
        res = 20 * np.log10(np.sqrt(np.mean(xs ** 2)) / max(np.sqrt(np.mean((xc - xs) ** 2)), 1e-12))
        ok &= res >= MIN_DB_BELOW
    oc = [t for t, _ in onsets(cand)]
    os_ = [t for t, _ in onsets(ship)]
    max_ms = None
    if len(oc) == len(os_):
        max_ms = max((abs(a - b) * 1000 for a, b in zip(oc, os_)), default=0.0)
        ok &= max_ms <= ONSET_TOL_MS
    else:
        ok = False
    return bool(ok), nc, ns, len(oc), len(os_), max_ms, res


def main(cand_dir, ship_dir):
    good = 0
    for b in range(1, 5):
        n = NAME.format(b)
        ok, nc, ns, noc, nos, max_ms, res = check_bar(os.path.join(cand_dir, n), os.path.join(ship_dir, n))
        good += ok
        print(f"bar {b}: {'ok  ' if ok else 'FAIL'} frames {nc}/{ns}, onsets {noc}/{nos}, "
              f"max onset error {'n/a' if max_ms is None else f'{max_ms:.3f} ms'}, "
              f"residual {'n/a' if res is None else f'{res:.1f} dB'} below shipped")
    print(f"{good}/4 bars match")
    return 0 if good == 4 else 1


if __name__ == "__main__":
    if len(sys.argv) != 3:
        sys.exit(__doc__)
    sys.exit(main(*sys.argv[1:]))
